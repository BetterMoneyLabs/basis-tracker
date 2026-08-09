//! Bounded, versioned persistence for the inactive Basis v2 state generation.
//!
//! This module deliberately does not activate v2 routes, scanners, transaction
//! builders, or network behavior.  It only owns the authoritative local bytes
//! needed to reconstruct the fixed-shape tracker and per-reserve AVL roots.

use basis_core::basis_v2::{
    BasisV2Error, ClaimDomainV2, ClaimV2, RedeemedStateV2, ReserveAssetV2, BASIS_V2_ABI_GENERATION,
    BASIS_V2_ERG_ASSET_KIND, BASIS_V2_TOKEN_ASSET_KIND,
};
use basis_trees::{ReserveAvlTree, TrackerAvlTree, TreeError};
use blake2::{Blake2b, Digest};
use fjall::{Config, Keyspace, Partition, PartitionCreateOptions, PersistMode};
use fs2::FileExt;
use generic_array::typenum::U32;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::path::Path;
use thiserror::Error;

const TRACKER_MAGIC: [u8; 4] = *b"BNS2";
const LEGACY_TRACKER_MAGIC: [u8; 4] = *b"BNS1";
const RESERVE_MAGIC: [u8; 4] = *b"BRS2";
const LEGACY_RESERVE_MAGIC: [u8; 4] = *b"BRS1";

const TRACKER_PARTITION: &str = "basis_v2_tracker_claims";
const TRACKER_SNAPSHOT_KEY: &[u8] = b"bns2_snapshot";
const RESERVE_PARTITION: &str = "basis_v2_reserve_redeemed";
const RESERVE_SNAPSHOT_KEY: &[u8] = b"brs2_snapshot";
const WRITER_LOCK_FILE: &str = ".basis-v2-writer.lock";

const TRACKER_CHECKSUM_DOMAIN: &[u8] = b"basis-v2-tracker-claims-bns2";
const RESERVE_CHECKSUM_DOMAIN: &[u8] = b"basis-v2-reserve-redeemed-brs2";

// reserve NFT + tracker NFT + owner + receiver + kind + token/zero + debt + timestamp + signature
const CLAIM_RECORD_LEN: usize = 32 + 32 + 33 + 33 + 1 + 32 + 8 + 8 + 65;
const TRACKER_HEADER_LEN: usize = 4 + 1 + 32 + 4 + 33;
const RESERVE_HEADER_LEN: usize = 4 + 1 + 32 + 32 + 1 + 32 + 4 + 33;
const CHECKSUM_LEN: usize = 32;
const RESERVE_RECORD_LEN: usize = CLAIM_RECORD_LEN + RedeemedStateV2::ENCODED_LEN;
const MAX_V2_ENTRY_COUNT: usize = 50_000;
const MAX_TRACKER_SNAPSHOT_LEN: usize =
    TRACKER_HEADER_LEN + MAX_V2_ENTRY_COUNT * CLAIM_RECORD_LEN + CHECKSUM_LEN;
const MAX_RESERVE_SNAPSHOT_LEN: usize =
    RESERVE_HEADER_LEN + MAX_V2_ENTRY_COUNT * RESERVE_RECORD_LEN + CHECKSUM_LEN;

/// Explicit consent required before an empty v2 generation is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FreshV2StateApproval {
    #[default]
    Reject,
    Approve,
}

/// Immutable lineage and asset binding for one per-reserve BRS2 directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReserveStoreBindingV2 {
    tracker_nft_id: [u8; 32],
    reserve_nft_id: [u8; 32],
    asset: ReserveAssetV2,
}

impl ReserveStoreBindingV2 {
    pub const fn erg(tracker_nft_id: [u8; 32], reserve_nft_id: [u8; 32]) -> Self {
        Self {
            tracker_nft_id,
            reserve_nft_id,
            asset: ReserveAssetV2::Erg,
        }
    }

    pub fn token(
        tracker_nft_id: [u8; 32],
        reserve_nft_id: [u8; 32],
        token_id: [u8; 32],
    ) -> Result<Self, V2StateError> {
        if token_id == reserve_nft_id {
            return Err(V2StateError::Claim(BasisV2Error::DuplicateReserveAssetId));
        }
        Ok(Self {
            tracker_nft_id,
            reserve_nft_id,
            asset: ReserveAssetV2::Token { token_id },
        })
    }

    pub const fn tracker_nft_id(&self) -> [u8; 32] {
        self.tracker_nft_id
    }

    pub const fn reserve_nft_id(&self) -> [u8; 32] {
        self.reserve_nft_id
    }

    pub const fn asset(&self) -> ReserveAssetV2 {
        self.asset
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum V2StateError {
    #[error("a new v2 data directory requires explicit fresh-generation approval")]
    FreshGenerationRequired,
    #[error("explicit migration or reset is required: {0}")]
    MigrationRequired(String),
    #[error("configured v2 lineage or asset does not match the stored binding")]
    BindingMismatch,
    #[error("v2 state is corrupt: {0}")]
    Corrupt(String),
    #[error("v2 state capacity exceeded (limit {limit})")]
    CapacityExceeded { limit: usize },
    #[error("v2 state writer is already active for this path")]
    WriterAlreadyActive,
    #[error("v2 state is terminally poisoned after an unknown write outcome")]
    Poisoned,
    #[error("confirmed transition does not start from the current reserve root")]
    StaleRoot,
    #[error("v2 storage outcome is unknown: {0}")]
    StorageOutcomeUnknown(String),
    #[error("v2 storage error: {0}")]
    Storage(String),
    #[error("invalid v2 claim/state transition: {0}")]
    Claim(#[from] BasisV2Error),
    #[error("v2 AVL reconstruction failed: {0}")]
    Tree(String),
}

impl From<TreeError> for V2StateError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value.to_string())
    }
}

/// One authoritative BNS2 snapshot for all claims bound to one tracker NFT.
pub struct TrackerClaimStoreV2 {
    keyspace: Keyspace,
    partition: Partition,
    _writer_lock: File,
    tracker_nft_id: [u8; 32],
    claims: Vec<ClaimV2>,
    positions: HashMap<[u8; 32], usize>,
    tree: TrackerAvlTree,
    poisoned: bool,
    capacity_limit: usize,
    #[cfg(test)]
    fail_next_persist: bool,
}

impl TrackerClaimStoreV2 {
    pub fn open<P: AsRef<Path>>(
        path: P,
        tracker_nft_id: [u8; 32],
        fresh: FreshV2StateApproval,
    ) -> Result<Self, V2StateError> {
        let opened = open_exact_partition(path.as_ref(), TRACKER_PARTITION, fresh)?;
        let (claims, tree) = if opened.is_fresh {
            let claims = Vec::new();
            let tree = TrackerAvlTree::new();
            let root = tree.root_digest()?;
            let bytes = encode_tracker_snapshot(tracker_nft_id, root, &claims)?;
            initialize_snapshot(
                &opened.keyspace,
                &opened.partition,
                TRACKER_SNAPSHOT_KEY,
                bytes,
            )?;
            (claims, tree)
        } else {
            read_only_snapshot(
                &opened.partition,
                TRACKER_SNAPSHOT_KEY,
                MAX_TRACKER_SNAPSHOT_LEN,
            )
            .and_then(|bytes| decode_tracker_snapshot(&bytes, tracker_nft_id))?
        };
        let positions = index_claims(&claims)?;
        Ok(Self {
            keyspace: opened.keyspace,
            partition: opened.partition,
            _writer_lock: opened.writer_lock,
            tracker_nft_id,
            claims,
            positions,
            tree,
            poisoned: false,
            capacity_limit: MAX_V2_ENTRY_COUNT,
            #[cfg(test)]
            fail_next_persist: false,
        })
    }

    pub fn tracker_nft_id(&self) -> [u8; 32] {
        self.tracker_nft_id
    }

    pub fn len(&self) -> Result<usize, V2StateError> {
        self.ensure_healthy()?;
        Ok(self.claims.len())
    }

    pub fn is_empty(&self) -> Result<bool, V2StateError> {
        self.ensure_healthy()?;
        Ok(self.claims.is_empty())
    }

    pub fn root_digest(&self) -> Result<[u8; 33], V2StateError> {
        self.ensure_healthy()?;
        self.tree.root_digest().map_err(Into::into)
    }

    pub fn claim(&self, claim_key: &[u8; 32]) -> Result<Option<&ClaimV2>, V2StateError> {
        self.ensure_healthy()?;
        Ok(self
            .positions
            .get(claim_key)
            .map(|position| &self.claims[*position]))
    }

    pub fn ordered_claim_keys(&self) -> Result<Vec<[u8; 32]>, V2StateError> {
        self.ensure_healthy()?;
        Ok(self
            .claims
            .iter()
            .map(|claim| claim.domain().claim_key())
            .collect())
    }

    /// Revalidate the complete signed claim and durably replace the snapshot.
    /// Existing keys keep their first-insertion position.
    pub fn record_validated_claim(&mut self, claim: ClaimV2) -> Result<[u8; 33], V2StateError> {
        self.ensure_healthy()?;
        revalidate_claim(&claim)?;
        if claim.domain().tracker_nft_id() != self.tracker_nft_id {
            return Err(V2StateError::BindingMismatch);
        }

        let key = claim.domain().claim_key();
        let mut candidate = self.claims.clone();
        if let Some(position) = self.positions.get(&key).copied() {
            validate_claim_successor(&candidate[position], &claim)?;
            candidate[position] = claim;
        } else {
            if candidate.len() >= self.capacity_limit {
                return Err(V2StateError::CapacityExceeded {
                    limit: self.capacity_limit,
                });
            }
            candidate.push(claim);
        }

        let candidate_positions = index_claims(&candidate)?;
        let candidate_tree = build_tracker_tree(&candidate)?;
        let candidate_root = candidate_tree.root_digest()?;
        let bytes = encode_tracker_snapshot(self.tracker_nft_id, candidate_root, &candidate)?;
        self.replace_snapshot(bytes)?;

        self.claims = candidate;
        self.positions = candidate_positions;
        self.tree = candidate_tree;
        Ok(candidate_root)
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn ensure_healthy(&self) -> Result<(), V2StateError> {
        if self.poisoned {
            Err(V2StateError::Poisoned)
        } else {
            Ok(())
        }
    }

    fn replace_snapshot(&mut self, bytes: Vec<u8>) -> Result<(), V2StateError> {
        if let Err(error) = self.partition.insert(TRACKER_SNAPSHOT_KEY, bytes) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(error.to_string()));
        }
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_persist) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(
                "injected post-insert durability failure".to_string(),
            ));
        }
        if let Err(error) = self.keyspace.persist(PersistMode::SyncData) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(error.to_string()));
        }
        Ok(())
    }
}

/// One authoritative BRS2 snapshot for exactly one reserve NFT lineage.
// The mutation half stays dormant until a confirmed-chain scanner can own its
// call boundary. Keeping it compiled here prevents a route-facing raw setter.
#[allow(dead_code)]
pub struct ReserveRedeemedStoreV2 {
    keyspace: Keyspace,
    partition: Partition,
    _writer_lock: File,
    binding: ReserveStoreBindingV2,
    records: Vec<(ClaimV2, RedeemedStateV2)>,
    positions: HashMap<[u8; 32], usize>,
    tree: ReserveAvlTree,
    poisoned: bool,
    capacity_limit: usize,
    #[cfg(test)]
    fail_next_persist: bool,
}

#[allow(dead_code)]
impl ReserveRedeemedStoreV2 {
    pub fn open<P: AsRef<Path>>(
        path: P,
        binding: ReserveStoreBindingV2,
        fresh: FreshV2StateApproval,
    ) -> Result<Self, V2StateError> {
        let opened = open_exact_partition(path.as_ref(), RESERVE_PARTITION, fresh)?;
        let (records, tree) = if opened.is_fresh {
            let records = Vec::new();
            let tree = ReserveAvlTree::new();
            let root = tree.root_digest()?;
            let bytes = encode_reserve_snapshot(binding, root, &records)?;
            initialize_snapshot(
                &opened.keyspace,
                &opened.partition,
                RESERVE_SNAPSHOT_KEY,
                bytes,
            )?;
            (records, tree)
        } else {
            read_only_snapshot(
                &opened.partition,
                RESERVE_SNAPSHOT_KEY,
                MAX_RESERVE_SNAPSHOT_LEN,
            )
            .and_then(|bytes| decode_reserve_snapshot(&bytes, binding))?
        };
        let positions = index_reserve_records(&records)?;
        Ok(Self {
            keyspace: opened.keyspace,
            partition: opened.partition,
            _writer_lock: opened.writer_lock,
            binding,
            records,
            positions,
            tree,
            poisoned: false,
            capacity_limit: MAX_V2_ENTRY_COUNT,
            #[cfg(test)]
            fail_next_persist: false,
        })
    }

    pub const fn binding(&self) -> ReserveStoreBindingV2 {
        self.binding
    }

    pub fn len(&self) -> Result<usize, V2StateError> {
        self.ensure_healthy()?;
        Ok(self.records.len())
    }

    pub fn root_digest(&self) -> Result<[u8; 33], V2StateError> {
        self.ensure_healthy()?;
        self.tree.root_digest().map_err(Into::into)
    }

    pub fn redeemed_state(
        &self,
        claim_key: &[u8; 32],
    ) -> Result<Option<RedeemedStateV2>, V2StateError> {
        self.ensure_healthy()?;
        Ok(self
            .positions
            .get(claim_key)
            .map(|position| self.records[*position].1))
    }

    pub fn claim(&self, claim_key: &[u8; 32]) -> Result<Option<&ClaimV2>, V2StateError> {
        self.ensure_healthy()?;
        Ok(self
            .positions
            .get(claim_key)
            .map(|position| &self.records[*position].0))
    }

    pub fn ordered_claim_keys(&self) -> Result<Vec<[u8; 32]>, V2StateError> {
        self.ensure_healthy()?;
        Ok(self
            .records
            .iter()
            .map(|(claim, _)| claim.domain().claim_key())
            .collect())
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn ensure_healthy(&self) -> Result<(), V2StateError> {
        if self.poisoned {
            Err(V2StateError::Poisoned)
        } else {
            Ok(())
        }
    }

    // Deliberately private. A later confirmed-chain scanner must own the only
    // call boundary; no route or builder can forge redeemed progress today.
    fn commit_confirmed_redemption(
        &mut self,
        expected_root: [u8; 33],
        claim: ClaimV2,
        amount: u64,
    ) -> Result<[u8; 33], V2StateError> {
        self.ensure_healthy()?;
        if self.tree.root_digest()? != expected_root {
            return Err(V2StateError::StaleRoot);
        }
        revalidate_claim(&claim)?;
        validate_reserve_binding(&claim, self.binding)?;

        let key = claim.domain().claim_key();
        let mut candidate = self.records.clone();
        if let Some(position) = self.positions.get(&key).copied() {
            if candidate[position].0.domain() != claim.domain() {
                return Err(V2StateError::Corrupt(
                    "claim successor changed its authenticated domain".to_string(),
                ));
            }
            let next =
                candidate[position]
                    .1
                    .advance(claim.timestamp(), claim.total_debt(), amount)?;
            candidate[position] = (claim, next);
        } else {
            if candidate.len() >= self.capacity_limit {
                return Err(V2StateError::CapacityExceeded {
                    limit: self.capacity_limit,
                });
            }
            if amount == 0 {
                return Err(V2StateError::Claim(BasisV2Error::InvalidRedemptionAmount));
            }
            if amount > claim.total_debt() {
                return Err(V2StateError::Claim(BasisV2Error::RedemptionExceedsClaim));
            }
            let state = RedeemedStateV2::new(claim.timestamp(), claim.total_debt(), amount)?;
            candidate.push((claim, state));
        }

        let candidate_positions = index_reserve_records(&candidate)?;
        let candidate_tree = build_reserve_tree(&candidate)?;
        let candidate_root = candidate_tree.root_digest()?;
        let bytes = encode_reserve_snapshot(self.binding, candidate_root, &candidate)?;
        self.replace_snapshot(bytes)?;

        self.records = candidate;
        self.positions = candidate_positions;
        self.tree = candidate_tree;
        Ok(candidate_root)
    }

    fn replace_snapshot(&mut self, bytes: Vec<u8>) -> Result<(), V2StateError> {
        if let Err(error) = self.partition.insert(RESERVE_SNAPSHOT_KEY, bytes) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(error.to_string()));
        }
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_persist) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(
                "injected post-insert durability failure".to_string(),
            ));
        }
        if let Err(error) = self.keyspace.persist(PersistMode::SyncData) {
            self.poisoned = true;
            return Err(V2StateError::StorageOutcomeUnknown(error.to_string()));
        }
        Ok(())
    }
}

struct OpenedPartition {
    keyspace: Keyspace,
    partition: Partition,
    writer_lock: File,
    is_fresh: bool,
}

fn open_exact_partition(
    path: &Path,
    expected_partition: &str,
    fresh: FreshV2StateApproval,
) -> Result<OpenedPartition, V2StateError> {
    let existed = path.exists();
    let had_payload = if existed {
        if !path.is_dir() {
            return Err(V2StateError::MigrationRequired(
                "v2 state path exists but is not a directory".to_string(),
            ));
        }
        let mut has_payload = false;
        for entry in
            std::fs::read_dir(path).map_err(|error| V2StateError::Storage(error.to_string()))?
        {
            let entry = entry.map_err(|error| V2StateError::Storage(error.to_string()))?;
            if entry.file_name() != std::ffi::OsStr::new(WRITER_LOCK_FILE) {
                has_payload = true;
                break;
            }
        }
        has_payload
    } else {
        false
    };

    if !had_payload && fresh != FreshV2StateApproval::Approve {
        return Err(V2StateError::FreshGenerationRequired);
    }
    if !existed {
        std::fs::create_dir_all(path).map_err(|error| V2StateError::Storage(error.to_string()))?;
    }

    let writer_lock_path = path.join(WRITER_LOCK_FILE);
    let writer_lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&writer_lock_path)
        .map_err(|error| V2StateError::Storage(error.to_string()))?;
    writer_lock
        .try_lock_exclusive()
        .map_err(|_| V2StateError::WriterAlreadyActive)?;

    let keyspace = Config::new(path)
        .open()
        .map_err(|error| V2StateError::Storage(error.to_string()))?;
    let partitions = keyspace.list_partitions();
    let is_fresh = partitions.is_empty() && !had_payload;

    if is_fresh {
        if fresh != FreshV2StateApproval::Approve {
            return Err(V2StateError::FreshGenerationRequired);
        }
    } else if partitions.is_empty() {
        return Err(V2StateError::MigrationRequired(
            "existing directory has no unambiguous v2 partition".to_string(),
        ));
    } else if partitions.len() != 1 || partitions[0].as_ref() != expected_partition {
        return Err(V2StateError::MigrationRequired(format!(
            "expected only partition {expected_partition}; found {}",
            partitions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    let partition = keyspace
        .open_partition(expected_partition, PartitionCreateOptions::default())
        .map_err(|error| V2StateError::Storage(error.to_string()))?;
    Ok(OpenedPartition {
        keyspace,
        partition,
        writer_lock,
        is_fresh,
    })
}

fn initialize_snapshot(
    keyspace: &Keyspace,
    partition: &Partition,
    key: &[u8],
    bytes: Vec<u8>,
) -> Result<(), V2StateError> {
    partition
        .insert(key, bytes)
        .map_err(|error| V2StateError::StorageOutcomeUnknown(error.to_string()))?;
    keyspace
        .persist(PersistMode::SyncData)
        .map_err(|error| V2StateError::StorageOutcomeUnknown(error.to_string()))
}

fn read_only_snapshot(
    partition: &Partition,
    key: &[u8],
    maximum_len: usize,
) -> Result<Vec<u8>, V2StateError> {
    let len = partition
        .len()
        .map_err(|error| V2StateError::Storage(error.to_string()))?;
    if len != 1 {
        return Err(V2StateError::Corrupt(format!(
            "authoritative partition must contain exactly one row, found {len}"
        )));
    }
    let bytes = partition
        .get(key)
        .map_err(|error| V2StateError::Storage(error.to_string()))?
        .ok_or_else(|| {
            V2StateError::Corrupt("authoritative snapshot row is missing".to_string())
        })?;
    if bytes.len() > maximum_len {
        return Err(V2StateError::Corrupt(
            "authoritative snapshot exceeds its byte bound".to_string(),
        ));
    }
    Ok(bytes.to_vec())
}

fn revalidate_claim(claim: &ClaimV2) -> Result<(), V2StateError> {
    claim.verify()?;
    let reconstructed = ClaimV2::from_signed(
        claim.domain(),
        claim.total_debt(),
        claim.timestamp(),
        *claim.signature(),
    )?;
    if reconstructed != *claim {
        return Err(V2StateError::Corrupt(
            "claim differs from its fully revalidated signed form".to_string(),
        ));
    }
    Ok(())
}

fn validate_claim_successor(previous: &ClaimV2, next: &ClaimV2) -> Result<(), V2StateError> {
    if previous.domain() != next.domain()
        || previous.domain().claim_key() != next.domain().claim_key()
    {
        return Err(V2StateError::Corrupt(
            "claim successor changed its authenticated domain or authoritative key".to_string(),
        ));
    }
    let same =
        next.timestamp() == previous.timestamp() && next.total_debt() == previous.total_debt();
    let monotone =
        next.timestamp() > previous.timestamp() && next.total_debt() >= previous.total_debt();
    if same || monotone {
        Ok(())
    } else {
        Err(V2StateError::Claim(BasisV2Error::ClaimRegression))
    }
}

fn validate_reserve_binding(
    claim: &ClaimV2,
    binding: ReserveStoreBindingV2,
) -> Result<(), V2StateError> {
    let domain = claim.domain();
    if domain.tracker_nft_id() != binding.tracker_nft_id
        || domain.reserve_nft_id() != binding.reserve_nft_id
        || domain.asset() != binding.asset
    {
        return Err(V2StateError::BindingMismatch);
    }
    Ok(())
}

fn index_claims(claims: &[ClaimV2]) -> Result<HashMap<[u8; 32], usize>, V2StateError> {
    let mut positions = HashMap::with_capacity(claims.len());
    for (position, claim) in claims.iter().enumerate() {
        let key = claim.domain().claim_key();
        if positions.insert(key, position).is_some() {
            return Err(V2StateError::Corrupt(
                "duplicate authoritative claim key".to_string(),
            ));
        }
    }
    Ok(positions)
}

fn index_reserve_records(
    records: &[(ClaimV2, RedeemedStateV2)],
) -> Result<HashMap<[u8; 32], usize>, V2StateError> {
    let mut positions = HashMap::with_capacity(records.len());
    for (position, (claim, _)) in records.iter().enumerate() {
        let key = claim.domain().claim_key();
        if positions.insert(key, position).is_some() {
            return Err(V2StateError::Corrupt(
                "duplicate authoritative claim key".to_string(),
            ));
        }
    }
    Ok(positions)
}

fn build_tracker_tree(claims: &[ClaimV2]) -> Result<TrackerAvlTree, V2StateError> {
    if claims.len() > MAX_V2_ENTRY_COUNT {
        return Err(V2StateError::CapacityExceeded {
            limit: MAX_V2_ENTRY_COUNT,
        });
    }
    let mut seen = HashSet::with_capacity(claims.len());
    let mut entries = Vec::with_capacity(claims.len());
    for claim in claims {
        revalidate_claim(claim)?;
        let key = claim.domain().claim_key();
        if !seen.insert(key) {
            return Err(V2StateError::Corrupt(
                "duplicate authoritative claim key".to_string(),
            ));
        }
        entries.push((key, claim.total_debt().to_be_bytes()));
    }
    TrackerAvlTree::from_ordered_entries(entries).map_err(Into::into)
}

fn build_reserve_tree(
    records: &[(ClaimV2, RedeemedStateV2)],
) -> Result<ReserveAvlTree, V2StateError> {
    if records.len() > MAX_V2_ENTRY_COUNT {
        return Err(V2StateError::CapacityExceeded {
            limit: MAX_V2_ENTRY_COUNT,
        });
    }
    let mut seen = HashSet::with_capacity(records.len());
    let mut entries = Vec::with_capacity(records.len());
    for (claim, state) in records {
        revalidate_claim(claim)?;
        if state.timestamp() != claim.timestamp() || state.total_debt() != claim.total_debt() {
            return Err(V2StateError::Corrupt(
                "redeemed state does not match its authenticated claim".to_string(),
            ));
        }
        let key = claim.domain().claim_key();
        if !seen.insert(key) {
            return Err(V2StateError::Corrupt(
                "duplicate authoritative claim key".to_string(),
            ));
        }
        entries.push((key, state.encode()));
    }
    ReserveAvlTree::from_ordered_entries(entries).map_err(Into::into)
}

fn encode_tracker_snapshot(
    tracker_nft_id: [u8; 32],
    root: [u8; 33],
    claims: &[ClaimV2],
) -> Result<Vec<u8>, V2StateError> {
    if claims.len() > MAX_V2_ENTRY_COUNT {
        return Err(V2StateError::CapacityExceeded {
            limit: MAX_V2_ENTRY_COUNT,
        });
    }
    let records_len = checked_records_len(claims.len(), CLAIM_RECORD_LEN)?;
    let capacity = TRACKER_HEADER_LEN
        .checked_add(records_len)
        .and_then(|len| len.checked_add(CHECKSUM_LEN))
        .ok_or_else(|| V2StateError::Corrupt("tracker snapshot length overflow".to_string()))?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&TRACKER_MAGIC);
    bytes.push(BASIS_V2_ABI_GENERATION);
    bytes.extend_from_slice(&tracker_nft_id);
    bytes.extend_from_slice(&(claims.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&root);
    for claim in claims {
        if claim.domain().tracker_nft_id() != tracker_nft_id {
            return Err(V2StateError::BindingMismatch);
        }
        encode_claim(&mut bytes, claim)?;
    }
    append_checksum(&mut bytes, TRACKER_CHECKSUM_DOMAIN);
    debug_assert_eq!(bytes.len(), capacity);
    Ok(bytes)
}

fn decode_tracker_snapshot(
    bytes: &[u8],
    expected_tracker_nft_id: [u8; 32],
) -> Result<(Vec<ClaimV2>, TrackerAvlTree), V2StateError> {
    reject_legacy_or_malformed(
        bytes,
        TRACKER_MAGIC,
        LEGACY_TRACKER_MAGIC,
        TRACKER_HEADER_LEN + CHECKSUM_LEN,
        MAX_TRACKER_SNAPSHOT_LEN,
        "tracker",
    )?;
    let count = read_u32_at(bytes, 4 + 1 + 32)? as usize;
    validate_snapshot_len(
        bytes.len(),
        count,
        TRACKER_HEADER_LEN,
        CLAIM_RECORD_LEN,
        MAX_TRACKER_SNAPSHOT_LEN,
        "tracker",
    )?;
    verify_checksum(bytes, TRACKER_CHECKSUM_DOMAIN)?;

    let payload_len = bytes.len() - CHECKSUM_LEN;
    let mut decoder = Decoder::new(&bytes[..payload_len]);
    if decoder.array::<4>()? != TRACKER_MAGIC || decoder.byte()? != BASIS_V2_ABI_GENERATION {
        return Err(V2StateError::MigrationRequired(
            "unsupported tracker snapshot generation".to_string(),
        ));
    }
    let tracker_nft_id = decoder.array::<32>()?;
    if tracker_nft_id != expected_tracker_nft_id {
        return Err(V2StateError::BindingMismatch);
    }
    let decoded_count = decoder.u32()? as usize;
    let stored_root = decoder.array::<33>()?;
    let mut claims = Vec::with_capacity(decoded_count);
    for _ in 0..decoded_count {
        let claim = decode_claim(&mut decoder)?;
        if claim.domain().tracker_nft_id() != tracker_nft_id {
            return Err(V2StateError::Corrupt(
                "claim is bound to a different tracker NFT".to_string(),
            ));
        }
        claims.push(claim);
    }
    decoder.finish()?;
    index_claims(&claims)?;
    let tree = build_tracker_tree(&claims)?;
    if tree.root_digest()? != stored_root {
        return Err(V2StateError::Corrupt(
            "stored tracker root does not match ordered claims".to_string(),
        ));
    }
    Ok((claims, tree))
}

fn encode_reserve_snapshot(
    binding: ReserveStoreBindingV2,
    root: [u8; 33],
    records: &[(ClaimV2, RedeemedStateV2)],
) -> Result<Vec<u8>, V2StateError> {
    if records.len() > MAX_V2_ENTRY_COUNT {
        return Err(V2StateError::CapacityExceeded {
            limit: MAX_V2_ENTRY_COUNT,
        });
    }
    let records_len = checked_records_len(records.len(), RESERVE_RECORD_LEN)?;
    let capacity = RESERVE_HEADER_LEN
        .checked_add(records_len)
        .and_then(|len| len.checked_add(CHECKSUM_LEN))
        .ok_or_else(|| V2StateError::Corrupt("reserve snapshot length overflow".to_string()))?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&RESERVE_MAGIC);
    bytes.push(BASIS_V2_ABI_GENERATION);
    bytes.extend_from_slice(&binding.tracker_nft_id);
    bytes.extend_from_slice(&binding.reserve_nft_id);
    encode_asset_binding(&mut bytes, binding.asset);
    bytes.extend_from_slice(&(records.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&root);
    for (claim, state) in records {
        validate_reserve_binding(claim, binding)?;
        if state.timestamp() != claim.timestamp() || state.total_debt() != claim.total_debt() {
            return Err(V2StateError::Corrupt(
                "redeemed state does not match its authenticated claim".to_string(),
            ));
        }
        encode_claim(&mut bytes, claim)?;
        bytes.extend_from_slice(&state.encode());
    }
    append_checksum(&mut bytes, RESERVE_CHECKSUM_DOMAIN);
    debug_assert_eq!(bytes.len(), capacity);
    Ok(bytes)
}

fn decode_reserve_snapshot(
    bytes: &[u8],
    expected_binding: ReserveStoreBindingV2,
) -> Result<(Vec<(ClaimV2, RedeemedStateV2)>, ReserveAvlTree), V2StateError> {
    reject_legacy_or_malformed(
        bytes,
        RESERVE_MAGIC,
        LEGACY_RESERVE_MAGIC,
        RESERVE_HEADER_LEN + CHECKSUM_LEN,
        MAX_RESERVE_SNAPSHOT_LEN,
        "reserve",
    )?;
    let count_offset = 4 + 1 + 32 + 32 + 1 + 32;
    let count = read_u32_at(bytes, count_offset)? as usize;
    validate_snapshot_len(
        bytes.len(),
        count,
        RESERVE_HEADER_LEN,
        RESERVE_RECORD_LEN,
        MAX_RESERVE_SNAPSHOT_LEN,
        "reserve",
    )?;
    verify_checksum(bytes, RESERVE_CHECKSUM_DOMAIN)?;

    let payload_len = bytes.len() - CHECKSUM_LEN;
    let mut decoder = Decoder::new(&bytes[..payload_len]);
    if decoder.array::<4>()? != RESERVE_MAGIC || decoder.byte()? != BASIS_V2_ABI_GENERATION {
        return Err(V2StateError::MigrationRequired(
            "unsupported reserve snapshot generation".to_string(),
        ));
    }
    let tracker_nft_id = decoder.array::<32>()?;
    let reserve_nft_id = decoder.array::<32>()?;
    let asset = decode_asset_binding(&mut decoder, reserve_nft_id)?;
    let stored_binding = ReserveStoreBindingV2 {
        tracker_nft_id,
        reserve_nft_id,
        asset,
    };
    if stored_binding != expected_binding {
        return Err(V2StateError::BindingMismatch);
    }
    let decoded_count = decoder.u32()? as usize;
    let stored_root = decoder.array::<33>()?;
    let mut records = Vec::with_capacity(decoded_count);
    for _ in 0..decoded_count {
        let claim = decode_claim(&mut decoder)?;
        validate_reserve_binding(&claim, stored_binding).map_err(|_| {
            V2StateError::Corrupt("claim does not match reserve lineage and asset".to_string())
        })?;
        let state = RedeemedStateV2::decode(&decoder.array::<24>()?)
            .map_err(|error| V2StateError::Corrupt(error.to_string()))?;
        if state.timestamp() != claim.timestamp() || state.total_debt() != claim.total_debt() {
            return Err(V2StateError::Corrupt(
                "redeemed state does not match its authenticated claim".to_string(),
            ));
        }
        records.push((claim, state));
    }
    decoder.finish()?;
    index_reserve_records(&records)?;
    let tree = build_reserve_tree(&records)?;
    if tree.root_digest()? != stored_root {
        return Err(V2StateError::Corrupt(
            "stored reserve root does not match ordered redeemed states".to_string(),
        ));
    }
    Ok((records, tree))
}

fn encode_claim(bytes: &mut Vec<u8>, claim: &ClaimV2) -> Result<(), V2StateError> {
    revalidate_claim(claim)?;
    let domain = claim.domain();
    bytes.extend_from_slice(&domain.reserve_nft_id());
    bytes.extend_from_slice(&domain.tracker_nft_id());
    bytes.extend_from_slice(&domain.owner_pubkey());
    bytes.extend_from_slice(&domain.receiver_pubkey());
    encode_asset_binding(bytes, domain.asset());
    bytes.extend_from_slice(&claim.total_debt().to_be_bytes());
    bytes.extend_from_slice(&claim.timestamp().to_be_bytes());
    bytes.extend_from_slice(claim.signature());
    Ok(())
}

fn decode_claim(decoder: &mut Decoder<'_>) -> Result<ClaimV2, V2StateError> {
    let reserve_nft_id = decoder.array::<32>()?;
    let tracker_nft_id = decoder.array::<32>()?;
    let owner_pubkey = decoder.array::<33>()?;
    let receiver_pubkey = decoder.array::<33>()?;
    let asset = decode_asset_binding(decoder, reserve_nft_id)?;
    let domain = match asset {
        ReserveAssetV2::Erg => ClaimDomainV2::erg(
            reserve_nft_id,
            tracker_nft_id,
            owner_pubkey,
            receiver_pubkey,
        ),
        ReserveAssetV2::Token { token_id } => ClaimDomainV2::token(
            reserve_nft_id,
            token_id,
            tracker_nft_id,
            owner_pubkey,
            receiver_pubkey,
        ),
    }
    .map_err(|error| V2StateError::Corrupt(error.to_string()))?;
    let total_debt = decoder.u64()?;
    let timestamp = decoder.u64()?;
    let signature = decoder.array::<65>()?;
    ClaimV2::from_signed(domain, total_debt, timestamp, signature)
        .map_err(|error| V2StateError::Corrupt(error.to_string()))
}

fn encode_asset_binding(bytes: &mut Vec<u8>, asset: ReserveAssetV2) {
    match asset {
        ReserveAssetV2::Erg => {
            bytes.push(BASIS_V2_ERG_ASSET_KIND);
            bytes.extend_from_slice(&[0u8; 32]);
        }
        ReserveAssetV2::Token { token_id } => {
            bytes.push(BASIS_V2_TOKEN_ASSET_KIND);
            bytes.extend_from_slice(&token_id);
        }
    }
}

fn decode_asset_binding(
    decoder: &mut Decoder<'_>,
    reserve_nft_id: [u8; 32],
) -> Result<ReserveAssetV2, V2StateError> {
    let kind = decoder.byte()?;
    let token_or_zero = decoder.array::<32>()?;
    match kind {
        BASIS_V2_ERG_ASSET_KIND if token_or_zero == [0u8; 32] => Ok(ReserveAssetV2::Erg),
        BASIS_V2_ERG_ASSET_KIND => Err(V2StateError::Corrupt(
            "ERG binding contains a non-zero token id".to_string(),
        )),
        BASIS_V2_TOKEN_ASSET_KIND if token_or_zero != reserve_nft_id => Ok(ReserveAssetV2::Token {
            token_id: token_or_zero,
        }),
        BASIS_V2_TOKEN_ASSET_KIND => Err(V2StateError::Corrupt(
            "reserve NFT and reserve token ids are identical".to_string(),
        )),
        _ => Err(V2StateError::MigrationRequired(
            "unsupported reserve asset discriminator".to_string(),
        )),
    }
}

fn checked_records_len(count: usize, record_len: usize) -> Result<usize, V2StateError> {
    count
        .checked_mul(record_len)
        .ok_or_else(|| V2StateError::Corrupt("snapshot length overflow".to_string()))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, V2StateError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| V2StateError::Corrupt("snapshot offset overflow".to_string()))?;
    let source = bytes.get(offset..end).ok_or_else(|| {
        V2StateError::Corrupt("snapshot ended inside its entry count".to_string())
    })?;
    let mut encoded = [0u8; 4];
    encoded.copy_from_slice(source);
    Ok(u32::from_be_bytes(encoded))
}

fn validate_snapshot_len(
    actual_len: usize,
    count: usize,
    header_len: usize,
    record_len: usize,
    max_len: usize,
    label: &str,
) -> Result<(), V2StateError> {
    if count > MAX_V2_ENTRY_COUNT {
        return Err(V2StateError::Corrupt(format!(
            "stored {label} count exceeds {MAX_V2_ENTRY_COUNT}"
        )));
    }
    let expected_len = header_len
        .checked_add(checked_records_len(count, record_len)?)
        .and_then(|len| len.checked_add(CHECKSUM_LEN))
        .ok_or_else(|| V2StateError::Corrupt(format!("{label} snapshot length overflow")))?;
    if actual_len > max_len || actual_len != expected_len {
        return Err(V2StateError::Corrupt(format!(
            "stored {label} length does not match its bounded count"
        )));
    }
    Ok(())
}

fn reject_legacy_or_malformed(
    bytes: &[u8],
    expected_magic: [u8; 4],
    legacy_magic: [u8; 4],
    minimum_len: usize,
    maximum_len: usize,
    label: &str,
) -> Result<(), V2StateError> {
    if bytes.get(..4) == Some(legacy_magic.as_slice()) {
        return Err(V2StateError::MigrationRequired(format!(
            "{label} v1 state cannot be converted implicitly"
        )));
    }
    if bytes.len() < minimum_len
        || bytes.len() > maximum_len
        || bytes.get(..4) != Some(expected_magic.as_slice())
    {
        return Err(V2StateError::MigrationRequired(format!(
            "unsupported or malformed {label} snapshot"
        )));
    }
    if bytes[4] != BASIS_V2_ABI_GENERATION {
        return Err(V2StateError::MigrationRequired(format!(
            "unsupported {label} ABI generation"
        )));
    }
    Ok(())
}

fn append_checksum(bytes: &mut Vec<u8>, domain: &[u8]) {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(domain);
    hasher.update(bytes.as_slice());
    let checksum: [u8; 32] = hasher.finalize().into();
    bytes.extend_from_slice(&checksum);
}

fn verify_checksum(bytes: &[u8], domain: &[u8]) -> Result<(), V2StateError> {
    if bytes.len() < CHECKSUM_LEN {
        return Err(V2StateError::Corrupt(
            "snapshot is shorter than its checksum".to_string(),
        ));
    }
    let payload_len = bytes.len() - CHECKSUM_LEN;
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(domain);
    hasher.update(&bytes[..payload_len]);
    let expected: [u8; 32] = hasher.finalize().into();
    if bytes[payload_len..] != expected[..] {
        return Err(V2StateError::Corrupt(
            "authoritative snapshot checksum mismatch".to_string(),
        ));
    }
    Ok(())
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], V2StateError> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or_else(|| V2StateError::Corrupt("decoder offset overflow".to_string()))?;
        let slice = self.bytes.get(self.offset..end).ok_or_else(|| {
            V2StateError::Corrupt("snapshot ended inside a fixed-width field".to_string())
        })?;
        self.offset = end;
        let mut output = [0u8; N];
        output.copy_from_slice(slice);
        Ok(output)
    }

    fn byte(&mut self) -> Result<u8, V2StateError> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, V2StateError> {
        Ok(u32::from_be_bytes(self.array::<4>()?))
    }

    fn u64(&mut self) -> Result<u64, V2StateError> {
        Ok(u64::from_be_bytes(self.array::<8>()?))
    }

    fn finish(self) -> Result<(), V2StateError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(V2StateError::Corrupt(
                "snapshot contains trailing payload bytes".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use basis_core::types::PubKey;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use tempfile::TempDir;

    fn key(seed: u8) -> ([u8; 32], PubKey) {
        let mut secret = [0u8; 32];
        secret[31] = seed;
        let secret_key = SecretKey::from_slice(&secret).unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &secret_key).serialize();
        (secret, public)
    }

    fn erg_claim(
        tracker_nft_id: [u8; 32],
        reserve_nft_id: [u8; 32],
        owner_seed: u8,
        receiver_seed: u8,
        total_debt: u64,
        timestamp: u64,
    ) -> ClaimV2 {
        let (owner_secret, owner) = key(owner_seed);
        let (_, receiver) = key(receiver_seed);
        let domain = ClaimDomainV2::erg(reserve_nft_id, tracker_nft_id, owner, receiver).unwrap();
        ClaimV2::sign(domain, total_debt, timestamp, &owner_secret).unwrap()
    }

    fn token_claim(
        tracker_nft_id: [u8; 32],
        reserve_nft_id: [u8; 32],
        token_id: [u8; 32],
        owner_seed: u8,
        receiver_seed: u8,
        total_debt: u64,
        timestamp: u64,
    ) -> ClaimV2 {
        let (owner_secret, owner) = key(owner_seed);
        let (_, receiver) = key(receiver_seed);
        let domain =
            ClaimDomainV2::token(reserve_nft_id, token_id, tracker_nft_id, owner, receiver)
                .unwrap();
        ClaimV2::sign(domain, total_debt, timestamp, &owner_secret).unwrap()
    }

    fn rewrite_checksum(bytes: &mut Vec<u8>, domain: &[u8]) {
        bytes.truncate(bytes.len() - CHECKSUM_LEN);
        append_checksum(bytes, domain);
    }

    fn replace_and_sync(partition: &Partition, keyspace: &Keyspace, key: &[u8], bytes: Vec<u8>) {
        partition.insert(key, bytes).unwrap();
        keyspace.persist(PersistMode::SyncData).unwrap();
    }

    #[test]
    fn tracker_requires_fresh_approval_enforces_binding_and_single_writer() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("tracker");
        let tracker = [7u8; 32];

        assert_eq!(
            TrackerClaimStoreV2::open(&path, tracker, FreshV2StateApproval::Reject)
                .err()
                .unwrap(),
            V2StateError::FreshGenerationRequired
        );
        assert!(!path.exists());

        let store =
            TrackerClaimStoreV2::open(&path, tracker, FreshV2StateApproval::Approve).unwrap();
        let empty_root = store.root_digest().unwrap();
        assert_eq!(store.len().unwrap(), 0);
        assert_eq!(
            TrackerClaimStoreV2::open(&path, tracker, FreshV2StateApproval::Reject)
                .err()
                .unwrap(),
            V2StateError::WriterAlreadyActive
        );
        drop(store);

        let reopened =
            TrackerClaimStoreV2::open(&path, tracker, FreshV2StateApproval::Reject).unwrap();
        assert_eq!(reopened.root_digest().unwrap(), empty_root);
        drop(reopened);
        assert_eq!(
            TrackerClaimStoreV2::open(&path, [8u8; 32], FreshV2StateApproval::Reject)
                .err()
                .unwrap(),
            V2StateError::BindingMismatch
        );
    }

    #[test]
    fn tracker_preserves_first_insertion_order_and_root_across_restart() {
        let temp = TempDir::new().unwrap();
        let tracker = [10u8; 32];
        let reserve = [11u8; 32];
        let first = erg_claim(tracker, reserve, 1, 2, 100, 10);
        let second = erg_claim(tracker, reserve, 3, 4, 200, 20);
        let first_key = first.domain().claim_key();
        let second_key = second.domain().claim_key();
        let first_update = erg_claim(tracker, reserve, 1, 2, 175, 30);

        let mut store =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Approve).unwrap();
        store.record_validated_claim(first).unwrap();
        store.record_validated_claim(second).unwrap();
        let root = store.record_validated_claim(first_update).unwrap();
        assert_eq!(
            store.ordered_claim_keys().unwrap(),
            vec![first_key, second_key]
        );
        assert_eq!(store.claim(&first_key).unwrap().unwrap().total_debt(), 175);

        let expected = TrackerAvlTree::from_ordered_entries([
            (first_key, 175u64.to_be_bytes()),
            (second_key, 200u64.to_be_bytes()),
        ])
        .unwrap()
        .root_digest()
        .unwrap();
        assert_eq!(root, expected);
        drop(store);

        let reopened =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Reject).unwrap();
        assert_eq!(reopened.root_digest().unwrap(), root);
        assert_eq!(
            reopened.ordered_claim_keys().unwrap(),
            vec![first_key, second_key]
        );
    }

    #[test]
    fn tracker_rejects_binding_regression_and_capacity_without_poisoning() {
        let temp = TempDir::new().unwrap();
        let tracker = [12u8; 32];
        let reserve = [13u8; 32];
        let mut store =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Approve).unwrap();
        store
            .record_validated_claim(erg_claim(tracker, reserve, 1, 2, 100, 10))
            .unwrap();
        assert_eq!(
            store
                .record_validated_claim(erg_claim(tracker, reserve, 1, 2, 99, 11))
                .unwrap_err(),
            V2StateError::Claim(BasisV2Error::ClaimRegression)
        );
        assert_eq!(
            store
                .record_validated_claim(erg_claim([99u8; 32], reserve, 3, 4, 5, 5))
                .unwrap_err(),
            V2StateError::BindingMismatch
        );
        store.capacity_limit = 1;
        assert_eq!(
            store
                .record_validated_claim(erg_claim(tracker, reserve, 3, 4, 5, 20))
                .unwrap_err(),
            V2StateError::CapacityExceeded { limit: 1 }
        );
        assert!(!store.is_poisoned());
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn tracker_unknown_write_outcome_is_terminal_and_restart_is_self_consistent() {
        let temp = TempDir::new().unwrap();
        let tracker = [14u8; 32];
        let reserve = [15u8; 32];
        let mut store =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Approve).unwrap();
        let old_root = store.root_digest().unwrap();
        let claim = erg_claim(tracker, reserve, 1, 2, 100, 10);
        let key = claim.domain().claim_key();
        let new_root = TrackerAvlTree::from_ordered_entries([(key, 100u64.to_be_bytes())])
            .unwrap()
            .root_digest()
            .unwrap();
        store.fail_next_persist = true;
        assert!(matches!(
            store.record_validated_claim(claim),
            Err(V2StateError::StorageOutcomeUnknown(_))
        ));
        assert!(store.is_poisoned());
        assert_eq!(store.root_digest().unwrap_err(), V2StateError::Poisoned);
        assert_eq!(
            store
                .record_validated_claim(erg_claim(tracker, reserve, 3, 4, 5, 20))
                .unwrap_err(),
            V2StateError::Poisoned
        );
        drop(store);

        let reopened =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Reject).unwrap();
        assert!([old_root, new_root].contains(&reopened.root_digest().unwrap()));
    }

    fn tracker_corruption_case<F>(mutate: F) -> V2StateError
    where
        F: FnOnce(&mut Vec<u8>),
    {
        let temp = TempDir::new().unwrap();
        let tracker = [20u8; 32];
        let reserve = [21u8; 32];
        let mut store =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Approve).unwrap();
        store
            .record_validated_claim(erg_claim(tracker, reserve, 1, 2, 100, 10))
            .unwrap();
        store
            .record_validated_claim(erg_claim(tracker, reserve, 3, 4, 200, 20))
            .unwrap();
        let mut bytes = store
            .partition
            .get(TRACKER_SNAPSHOT_KEY)
            .unwrap()
            .unwrap()
            .to_vec();
        mutate(&mut bytes);
        replace_and_sync(
            &store.partition,
            &store.keyspace,
            TRACKER_SNAPSHOT_KEY,
            bytes,
        );
        drop(store);
        TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Reject)
            .err()
            .expect("corruption must reject restart")
    }

    #[test]
    fn tracker_restart_rejects_checksum_signature_root_order_duplicate_and_bounds_corruption() {
        assert!(matches!(
            tracker_corruption_case(|bytes| *bytes.last_mut().unwrap() ^= 1),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| {
                let debt_offset = TRACKER_HEADER_LEN + 163;
                bytes[debt_offset + 7] ^= 1;
                rewrite_checksum(bytes, TRACKER_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| {
                let root_offset = 4 + 1 + 32 + 4;
                bytes[root_offset] ^= 1;
                rewrite_checksum(bytes, TRACKER_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| {
                let first =
                    bytes[TRACKER_HEADER_LEN..TRACKER_HEADER_LEN + CLAIM_RECORD_LEN].to_vec();
                let second = bytes[TRACKER_HEADER_LEN + CLAIM_RECORD_LEN
                    ..TRACKER_HEADER_LEN + 2 * CLAIM_RECORD_LEN]
                    .to_vec();
                bytes[TRACKER_HEADER_LEN..TRACKER_HEADER_LEN + CLAIM_RECORD_LEN]
                    .copy_from_slice(&second);
                bytes[TRACKER_HEADER_LEN + CLAIM_RECORD_LEN
                    ..TRACKER_HEADER_LEN + 2 * CLAIM_RECORD_LEN]
                    .copy_from_slice(&first);
                rewrite_checksum(bytes, TRACKER_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| {
                let first =
                    bytes[TRACKER_HEADER_LEN..TRACKER_HEADER_LEN + CLAIM_RECORD_LEN].to_vec();
                bytes[TRACKER_HEADER_LEN + CLAIM_RECORD_LEN
                    ..TRACKER_HEADER_LEN + 2 * CLAIM_RECORD_LEN]
                    .copy_from_slice(&first);
                rewrite_checksum(bytes, TRACKER_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| {
                bytes[37..41].copy_from_slice(&((MAX_V2_ENTRY_COUNT as u32) + 1).to_be_bytes());
            }),
            V2StateError::Corrupt(_)
        ));

        assert!(matches!(
            tracker_corruption_case(|bytes| bytes[..4].copy_from_slice(&LEGACY_TRACKER_MAGIC)),
            V2StateError::MigrationRequired(_)
        ));
    }

    #[test]
    fn tracker_rejects_extra_rows_and_ambiguous_existing_directories() {
        let temp = TempDir::new().unwrap();
        let tracker = [22u8; 32];
        let store =
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Approve).unwrap();
        store.partition.insert(b"unexpected", [1u8]).unwrap();
        store.keyspace.persist(PersistMode::SyncData).unwrap();
        drop(store);
        assert!(matches!(
            TrackerClaimStoreV2::open(temp.path(), tracker, FreshV2StateApproval::Reject),
            Err(V2StateError::Corrupt(_))
        ));

        let ambiguous = TempDir::new().unwrap();
        std::fs::write(ambiguous.path().join("legacy.data"), b"not a v2 database").unwrap();
        assert!(matches!(
            TrackerClaimStoreV2::open(ambiguous.path(), tracker, FreshV2StateApproval::Approve),
            Err(V2StateError::MigrationRequired(_))
        ));
    }

    #[test]
    fn reserve_roots_are_independent_bound_and_restart_deterministically() {
        let temp = TempDir::new().unwrap();
        let tracker = [30u8; 32];
        let first_reserve = [31u8; 32];
        let second_reserve = [32u8; 32];
        let first_binding = ReserveStoreBindingV2::erg(tracker, first_reserve);
        let second_binding = ReserveStoreBindingV2::erg(tracker, second_reserve);
        let first_path = temp.path().join("reserve-a");
        let second_path = temp.path().join("reserve-b");
        let mut first =
            ReserveRedeemedStoreV2::open(&first_path, first_binding, FreshV2StateApproval::Approve)
                .unwrap();
        let mut second = ReserveRedeemedStoreV2::open(
            &second_path,
            second_binding,
            FreshV2StateApproval::Approve,
        )
        .unwrap();
        assert_eq!(
            ReserveRedeemedStoreV2::open(&first_path, first_binding, FreshV2StateApproval::Reject)
                .err()
                .unwrap(),
            V2StateError::WriterAlreadyActive
        );

        let first_claim = erg_claim(tracker, first_reserve, 1, 2, 100, 10);
        let other_claim = erg_claim(tracker, first_reserve, 3, 4, 200, 20);
        let first_key = first_claim.domain().claim_key();
        let other_key = other_claim.domain().claim_key();
        let initial_root = first.root_digest().unwrap();
        let first_root = first
            .commit_confirmed_redemption(initial_root, first_claim, 25)
            .unwrap();
        let first_root = first
            .commit_confirmed_redemption(first_root, other_claim, 50)
            .unwrap();
        let first_update = erg_claim(tracker, first_reserve, 1, 2, 150, 30);
        let first_root = first
            .commit_confirmed_redemption(first_root, first_update, 20)
            .unwrap();
        assert_eq!(
            first.ordered_claim_keys().unwrap(),
            vec![first_key, other_key]
        );
        assert_eq!(
            first
                .redeemed_state(&first_key)
                .unwrap()
                .unwrap()
                .redeemed(),
            45
        );

        let second_claim = erg_claim(tracker, second_reserve, 1, 2, 100, 10);
        let second_root = second
            .commit_confirmed_redemption(second.root_digest().unwrap(), second_claim, 25)
            .unwrap();
        assert_ne!(first_root, second_root);
        drop(first);
        drop(second);

        let reopened =
            ReserveRedeemedStoreV2::open(&first_path, first_binding, FreshV2StateApproval::Reject)
                .unwrap();
        assert_eq!(reopened.root_digest().unwrap(), first_root);
        assert_eq!(
            reopened.ordered_claim_keys().unwrap(),
            vec![first_key, other_key]
        );
        drop(reopened);
        assert_eq!(
            ReserveRedeemedStoreV2::open(&first_path, second_binding, FreshV2StateApproval::Reject)
                .err()
                .unwrap(),
            V2StateError::BindingMismatch
        );
    }

    #[test]
    fn reserve_rejects_unconfirmed_inputs_and_asset_or_lineage_mismatch_without_poisoning() {
        let temp = TempDir::new().unwrap();
        let tracker = [40u8; 32];
        let reserve = [41u8; 32];
        let token = [42u8; 32];
        let binding = ReserveStoreBindingV2::erg(tracker, reserve);
        let mut store =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Approve)
                .unwrap();
        let root = store.root_digest().unwrap();

        assert_eq!(
            store
                .commit_confirmed_redemption(
                    [0u8; 33],
                    erg_claim(tracker, reserve, 1, 2, 100, 10),
                    1,
                )
                .unwrap_err(),
            V2StateError::StaleRoot
        );
        assert_eq!(
            store
                .commit_confirmed_redemption(
                    root,
                    token_claim(tracker, reserve, token, 1, 2, 100, 10),
                    1,
                )
                .unwrap_err(),
            V2StateError::BindingMismatch
        );
        assert_eq!(
            store
                .commit_confirmed_redemption(
                    root,
                    erg_claim([99u8; 32], reserve, 1, 2, 100, 10),
                    1,
                )
                .unwrap_err(),
            V2StateError::BindingMismatch
        );
        assert_eq!(
            store
                .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 0,)
                .unwrap_err(),
            V2StateError::Claim(BasisV2Error::InvalidRedemptionAmount)
        );
        assert_eq!(
            store
                .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 101,)
                .unwrap_err(),
            V2StateError::Claim(BasisV2Error::RedemptionExceedsClaim)
        );

        store.capacity_limit = 0;
        assert_eq!(
            store
                .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 1,)
                .unwrap_err(),
            V2StateError::CapacityExceeded { limit: 0 }
        );
        assert!(!store.is_poisoned());
        assert_eq!(store.len().unwrap(), 0);
    }

    #[test]
    fn reserve_enforces_cumulative_redemption_and_claim_successor_rules() {
        let temp = TempDir::new().unwrap();
        let tracker = [43u8; 32];
        let reserve = [44u8; 32];
        let binding = ReserveStoreBindingV2::erg(tracker, reserve);
        let mut store =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Approve)
                .unwrap();
        let root = store.root_digest().unwrap();
        let root = store
            .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 90)
            .unwrap();
        assert_eq!(
            store
                .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 11,)
                .unwrap_err(),
            V2StateError::Claim(BasisV2Error::RedemptionExceedsClaim)
        );
        assert_eq!(
            store
                .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 99, 11), 1,)
                .unwrap_err(),
            V2StateError::Claim(BasisV2Error::ClaimRegression)
        );
        assert!(!store.is_poisoned());
    }

    #[test]
    fn token_reserve_accepts_only_its_exact_token_binding() {
        let temp = TempDir::new().unwrap();
        let tracker = [45u8; 32];
        let reserve = [46u8; 32];
        let token = [47u8; 32];
        assert_eq!(
            ReserveStoreBindingV2::token(tracker, reserve, reserve)
                .err()
                .unwrap(),
            V2StateError::Claim(BasisV2Error::DuplicateReserveAssetId)
        );
        let binding = ReserveStoreBindingV2::token(tracker, reserve, token).unwrap();
        let mut store =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Approve)
                .unwrap();
        let root = store.root_digest().unwrap();
        store
            .commit_confirmed_redemption(
                root,
                token_claim(tracker, reserve, token, 1, 2, 100, 10),
                1,
            )
            .unwrap();
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn reserve_unknown_write_outcome_is_terminal_and_restart_is_self_consistent() {
        let temp = TempDir::new().unwrap();
        let tracker = [48u8; 32];
        let reserve = [49u8; 32];
        let binding = ReserveStoreBindingV2::erg(tracker, reserve);
        let claim = erg_claim(tracker, reserve, 1, 2, 100, 10);
        let key = claim.domain().claim_key();
        let state = RedeemedStateV2::new(10, 100, 25).unwrap();
        let new_root = ReserveAvlTree::from_ordered_entries([(key, state.encode())])
            .unwrap()
            .root_digest()
            .unwrap();
        let mut store =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Approve)
                .unwrap();
        let old_root = store.root_digest().unwrap();
        store.fail_next_persist = true;
        assert!(matches!(
            store.commit_confirmed_redemption(old_root, claim, 25),
            Err(V2StateError::StorageOutcomeUnknown(_))
        ));
        assert!(store.is_poisoned());
        assert_eq!(store.len().unwrap_err(), V2StateError::Poisoned);
        drop(store);

        let reopened =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Reject)
                .unwrap();
        assert!([old_root, new_root].contains(&reopened.root_digest().unwrap()));
    }

    fn reserve_corruption_case<F>(mutate: F) -> V2StateError
    where
        F: FnOnce(&mut Vec<u8>),
    {
        let temp = TempDir::new().unwrap();
        let tracker = [50u8; 32];
        let reserve = [51u8; 32];
        let binding = ReserveStoreBindingV2::erg(tracker, reserve);
        let mut store =
            ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Approve)
                .unwrap();
        let root = store.root_digest().unwrap();
        store
            .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 1, 2, 100, 10), 25)
            .unwrap();
        let root = store.root_digest().unwrap();
        store
            .commit_confirmed_redemption(root, erg_claim(tracker, reserve, 3, 4, 200, 20), 50)
            .unwrap();
        let mut bytes = store
            .partition
            .get(RESERVE_SNAPSHOT_KEY)
            .unwrap()
            .unwrap()
            .to_vec();
        mutate(&mut bytes);
        replace_and_sync(
            &store.partition,
            &store.keyspace,
            RESERVE_SNAPSHOT_KEY,
            bytes,
        );
        drop(store);
        ReserveRedeemedStoreV2::open(temp.path(), binding, FreshV2StateApproval::Reject)
            .err()
            .expect("corruption must reject restart")
    }

    #[test]
    fn reserve_restart_rejects_checksum_signature_state_root_order_and_legacy_corruption() {
        assert!(matches!(
            reserve_corruption_case(|bytes| *bytes.last_mut().unwrap() ^= 1),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let signature_offset = RESERVE_HEADER_LEN + 179;
                bytes[signature_offset + 10] ^= 1;
                rewrite_checksum(bytes, RESERVE_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let state_total_debt_offset = RESERVE_HEADER_LEN + CLAIM_RECORD_LEN + 8;
                bytes[state_total_debt_offset + 7] ^= 1;
                rewrite_checksum(bytes, RESERVE_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let root_offset = 4 + 1 + 32 + 32 + 1 + 32 + 4;
                bytes[root_offset] ^= 1;
                rewrite_checksum(bytes, RESERVE_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let first =
                    bytes[RESERVE_HEADER_LEN..RESERVE_HEADER_LEN + RESERVE_RECORD_LEN].to_vec();
                bytes[RESERVE_HEADER_LEN + RESERVE_RECORD_LEN
                    ..RESERVE_HEADER_LEN + 2 * RESERVE_RECORD_LEN]
                    .copy_from_slice(&first);
                rewrite_checksum(bytes, RESERVE_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let first =
                    bytes[RESERVE_HEADER_LEN..RESERVE_HEADER_LEN + RESERVE_RECORD_LEN].to_vec();
                let second = bytes[RESERVE_HEADER_LEN + RESERVE_RECORD_LEN
                    ..RESERVE_HEADER_LEN + 2 * RESERVE_RECORD_LEN]
                    .to_vec();
                bytes[RESERVE_HEADER_LEN..RESERVE_HEADER_LEN + RESERVE_RECORD_LEN]
                    .copy_from_slice(&second);
                bytes[RESERVE_HEADER_LEN + RESERVE_RECORD_LEN
                    ..RESERVE_HEADER_LEN + 2 * RESERVE_RECORD_LEN]
                    .copy_from_slice(&first);
                rewrite_checksum(bytes, RESERVE_CHECKSUM_DOMAIN);
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| {
                let count_offset = 4 + 1 + 32 + 32 + 1 + 32;
                bytes[count_offset..count_offset + 4]
                    .copy_from_slice(&((MAX_V2_ENTRY_COUNT as u32) + 1).to_be_bytes());
            }),
            V2StateError::Corrupt(_)
        ));
        assert!(matches!(
            reserve_corruption_case(|bytes| bytes[..4].copy_from_slice(&LEGACY_RESERVE_MAGIC)),
            V2StateError::MigrationRequired(_)
        ));
    }
}
