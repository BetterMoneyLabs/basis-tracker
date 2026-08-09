//! Durable, fail-closed reconciliation of signed protocol transactions.
//!
//! Transaction presence is not confirmation. Acceptance requires one exact
//! signed transaction, canonical predecessor and successor boxes, and a
//! coherent selected-chain header segment whose first block contains the
//! transaction and whose last block is the unchanged node tip.

use crate::{blake2b256_hash, ConfirmedProjectionAnchor};
use ergo_lib::{
    chain::{block::FullBlock, transaction::Transaction},
    ergo_chain_types::{blake2b256_hash as header_hash, BlockId, Digest32, Header},
    ergo_merkle_tree::{MerkleNode, MerkleTree},
    ergotree_ir::{
        chain::ergo_box::{ErgoBox, NonMandatoryRegisterId},
        serialization::SigmaSerializable,
    },
};
use fjall::{Config, Keyspace, PartitionCreateOptions, PersistMode};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
};

const JOURNAL_MAGIC: &[u8; 4] = b"BCJ1";
const JOURNAL_KEY: &[u8] = b"confirmed_chain_journal_v1";
const JOURNAL_CHECKSUM_DOMAIN: &[u8] = b"basis-confirmed-chain-journal-v1";
const JOURNAL_MANIFEST_FILE: &str = "confirmed-chain.manifest";
const JOURNAL_MANIFEST_MAGIC: &[u8; 4] = b"BCM1";
const JOURNAL_MANIFEST_CHECKSUM_DOMAIN: &[u8] = b"basis-confirmed-chain-manifest-v1";
const MAX_SIGNED_TRANSACTION_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHAIN_HEADERS: usize = 4096;
const MAX_HISTORY_ENTRIES: usize = 256;

/// Largest bounded reorg-monitoring horizon accepted by this implementation.
/// The inclusive chain window contains `depth + 1` headers.
pub const MAX_REORG_MONITOR_DEPTH: u64 = (MAX_CHAIN_HEADERS - 1) as u64;

/// Stable identity shared by the BNS1 tracker state and this reconciliation
/// journal. The state identity is derived, rather than caller-selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationJournalBinding {
    tracker_nft_id: [u8; 32],
    protocol_generation: u8,
    state_identity: [u8; 32],
}

impl ReconciliationJournalBinding {
    pub fn tracker_v1(tracker_nft_id: [u8; 32]) -> Self {
        let mut material = Vec::with_capacity(24 + tracker_nft_id.len());
        material.extend_from_slice(b"basis-tracker-state-v1");
        material.extend_from_slice(&tracker_nft_id);
        Self {
            tracker_nft_id,
            protocol_generation: 1,
            state_identity: blake2b256_hash(&material),
        }
    }

    pub fn tracker_nft_id(&self) -> &[u8; 32] {
        &self.tracker_nft_id
    }
}

/// Whether startup may initialize a journal manifest. Existing BNS1 chain or
/// accounting history must use `ExistingRequired` so a lost/misdirected
/// journal fails before creating any replacement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalBootstrap {
    FreshAllowed,
    ExistingRequired,
}

thread_local! {
    /// Validated tickets may be reconstructed only while reading this module's
    /// checksummed single-writer journal. A generic serde caller cannot mint a
    /// policy ticket from arbitrary ids.
    static JOURNAL_DESERIALIZATION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct JournalDeserializationGuard;

impl JournalDeserializationGuard {
    fn enter() -> Self {
        JOURNAL_DESERIALIZATION_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }

    fn is_active() -> bool {
        JOURNAL_DESERIALIZATION_DEPTH.with(|depth| depth.get() > 0)
    }
}

impl Drop for JournalDeserializationGuard {
    fn drop(&mut self) {
        JOURNAL_DESERIALIZATION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// One ordered token entry. Token order is part of the protocol ABI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolAsset {
    token_id: String,
    amount: u64,
}

impl ProtocolAsset {
    pub fn token_id(&self) -> &str {
        &self.token_id
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }
}

/// Canonical protocol box. Every field, including R4-R9 and token order, is
/// re-derived from `canonical_bytes`; callers cannot bind arbitrary values to
/// a real box id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolBox {
    canonical_bytes: Vec<u8>,
    box_id: String,
    value: u64,
    ergo_tree: Vec<u8>,
    assets: Vec<ProtocolAsset>,
    registers: [Option<Vec<u8>>; 6],
    creation_height: u32,
    transaction_id: String,
    index: u16,
}

impl ProtocolBox {
    /// Parse a node box response. `ergo-lib` recomputes and verifies `boxId`
    /// from value/tree/assets/registers/creation reference.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, ReconciliationError> {
        let ergo_box: ErgoBox = serde_json::from_slice(bytes)
            .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))?;
        Self::from_ergo_box(&ergo_box)
    }

    /// Parse canonical binary box bytes. The embedded creation reference is
    /// included in the recomputed box id.
    pub fn from_serialized_bytes(bytes: &[u8]) -> Result<Self, ReconciliationError> {
        let ergo_box = ErgoBox::sigma_parse_bytes(bytes)
            .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))?;
        let result = Self::from_ergo_box(&ergo_box)?;
        if result.canonical_bytes != bytes {
            return Err(ReconciliationError::MalformedBox(
                "box bytes are not canonical".to_string(),
            ));
        }
        Ok(result)
    }

    fn from_ergo_box(ergo_box: &ErgoBox) -> Result<Self, ReconciliationError> {
        let canonical_bytes = ergo_box
            .sigma_serialize_bytes()
            .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))?;
        if canonical_bytes.len() > ErgoBox::MAX_BOX_SIZE {
            return Err(ReconciliationError::MalformedBox(
                "box exceeds the protocol size bound".to_string(),
            ));
        }
        let ergo_tree = ergo_box
            .ergo_tree
            .sigma_serialize_bytes()
            .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))?;
        let assets = ergo_box
            .tokens
            .as_ref()
            .map(|tokens| {
                tokens
                    .iter()
                    .map(|token| ProtocolAsset {
                        token_id: String::from(token.token_id),
                        amount: *token.amount.as_u64(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut registers: [Option<Vec<u8>>; 6] = Default::default();
        for (offset, register_id) in NonMandatoryRegisterId::REG_IDS.iter().enumerate() {
            registers[offset] = ergo_box
                .additional_registers
                .get_constant(*register_id)
                .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))?
                .map(|constant| {
                    constant
                        .sigma_serialize_bytes()
                        .map_err(|error| ReconciliationError::MalformedBox(error.to_string()))
                })
                .transpose()?;
        }
        Ok(Self {
            canonical_bytes,
            box_id: ergo_box.box_id().to_string(),
            value: *ergo_box.value.as_u64(),
            ergo_tree,
            assets,
            registers,
            creation_height: ergo_box.creation_height,
            transaction_id: ergo_box.transaction_id.to_string(),
            index: ergo_box.index,
        })
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        let rebuilt = Self::from_serialized_bytes(&self.canonical_bytes)?;
        if rebuilt != *self {
            return Err(ReconciliationError::MalformedBox(
                "stored box fields differ from canonical bytes".to_string(),
            ));
        }
        Ok(())
    }

    fn singleton_index(&self, token_id: &str) -> Result<u16, ReconciliationError> {
        let matches: Vec<usize> = self
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.token_id == token_id && asset.amount == 1)
            .map(|(index, _)| index)
            .collect();
        if matches.len() != 1
            || self
                .assets
                .iter()
                .filter(|asset| asset.token_id == token_id)
                .count()
                != 1
        {
            return Err(ReconciliationError::SingletonMismatch);
        }
        u16::try_from(matches[0]).map_err(|_| ReconciliationError::SingletonMismatch)
    }

    pub fn box_id(&self) -> &str {
        &self.box_id
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn ergo_tree(&self) -> &[u8] {
        &self.ergo_tree
    }

    pub fn assets(&self) -> &[ProtocolAsset] {
        &self.assets
    }

    /// Exact serialized R4-R9 values in ABI order.
    pub fn registers(&self) -> &[Option<Vec<u8>>; 6] {
        &self.registers
    }

    pub fn creation_height(&self) -> u32 {
        self.creation_height
    }

    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub fn index(&self) -> u16 {
        self.index
    }
}

/// Full private manifest derived from the signed transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
enum ReconciliationEffect {
    TrackerPublication {
        committed_root: Vec<u8>,
        protocol_nft_id: String,
        protocol_nft_index: u16,
    },
}

/// Private signed intent. Its successor and all payout manifests are derived
/// from the parsed transaction; none is accepted as a caller-supplied box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationIntent {
    tx_id: String,
    signed_transaction_json: Vec<u8>,
    predecessor: ProtocolBox,
    successor_output_index: u16,
    successor: ProtocolBox,
    effect: ReconciliationEffect,
    intent_id: String,
}

impl ReconciliationIntent {
    pub fn tracker_publication(
        signed_transaction_json: Vec<u8>,
        predecessor_box_json: Vec<u8>,
        committed_root: [u8; 33],
    ) -> Result<Self, ReconciliationError> {
        let predecessor = ProtocolBox::from_json_bytes(&predecessor_box_json)?;
        let first_asset = predecessor
            .assets
            .first()
            .filter(|asset| asset.amount == 1)
            .ok_or(ReconciliationError::SingletonMismatch)?;
        let protocol_nft_id = normalize_id(first_asset.token_id.clone(), "protocol NFT id")?;
        let protocol_nft_index = predecessor.singleton_index(&protocol_nft_id)?;
        if protocol_nft_index != 0 {
            return Err(ReconciliationError::SingletonMismatch);
        }
        let transaction = parse_transaction(&signed_transaction_json)?;
        let (successor_output_index, successor) =
            unique_tracker_successor(&transaction, &predecessor, &protocol_nft_id)?;
        let effect = ReconciliationEffect::TrackerPublication {
            committed_root: committed_root.to_vec(),
            protocol_nft_id,
            protocol_nft_index,
        };
        Self::new(
            signed_transaction_json,
            predecessor,
            successor_output_index,
            successor,
            effect,
        )
    }

    fn new(
        signed_transaction_json: Vec<u8>,
        predecessor: ProtocolBox,
        successor_output_index: u16,
        successor: ProtocolBox,
        effect: ReconciliationEffect,
    ) -> Result<Self, ReconciliationError> {
        let tx_id = parse_transaction(&signed_transaction_json)?
            .id()
            .to_string();
        let mut intent = Self {
            tx_id,
            signed_transaction_json,
            predecessor,
            successor_output_index,
            successor,
            effect,
            intent_id: String::new(),
        };
        intent.validate_semantics()?;
        intent.intent_id = intent.compute_intent_id()?;
        Ok(intent)
    }

    fn compute_intent_id(&self) -> Result<String, ReconciliationError> {
        let mut clone = self.clone();
        clone.intent_id.clear();
        let encoded = serde_json::to_vec(&clone)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        Ok(hex::encode(blake2b256_hash(&encoded)))
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        if normalize_id(self.tx_id.clone(), "transaction id")? != self.tx_id
            || normalize_id(self.intent_id.clone(), "intent id")? != self.intent_id
            || self.intent_id != self.compute_intent_id()?
        {
            return Err(ReconciliationError::MalformedIntent(
                "intent identity is inconsistent".to_string(),
            ));
        }
        self.validate_semantics()
    }

    fn validate_semantics(&self) -> Result<(), ReconciliationError> {
        self.predecessor.validate()?;
        self.successor.validate()?;
        let transaction = parse_transaction(&self.signed_transaction_json)?;
        if transaction.id().to_string() != self.tx_id {
            return Err(ReconciliationError::TransactionMismatch);
        }
        if transaction
            .inputs
            .iter()
            .filter(|input| input.box_id.to_string() == self.predecessor.box_id)
            .count()
            != 1
        {
            return Err(ReconciliationError::LineageMismatch);
        }
        if transaction_output(&transaction, self.successor_output_index)? != self.successor {
            return Err(ReconciliationError::SuccessorMismatch);
        }
        match &self.effect {
            ReconciliationEffect::TrackerPublication {
                committed_root,
                protocol_nft_id,
                protocol_nft_index,
            } => {
                ensure_singleton_lineage(
                    &transaction,
                    &self.predecessor,
                    &self.successor,
                    protocol_nft_id,
                    *protocol_nft_index,
                    self.successor_output_index,
                )?;
                if self.successor.value != self.predecessor.value
                    || self.successor.ergo_tree != self.predecessor.ergo_tree
                    || self.successor.assets != self.predecessor.assets
                    || self.successor.registers[0] != self.predecessor.registers[0]
                    || self.successor.registers[2..] != self.predecessor.registers[2..]
                {
                    return Err(ReconciliationError::SuccessorMismatch);
                }
                let predecessor_r5 = self.predecessor.registers[1]
                    .as_ref()
                    .ok_or(ReconciliationError::RootMismatch)?;
                validate_tracker_avl_register(predecessor_r5)?;
                let r5 = self.successor.registers[1]
                    .as_ref()
                    .ok_or(ReconciliationError::RootMismatch)?;
                let derived_root = validate_tracker_avl_register(r5)?;
                if committed_root.len() != 33 || derived_root != committed_root.as_slice() {
                    return Err(ReconciliationError::RootMismatch);
                }
            }
        }
        Ok(())
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn intent_id(&self) -> &str {
        &self.intent_id
    }

    pub fn signed_transaction_json(&self) -> &[u8] {
        &self.signed_transaction_json
    }

    pub fn predecessor(&self) -> &ProtocolBox {
        &self.predecessor
    }

    pub fn successor(&self) -> &ProtocolBox {
        &self.successor
    }

    pub fn tracker_root(&self) -> Option<[u8; 33]> {
        match &self.effect {
            ReconciliationEffect::TrackerPublication { committed_root, .. }
                if committed_root.len() == 33 =>
            {
                let mut root = [0u8; 33];
                root.copy_from_slice(committed_root);
                Some(root)
            }
            _ => None,
        }
    }

    pub fn protocol_nft_id(&self) -> Option<&str> {
        match &self.effect {
            ReconciliationEffect::TrackerPublication {
                protocol_nft_id, ..
            } => Some(protocol_nft_id),
        }
    }
}

fn parse_transaction(bytes: &[u8]) -> Result<Transaction, ReconciliationError> {
    if bytes.is_empty() || bytes.len() > MAX_SIGNED_TRANSACTION_BYTES {
        return Err(ReconciliationError::MalformedIntent(
            "signed transaction bytes are outside the journal bound".to_string(),
        ));
    }
    serde_json::from_slice(bytes)
        .map_err(|error| ReconciliationError::MalformedIntent(error.to_string()))
}

fn transaction_output(
    transaction: &Transaction,
    output_index: u16,
) -> Result<ProtocolBox, ReconciliationError> {
    transaction
        .outputs
        .get(output_index as usize)
        .ok_or(ReconciliationError::SuccessorMismatch)
        .and_then(ProtocolBox::from_ergo_box)
}

fn unique_tracker_successor(
    transaction: &Transaction,
    predecessor: &ProtocolBox,
    protocol_nft_id: &str,
) -> Result<(u16, ProtocolBox), ReconciliationError> {
    let candidates = transaction
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(index, output)| {
            let candidate = ProtocolBox::from_ergo_box(output).ok()?;
            (candidate.ergo_tree == predecessor.ergo_tree
                && candidate
                    .assets
                    .first()
                    .is_some_and(|asset| asset.token_id == protocol_nft_id && asset.amount == 1))
            .then_some((index, candidate))
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(ReconciliationError::SuccessorMismatch);
    }
    let (index, successor) = candidates
        .into_iter()
        .next()
        .ok_or(ReconciliationError::SuccessorMismatch)?;
    Ok((
        u16::try_from(index).map_err(|_| ReconciliationError::SuccessorMismatch)?,
        successor,
    ))
}

fn validate_tracker_avl_register(bytes: &[u8]) -> Result<&[u8], ReconciliationError> {
    if bytes.len() != 37
        || bytes[0] != 0x64
        || bytes[34] != 0x03
        || bytes[35] != 0x20
        || bytes[36] != 0x00
    {
        return Err(ReconciliationError::RootMismatch);
    }
    Ok(&bytes[1..34])
}

fn ensure_singleton_lineage(
    transaction: &Transaction,
    predecessor: &ProtocolBox,
    successor: &ProtocolBox,
    token_id: &str,
    token_index: u16,
    successor_output_index: u16,
) -> Result<(), ReconciliationError> {
    if predecessor.singleton_index(token_id)? != token_index
        || successor.singleton_index(token_id)? != token_index
    {
        return Err(ReconciliationError::SingletonMismatch);
    }
    let carrying_outputs = transaction
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, output)| {
            output.tokens.as_ref().is_some_and(|tokens| {
                tokens
                    .iter()
                    .any(|token| String::from(token.token_id) == token_id)
            })
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if carrying_outputs != vec![successor_output_index as usize] {
        return Err(ReconciliationError::SingletonMismatch);
    }
    Ok(())
}

/// A header whose id was recomputed from its canonical Scorex bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CanonicalHeader {
    bytes: Vec<u8>,
    id: String,
    parent_id: String,
    height: u64,
}

impl CanonicalHeader {
    fn from_header(header: &Header) -> Result<Self, ReconciliationError> {
        let mut json = serde_json::to_value(header)
            .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
        // sigma-rust serializes absent Autolykos-v1-only fields as null, while
        // its own v2 deserializer accepts them only when omitted.
        if header.version > 1 {
            if let Some(solution) = json
                .get_mut("powSolutions")
                .and_then(serde_json::Value::as_object_mut)
            {
                solution.remove("w");
                solution.remove("d");
            }
        }
        let bytes = serde_json::to_vec(&json)
            .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
        let derived = derived_header_id(header)?;
        if derived != header.id.to_string() {
            return Err(ReconciliationError::HeaderIdMismatch);
        }
        Ok(Self {
            bytes,
            id: derived,
            parent_id: header.parent_id.to_string(),
            height: header.height as u64,
        })
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        let header: Header = serde_json::from_slice(&self.bytes)
            .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
        let rebuilt = Self::from_header(&header)?;
        if rebuilt != *self {
            return Err(ReconciliationError::HeaderIdMismatch);
        }
        Ok(())
    }
}

fn derived_header_id(header: &Header) -> Result<String, ReconciliationError> {
    let mut bytes = header
        .serialize_without_pow()
        .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
    header
        .autolykos_solution
        .serialize_bytes(header.version, &mut bytes)
        .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
    Ok(BlockId(header_hash(&bytes)).to_string())
}

/// Coherent selected-chain path returned between two identical node tip
/// snapshots. Every header id is recomputed and every parent link is checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveChainProof {
    inclusion_height: u64,
    selected_through_height: u64,
    tip_id: String,
    tip_height: u64,
    observed_at_unix_ms: u64,
    headers: Vec<CanonicalHeader>,
}

impl ActiveChainProof {
    /// Seal raw `/blocks/chainSlice` JSON with the `/info` tip observed before
    /// and after the request. `toHeight` must have been `tip + 1`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_node_responses(
        before_tip_id: impl Into<String>,
        before_tip_height: u64,
        after_tip_id: impl Into<String>,
        after_tip_height: u64,
        inclusion_height: u64,
        chain_slice_json: &[u8],
        observed_at_unix_ms: u64,
    ) -> Result<Self, ReconciliationError> {
        Self::from_bounded_node_responses(
            before_tip_id,
            before_tip_height,
            after_tip_id,
            after_tip_height,
            inclusion_height,
            before_tip_height,
            chain_slice_json,
            observed_at_unix_ms,
        )
    }

    /// Seal a bounded historical window returned by `/blocks/chainSlice`
    /// between two identical node-tip observations. This is used only to make
    /// the explicit reorg-monitoring horizon decision; transaction acceptance
    /// and ordinary rollback still require a path through the current tip.
    #[allow(clippy::too_many_arguments)]
    pub fn from_bounded_node_responses(
        before_tip_id: impl Into<String>,
        before_tip_height: u64,
        after_tip_id: impl Into<String>,
        after_tip_height: u64,
        inclusion_height: u64,
        selected_through_height: u64,
        chain_slice_json: &[u8],
        observed_at_unix_ms: u64,
    ) -> Result<Self, ReconciliationError> {
        let before_tip_id = normalize_id(before_tip_id.into(), "before tip id")?;
        let after_tip_id = normalize_id(after_tip_id.into(), "after tip id")?;
        if before_tip_id != after_tip_id || before_tip_height != after_tip_height {
            return Err(ReconciliationError::IncoherentSnapshot);
        }
        let parsed: Vec<Header> = serde_json::from_slice(chain_slice_json)
            .map_err(|error| ReconciliationError::MalformedChainProof(error.to_string()))?;
        let headers = parsed
            .iter()
            .map(CanonicalHeader::from_header)
            .collect::<Result<Vec<_>, _>>()?;
        let proof = Self {
            inclusion_height,
            selected_through_height,
            tip_id: before_tip_id,
            tip_height: before_tip_height,
            observed_at_unix_ms,
            headers,
        };
        proof.validate()?;
        Ok(proof)
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        if self.headers.is_empty() || self.headers.len() > MAX_CHAIN_HEADERS {
            return Err(ReconciliationError::MalformedChainProof(
                "chain segment length is outside the bound".to_string(),
            ));
        }
        normalize_id(self.tip_id.clone(), "tip id")?;
        if self.selected_through_height > self.tip_height {
            return Err(ReconciliationError::DepthMismatch);
        }
        let expected_len = self
            .selected_through_height
            .checked_sub(self.inclusion_height)
            .and_then(|depth| depth.checked_add(1))
            .and_then(|length| usize::try_from(length).ok())
            .ok_or(ReconciliationError::DepthMismatch)?;
        if self.headers.len() != expected_len {
            return Err(ReconciliationError::DepthMismatch);
        }
        for (offset, header) in self.headers.iter().enumerate() {
            header.validate()?;
            if header.height != self.inclusion_height + offset as u64 {
                return Err(ReconciliationError::DepthMismatch);
            }
            if offset > 0 && header.parent_id != self.headers[offset - 1].id {
                return Err(ReconciliationError::AncestryMismatch);
            }
        }
        let tip = self.headers.last().ok_or_else(|| {
            ReconciliationError::MalformedChainProof("empty chain segment".to_string())
        })?;
        if tip.height != self.selected_through_height {
            return Err(ReconciliationError::DepthMismatch);
        }
        if self.selected_through_height == self.tip_height && tip.id != self.tip_id {
            return Err(ReconciliationError::IncoherentSnapshot);
        }
        Ok(())
    }

    pub fn inclusion_height(&self) -> u64 {
        self.inclusion_height
    }

    pub fn first_block_id(&self) -> &str {
        &self.headers[0].id
    }

    pub fn tip_id(&self) -> &str {
        &self.tip_id
    }

    pub fn tip_height(&self) -> u64 {
        self.tip_height
    }

    pub fn selected_through_height(&self) -> u64 {
        self.selected_through_height
    }

    pub fn covers_tip(&self) -> bool {
        self.selected_through_height == self.tip_height
    }

    pub fn successor_depth(&self) -> u64 {
        self.tip_height - self.inclusion_height
    }

    pub fn observed_at_unix_ms(&self) -> u64 {
        self.observed_at_unix_ms
    }
}

/// Exact node transaction plus canonical predecessor boxes and its coherent
/// selected-chain proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionChainEvidence {
    transaction_json: Vec<u8>,
    full_block_json: Vec<u8>,
    tx_id: String,
    reported_block_id: String,
    reported_inclusion_height: u64,
    predecessor_boxes: Vec<ProtocolBox>,
    chain: ActiveChainProof,
}

impl TransactionChainEvidence {
    pub fn from_node_snapshot(
        transaction_json: Vec<u8>,
        full_block_json: Vec<u8>,
        reported_block_id: impl Into<String>,
        reported_inclusion_height: u64,
        predecessor_box_json: Vec<Vec<u8>>,
        chain: ActiveChainProof,
    ) -> Result<Self, ReconciliationError> {
        let transaction = parse_transaction(&transaction_json)?;
        let predecessor_boxes = predecessor_box_json
            .iter()
            .map(|bytes| ProtocolBox::from_json_bytes(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let evidence = Self {
            transaction_json,
            full_block_json,
            tx_id: transaction.id().to_string(),
            reported_block_id: normalize_id(reported_block_id.into(), "transaction block id")?,
            reported_inclusion_height,
            predecessor_boxes,
            chain,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        self.chain.validate()?;
        let transaction = parse_transaction(&self.transaction_json)?;
        if transaction.id().to_string() != self.tx_id {
            return Err(ReconciliationError::TransactionMismatch);
        }
        if self.reported_block_id != self.chain.first_block_id()
            || self.reported_inclusion_height != self.chain.inclusion_height()
        {
            return Err(ReconciliationError::InactiveBlock);
        }
        validate_full_block_inclusion(
            &self.full_block_json,
            &transaction,
            self.chain.first_block_id(),
            self.chain.inclusion_height(),
        )?;
        let mut ids = BTreeSet::new();
        for predecessor in &self.predecessor_boxes {
            predecessor.validate()?;
            if !ids.insert(predecessor.box_id.clone())
                || transaction
                    .inputs
                    .iter()
                    .filter(|input| input.box_id.to_string() == predecessor.box_id)
                    .count()
                    != 1
            {
                return Err(ReconciliationError::LineageMismatch);
            }
        }
        Ok(())
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn block_id(&self) -> &str {
        &self.reported_block_id
    }

    pub fn inclusion_height(&self) -> u64 {
        self.reported_inclusion_height
    }

    pub fn chain(&self) -> &ActiveChainProof {
        &self.chain
    }
}

fn validate_full_block_inclusion(
    full_block_json: &[u8],
    expected_transaction: &Transaction,
    expected_block_id: &str,
    expected_height: u64,
) -> Result<(), ReconciliationError> {
    let block: FullBlock = serde_json::from_slice(full_block_json)
        .map_err(|error| ReconciliationError::MalformedBlock(error.to_string()))?;
    let canonical_header = CanonicalHeader::from_header(&block.header)?;
    if canonical_header.id != expected_block_id || canonical_header.height != expected_height {
        return Err(ReconciliationError::InactiveBlock);
    }
    let transactions = block.block_transactions.transactions.as_vec();
    if transactions
        .iter()
        .filter(|transaction| *transaction == expected_transaction)
        .count()
        != 1
    {
        return Err(ReconciliationError::TransactionNotInBlock);
    }
    if transaction_merkle_root(transactions, block.header.version)? != block.header.transaction_root
    {
        return Err(ReconciliationError::TransactionRootMismatch);
    }
    Ok(())
}

/// Exact Ergo `BlockTransactions.transactionsRoot` construction. For modern
/// blocks the witness bytes are appended to the transaction id in the same
/// Merkle leaf; they are not hashed, truncated, or inserted as separate
/// leaves. Version 1 commits only to transaction ids.
fn transaction_merkle_root(
    transactions: &[Transaction],
    block_version: u8,
) -> Result<Digest32, ReconciliationError> {
    let leaves = transactions
        .iter()
        .map(|transaction| {
            let unsigned_bytes = transaction.bytes_to_sign().map_err(|error| {
                ReconciliationError::MalformedBlock(format!(
                    "cannot serialize transaction bytes-to-sign: {error}"
                ))
            })?;
            let transaction_id = blake2b256_hash(&unsigned_bytes);
            if transaction.id().as_ref() != transaction_id.as_slice() {
                return Err(ReconciliationError::TransactionMismatch);
            }
            let mut leaf = transaction_id.to_vec();
            if block_version != 1 {
                leaf.extend(
                    transaction
                        .inputs
                        .iter()
                        .flat_map(|input| input.spending_proof.proof.as_ref().iter().copied()),
                );
            }
            Ok(MerkleNode::from_bytes(leaf))
        })
        .collect::<Result<Vec<_>, ReconciliationError>>()?;
    Ok(MerkleTree::new(leaves).root_hash_special())
}

/// Application finality policy. Depth counts successors, so the tip block has
/// depth zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconciliationPolicy {
    min_successor_depth: u64,
    max_evidence_age_ms: u64,
    reorg_monitor_depth: u64,
}

impl ReconciliationPolicy {
    pub fn new(
        min_successor_depth: u64,
        max_evidence_age_ms: u64,
        reorg_monitor_depth: u64,
    ) -> Self {
        Self {
            min_successor_depth,
            max_evidence_age_ms,
            reorg_monitor_depth,
        }
    }

    pub fn min_successor_depth(&self) -> u64 {
        self.min_successor_depth
    }

    pub fn reorg_monitor_depth(&self) -> u64 {
        self.reorg_monitor_depth
    }

    fn validate(&self) -> Result<(), ReconciliationError> {
        if self.max_evidence_age_ms == 0
            || self.reorg_monitor_depth == 0
            || self.reorg_monitor_depth < self.min_successor_depth
            || self.reorg_monitor_depth > MAX_REORG_MONITOR_DEPTH
        {
            return Err(ReconciliationError::InvalidPolicy);
        }
        Ok(())
    }

    fn validate_freshness(
        &self,
        observed_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<(), ReconciliationError> {
        self.validate()?;
        if now_unix_ms
            .checked_sub(observed_at_unix_ms)
            .is_none_or(|age| age > self.max_evidence_age_ms)
        {
            return Err(ReconciliationError::StaleEvidence);
        }
        Ok(())
    }
}

/// Effect sealed by signed bytes, exact boxes, active-chain ancestry, coherent
/// tip and policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidatedChainEffect {
    intent_id: String,
    tx_id: String,
    block_id: String,
    inclusion_height: u64,
    successor_depth: u64,
    tip_id: String,
    tip_height: u64,
    successor_box_id: String,
    observed_at_unix_ms: u64,
    effect: ReconciliationEffect,
}

#[derive(Deserialize)]
struct StoredValidatedChainEffect {
    intent_id: String,
    tx_id: String,
    block_id: String,
    inclusion_height: u64,
    successor_depth: u64,
    tip_id: String,
    tip_height: u64,
    successor_box_id: String,
    observed_at_unix_ms: u64,
    effect: ReconciliationEffect,
}

impl<'de> Deserialize<'de> for ValidatedChainEffect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if !JournalDeserializationGuard::is_active() {
            return Err(serde::de::Error::custom(
                "validated chain tickets are journal-private",
            ));
        }
        let stored = StoredValidatedChainEffect::deserialize(deserializer)?;
        Ok(Self {
            intent_id: stored.intent_id,
            tx_id: stored.tx_id,
            block_id: stored.block_id,
            inclusion_height: stored.inclusion_height,
            successor_depth: stored.successor_depth,
            tip_id: stored.tip_id,
            tip_height: stored.tip_height,
            successor_box_id: stored.successor_box_id,
            observed_at_unix_ms: stored.observed_at_unix_ms,
            effect: stored.effect,
        })
    }
}

impl ValidatedChainEffect {
    pub fn intent_id(&self) -> &str {
        &self.intent_id
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn block_id(&self) -> &str {
        &self.block_id
    }

    pub fn inclusion_height(&self) -> u64 {
        self.inclusion_height
    }

    pub fn successor_depth(&self) -> u64 {
        self.successor_depth
    }

    pub fn successor_box_id(&self) -> &str {
        &self.successor_box_id
    }

    pub fn tracker_root(&self) -> Option<[u8; 33]> {
        match &self.effect {
            ReconciliationEffect::TrackerPublication { committed_root, .. }
                if committed_root.len() == 33 =>
            {
                let mut root = [0u8; 33];
                root.copy_from_slice(committed_root);
                Some(root)
            }
            _ => None,
        }
    }

    pub fn protocol_nft_id(&self) -> Option<&str> {
        match &self.effect {
            ReconciliationEffect::TrackerPublication {
                protocol_nft_id, ..
            } => Some(protocol_nft_id),
        }
    }
}

#[cfg(test)]
pub(crate) fn validated_tracker_effect_for_test(
    intent_id: String,
    tx_id: String,
    block_id: String,
    successor_box_id: String,
    inclusion_height: u64,
    successor_depth: u64,
    root: [u8; 33],
) -> ValidatedChainEffect {
    ValidatedChainEffect {
        intent_id,
        tx_id,
        block_id,
        inclusion_height,
        successor_depth,
        tip_id: "cc".repeat(32),
        tip_height: inclusion_height + successor_depth,
        successor_box_id,
        observed_at_unix_ms: 1_000,
        effect: ReconciliationEffect::TrackerPublication {
            committed_root: root.to_vec(),
            protocol_nft_id: "dd".repeat(32),
            protocol_nft_index: 0,
        },
    }
}

pub fn validate_chain_effect(
    intent: &ReconciliationIntent,
    evidence: &TransactionChainEvidence,
    policy: ReconciliationPolicy,
    now_unix_ms: u64,
) -> Result<ValidatedChainEffect, ReconciliationError> {
    intent.validate()?;
    evidence.validate()?;
    if !evidence.chain.covers_tip() {
        return Err(ReconciliationError::IncompleteAncestry);
    }
    policy.validate_freshness(evidence.chain.observed_at_unix_ms, now_unix_ms)?;
    if evidence.tx_id != intent.tx_id {
        return Err(ReconciliationError::TransactionMismatch);
    }
    let signed = parse_transaction(&intent.signed_transaction_json)?;
    let observed = parse_transaction(&evidence.transaction_json)?;
    if signed != observed {
        return Err(ReconciliationError::TransactionMismatch);
    }
    if evidence
        .predecessor_boxes
        .iter()
        .filter(|predecessor| predecessor.box_id == intent.predecessor.box_id)
        .count()
        != 1
        || !evidence
            .predecessor_boxes
            .iter()
            .any(|predecessor| predecessor == &intent.predecessor)
    {
        return Err(ReconciliationError::LineageMismatch);
    }
    let observed_successor = transaction_output(&observed, intent.successor_output_index)?;
    if observed_successor != intent.successor {
        return Err(ReconciliationError::SuccessorMismatch);
    }
    let successor_depth = evidence.chain.successor_depth();
    if successor_depth < policy.min_successor_depth {
        return Err(ReconciliationError::DepthTooShallow {
            observed: successor_depth,
            required: policy.min_successor_depth,
        });
    }
    Ok(ValidatedChainEffect {
        intent_id: intent.intent_id.clone(),
        tx_id: intent.tx_id.clone(),
        block_id: evidence.reported_block_id.clone(),
        inclusion_height: evidence.reported_inclusion_height,
        successor_depth,
        tip_id: evidence.chain.tip_id.clone(),
        tip_height: evidence.chain.tip_height,
        successor_box_id: intent.successor.box_id.clone(),
        observed_at_unix_ms: evidence.chain.observed_at_unix_ms,
        effect: intent.effect.clone(),
    })
}

/// Evidence that an earlier accepted block is no longer the first block in a
/// fresh coherent selected-chain segment at the exact inclusion height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidatedRollback {
    intent_id: String,
    tx_id: String,
    removed_block_id: String,
    replacement_block_id: String,
    inclusion_height: u64,
    observed_tip_id: String,
    observed_tip_height: u64,
    observed_at_unix_ms: u64,
}

#[derive(Deserialize)]
struct StoredValidatedRollback {
    intent_id: String,
    tx_id: String,
    removed_block_id: String,
    replacement_block_id: String,
    inclusion_height: u64,
    observed_tip_id: String,
    observed_tip_height: u64,
    observed_at_unix_ms: u64,
}

impl<'de> Deserialize<'de> for ValidatedRollback {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if !JournalDeserializationGuard::is_active() {
            return Err(serde::de::Error::custom(
                "validated rollback tickets are journal-private",
            ));
        }
        let stored = StoredValidatedRollback::deserialize(deserializer)?;
        Ok(Self {
            intent_id: stored.intent_id,
            tx_id: stored.tx_id,
            removed_block_id: stored.removed_block_id,
            replacement_block_id: stored.replacement_block_id,
            inclusion_height: stored.inclusion_height,
            observed_tip_id: stored.observed_tip_id,
            observed_tip_height: stored.observed_tip_height,
            observed_at_unix_ms: stored.observed_at_unix_ms,
        })
    }
}

impl ValidatedRollback {
    pub fn intent_id(&self) -> &str {
        &self.intent_id
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn removed_block_id(&self) -> &str {
        &self.removed_block_id
    }
}

#[cfg(test)]
pub(crate) fn validated_rollback_for_test(effect: &ValidatedChainEffect) -> ValidatedRollback {
    ValidatedRollback {
        intent_id: effect.intent_id.clone(),
        tx_id: effect.tx_id.clone(),
        removed_block_id: effect.block_id.clone(),
        replacement_block_id: "ee".repeat(32),
        inclusion_height: effect.inclusion_height,
        observed_tip_id: "ff".repeat(32),
        observed_tip_height: effect.tip_height + 1,
        observed_at_unix_ms: 1_001,
    }
}

pub fn validate_rollback(
    accepted: &ValidatedChainEffect,
    selected_chain: &ActiveChainProof,
    policy: ReconciliationPolicy,
    now_unix_ms: u64,
) -> Result<ValidatedRollback, ReconciliationError> {
    selected_chain.validate()?;
    if !selected_chain.covers_tip() {
        return Err(ReconciliationError::IncompleteAncestry);
    }
    policy.validate_freshness(selected_chain.observed_at_unix_ms, now_unix_ms)?;
    if selected_chain.inclusion_height != accepted.inclusion_height {
        return Err(ReconciliationError::RollbackNotProven);
    }
    let replacement = selected_chain.first_block_id();
    if replacement == accepted.block_id {
        return Err(ReconciliationError::RollbackNotProven);
    }
    Ok(ValidatedRollback {
        intent_id: accepted.intent_id.clone(),
        tx_id: accepted.tx_id.clone(),
        removed_block_id: accepted.block_id.clone(),
        replacement_block_id: replacement.to_string(),
        inclusion_height: accepted.inclusion_height,
        observed_tip_id: selected_chain.tip_id.clone(),
        observed_tip_height: selected_chain.tip_height,
        observed_at_unix_ms: selected_chain.observed_at_unix_ms,
    })
}

/// Validate an accepted anchor against the same coherent-chain authority used
/// for rollback detection.
pub fn validate_anchor_still_active(
    accepted: &ValidatedChainEffect,
    selected_chain: &ActiveChainProof,
    policy: ReconciliationPolicy,
    now_unix_ms: u64,
) -> Result<(), ReconciliationError> {
    selected_chain.validate()?;
    if !selected_chain.covers_tip() {
        return Err(ReconciliationError::IncompleteAncestry);
    }
    policy.validate_freshness(selected_chain.observed_at_unix_ms, now_unix_ms)?;
    if selected_chain.inclusion_height != accepted.inclusion_height
        || selected_chain.first_block_id() != accepted.block_id
    {
        return Err(ReconciliationError::InactiveBlock);
    }
    Ok(())
}

/// Private authorization to stop polling an accepted anchor after it has
/// remained on the selected chain for the complete configured reorg horizon.
/// Generic serde callers cannot mint this ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidatedRetirement {
    intent_id: String,
    tx_id: String,
    block_id: String,
    inclusion_height: u64,
    monitor_depth: u64,
    observed_tip_id: String,
    observed_tip_height: u64,
    observed_at_unix_ms: u64,
}

#[derive(Deserialize)]
struct StoredValidatedRetirement {
    intent_id: String,
    tx_id: String,
    block_id: String,
    inclusion_height: u64,
    monitor_depth: u64,
    observed_tip_id: String,
    observed_tip_height: u64,
    observed_at_unix_ms: u64,
}

impl<'de> Deserialize<'de> for ValidatedRetirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if !JournalDeserializationGuard::is_active() {
            return Err(serde::de::Error::custom(
                "validated retirement tickets are journal-private",
            ));
        }
        let stored = StoredValidatedRetirement::deserialize(deserializer)?;
        Ok(Self {
            intent_id: stored.intent_id,
            tx_id: stored.tx_id,
            block_id: stored.block_id,
            inclusion_height: stored.inclusion_height,
            monitor_depth: stored.monitor_depth,
            observed_tip_id: stored.observed_tip_id,
            observed_tip_height: stored.observed_tip_height,
            observed_at_unix_ms: stored.observed_at_unix_ms,
        })
    }
}

/// Result of checking the bounded selected-chain window at the configured
/// monitoring horizon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReorgHorizonDecision {
    Retire(ValidatedRetirement),
    Rollback(ValidatedRollback),
}

/// Decide an old anchor using an inclusive, bounded selected-chain window.
///
/// This is deliberately distinct from acceptance: it cannot confirm a new
/// transaction. The window must cover exactly `inclusion..=inclusion+horizon`
/// and must have been returned between two identical node-tip observations.
/// A matching first header authorizes durable retirement; a different first
/// header authorizes rollback before any newer ticket is processed.
pub fn validate_reorg_horizon(
    accepted: &ValidatedChainEffect,
    selected_window: &ActiveChainProof,
    policy: ReconciliationPolicy,
    now_unix_ms: u64,
) -> Result<ReorgHorizonDecision, ReconciliationError> {
    selected_window.validate()?;
    policy.validate_freshness(selected_window.observed_at_unix_ms, now_unix_ms)?;
    if selected_window.inclusion_height != accepted.inclusion_height {
        return Err(ReconciliationError::RollbackNotProven);
    }
    let expected_through = accepted
        .inclusion_height
        .checked_add(policy.reorg_monitor_depth)
        .ok_or(ReconciliationError::DepthMismatch)?;
    if selected_window.selected_through_height != expected_through
        || selected_window.tip_height < expected_through
    {
        return Err(ReconciliationError::IncompleteAncestry);
    }
    if selected_window.first_block_id() != accepted.block_id {
        return Ok(ReorgHorizonDecision::Rollback(ValidatedRollback {
            intent_id: accepted.intent_id.clone(),
            tx_id: accepted.tx_id.clone(),
            removed_block_id: accepted.block_id.clone(),
            replacement_block_id: selected_window.first_block_id().to_string(),
            inclusion_height: accepted.inclusion_height,
            observed_tip_id: selected_window.tip_id.clone(),
            observed_tip_height: selected_window.tip_height,
            observed_at_unix_ms: selected_window.observed_at_unix_ms,
        }));
    }
    Ok(ReorgHorizonDecision::Retire(ValidatedRetirement {
        intent_id: accepted.intent_id.clone(),
        tx_id: accepted.tx_id.clone(),
        block_id: accepted.block_id.clone(),
        inclusion_height: accepted.inclusion_height,
        monitor_depth: policy.reorg_monitor_depth,
        observed_tip_id: selected_window.tip_id.clone(),
        observed_tip_height: selected_window.tip_height,
        observed_at_unix_ms: selected_window.observed_at_unix_ms,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PendingPhase {
    Prepared,
    SubmissionArmed,
    AcceptanceReady,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingTicket {
    intent: ReconciliationIntent,
    phase: PendingPhase,
    accepted_effect: Option<ValidatedChainEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AcceptedAnchor {
    effect: ValidatedChainEffect,
    rollback: Option<ValidatedRollback>,
    applied: bool,
    #[serde(default)]
    retirement: Option<ValidatedRetirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct JournalEvent {
    sequence: u64,
    event_id: String,
    kind: String,
    tx_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct JournalState {
    sequence: u64,
    pending: Option<PendingTicket>,
    accepted: Option<AcceptedAnchor>,
    history: Vec<JournalEvent>,
}

/// Recovery action chosen exclusively from durable journal state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    Idle,
    SubmitPrepared(ReconciliationIntent),
    QueryExactTransaction(ReconciliationIntent),
    ApplyAccepted(ValidatedChainEffect),
    RevalidateAccepted(ValidatedChainEffect),
    RestoreRetired(ValidatedChainEffect),
    ApplyRollback(ValidatedRollback),
}

/// Single-writer durable journal for publication and settlement tickets.
pub struct ReconciliationJournal {
    keyspace: Keyspace,
    partition: fjall::Partition,
    binding: ReconciliationJournalBinding,
    write_lock: Mutex<()>,
    _writer_file_lock: File,
}

impl ReconciliationJournal {
    pub fn open(
        path: impl AsRef<Path>,
        binding: ReconciliationJournalBinding,
        bootstrap: JournalBootstrap,
    ) -> Result<Self, ReconciliationError> {
        let path = path.as_ref();
        ensure_journal_manifest(path, &binding, bootstrap)?;
        let lock_path = path.join(".confirmed-chain-writer.lock");
        let writer_file_lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        writer_file_lock.try_lock_exclusive().map_err(|error| {
            ReconciliationError::Journal(format!(
                "confirmed-chain journal already has an active writer ({}): {}",
                lock_path.display(),
                error
            ))
        })?;
        let keyspace = Config::new(path)
            .open()
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        let partition = keyspace
            .open_partition("confirmed_chain", PartitionCreateOptions::default())
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        let journal = Self {
            keyspace,
            partition,
            binding,
            write_lock: Mutex::new(()),
            _writer_file_lock: writer_file_lock,
        };
        journal.read_state()?;
        Ok(journal)
    }

    pub fn recovery_action(&self) -> Result<RecoveryAction, ReconciliationError> {
        let state = self.read_state()?;
        Self::recovery_action_from_state(&state)
    }

    fn recovery_action_from_state(
        state: &JournalState,
    ) -> Result<RecoveryAction, ReconciliationError> {
        // A validated reorg must demote the old accounting projection before
        // any newer signed ticket is submitted or queried. The pending ticket
        // remains durable and resumes after `mark_rollback_applied`.
        if let Some(rollback) = state
            .accepted
            .as_ref()
            .and_then(|anchor| anchor.rollback.clone())
        {
            return Ok(RecoveryAction::ApplyRollback(rollback));
        }
        if let Some(pending) = &state.pending {
            return Ok(match (pending.phase, pending.accepted_effect.clone()) {
                (PendingPhase::Prepared, _) => {
                    RecoveryAction::SubmitPrepared(pending.intent.clone())
                }
                (PendingPhase::SubmissionArmed, _) => {
                    RecoveryAction::QueryExactTransaction(pending.intent.clone())
                }
                (PendingPhase::AcceptanceReady, Some(effect)) => {
                    RecoveryAction::ApplyAccepted(effect)
                }
                (PendingPhase::AcceptanceReady, None) => {
                    return Err(ReconciliationError::Journal(
                        "acceptance-ready ticket has no validated effect".to_string(),
                    ));
                }
            });
        }
        if let Some(anchor) = &state.accepted {
            return Ok(
                match (
                    anchor.applied,
                    anchor.rollback.as_ref(),
                    anchor.retirement.as_ref(),
                ) {
                    (_, Some(_), _) => {
                        return Err(ReconciliationError::Journal(
                            "rollback priority invariant was bypassed".to_string(),
                        ));
                    }
                    (false, None, _) => RecoveryAction::ApplyAccepted(anchor.effect.clone()),
                    (true, None, Some(_)) => RecoveryAction::RestoreRetired(anchor.effect.clone()),
                    (true, None, None) => RecoveryAction::RevalidateAccepted(anchor.effect.clone()),
                },
            );
        }
        Ok(RecoveryAction::Idle)
    }

    /// Validate the complete crash-recovery join between BNS1 accounting
    /// state, its exact BPA1 in-flight receipt, and this sealed journal before
    /// any node request is permitted.
    pub fn validate_tracker_startup_join(
        &self,
        restored_pending: Option<&(String, [u8; 33])>,
        historical_confirmation: Option<&ConfirmedProjectionAnchor>,
    ) -> Result<(), ReconciliationError> {
        let state = self.read_state()?;
        let action = Self::recovery_action_from_state(&state)?;
        let receipt_matches = |tx_id: &str, root: [u8; 33]| {
            restored_pending.is_some_and(|(restored_tx_id, restored_root)| {
                restored_tx_id.eq_ignore_ascii_case(tx_id) && *restored_root == root
            })
        };
        let intent_receipt_matches = |intent: &ReconciliationIntent| {
            intent
                .tracker_root()
                .is_some_and(|root| receipt_matches(intent.tx_id(), root))
        };
        let effect_receipt_matches = |effect: &ValidatedChainEffect| {
            effect
                .tracker_root()
                .is_some_and(|root| receipt_matches(effect.tx_id(), root))
        };

        match &action {
            RecoveryAction::SubmitPrepared(intent)
            | RecoveryAction::QueryExactTransaction(intent) => {
                if !intent_receipt_matches(intent) {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
            }
            RecoveryAction::ApplyAccepted(effect) => {
                let receipt_is_exact = effect_receipt_matches(effect);
                if restored_pending.is_some() && !receipt_is_exact {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
                let projection_is_already_applied = historical_confirmation
                    .is_some_and(|anchor| anchor.matches_validated_effect(effect));
                if !receipt_is_exact && !projection_is_already_applied {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
            }
            RecoveryAction::ApplyRollback(_) => match &state.pending {
                Some(pending) if !intent_receipt_matches(&pending.intent) => {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
                None if restored_pending.is_some() => {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
                _ => {}
            },
            RecoveryAction::RevalidateAccepted(_)
            | RecoveryAction::RestoreRetired(_)
            | RecoveryAction::Idle => {
                if restored_pending.is_some() {
                    return Err(ReconciliationError::AccountingProjectionMismatch);
                }
            }
        }

        let mut candidates = Vec::with_capacity(2);
        if let Some(accepted) = &state.accepted {
            candidates.push(&accepted.effect);
        }
        if let Some(effect) = state.pending.as_ref().and_then(|pending| {
            (pending.phase == PendingPhase::AcceptanceReady)
                .then_some(pending.accepted_effect.as_ref())
                .flatten()
        }) {
            if !candidates.contains(&effect) {
                candidates.push(effect);
            }
        }
        if let Some(anchor) = historical_confirmation {
            if !candidates
                .iter()
                .any(|effect| anchor.matches_validated_effect(effect))
            {
                return Err(ReconciliationError::AccountingProjectionMismatch);
            }
        } else {
            let accepted_was_applied = state
                .accepted
                .as_ref()
                .is_some_and(|accepted| accepted.applied);
            let safe_absence = matches!(&action, RecoveryAction::ApplyRollback(_))
                || matches!(&action, RecoveryAction::ApplyAccepted(effect) if effect_receipt_matches(effect));
            if accepted_was_applied && !safe_absence {
                return Err(ReconciliationError::AccountingProjectionMismatch);
            }
        }
        Ok(())
    }

    pub fn pending_intent(&self) -> Result<Option<ReconciliationIntent>, ReconciliationError> {
        Ok(self.read_state()?.pending.map(|ticket| ticket.intent))
    }

    pub fn accepted_effect(&self) -> Result<Option<ValidatedChainEffect>, ReconciliationError> {
        Ok(self
            .read_state()?
            .accepted
            .filter(|anchor| anchor.retirement.is_none())
            .map(|anchor| anchor.effect))
    }

    /// Effects which may legitimately describe the actor's persisted
    /// accounting projection at startup. A crash may leave either the older
    /// accepted anchor or the newer acceptance-ready effect applied locally.
    pub fn accounting_effect_candidates(
        &self,
    ) -> Result<Vec<ValidatedChainEffect>, ReconciliationError> {
        let state = self.read_state()?;
        let mut candidates = Vec::with_capacity(2);
        if let Some(accepted) = state.accepted {
            candidates.push(accepted.effect);
        }
        if let Some(effect) = state.pending.and_then(|pending| {
            (pending.phase == PendingPhase::AcceptanceReady)
                .then_some(pending.accepted_effect)
                .flatten()
        }) {
            if !candidates.contains(&effect) {
                candidates.push(effect);
            }
        }
        Ok(candidates)
    }

    pub fn record_prepared(&self, intent: ReconciliationIntent) -> Result<(), ReconciliationError> {
        intent.validate()?;
        self.ensure_bound_nft(intent.protocol_nft_id())?;
        self.mutate(|state| {
            if let Some(existing) = &state.pending {
                return if existing.intent == intent {
                    Ok(false)
                } else if existing.intent.tx_id == intent.tx_id {
                    Err(ReconciliationError::DuplicateTransactionConflict)
                } else {
                    Err(ReconciliationError::TicketInProgress)
                };
            }
            if state
                .accepted
                .as_ref()
                .is_some_and(|accepted| accepted.effect.tx_id == intent.tx_id)
            {
                return Err(ReconciliationError::DuplicateTransactionConflict);
            }
            state.pending = Some(PendingTicket {
                intent: intent.clone(),
                phase: PendingPhase::Prepared,
                accepted_effect: None,
            });
            append_event(state, "prepared", intent.tx_id())?;
            Ok(true)
        })
    }

    /// Persist immediately before a request may cross the node boundary.
    pub fn arm_submission(&self, intent_id: &str) -> Result<(), ReconciliationError> {
        self.mutate(|state| {
            let pending = state
                .pending
                .as_mut()
                .ok_or(ReconciliationError::NoTicket)?;
            if pending.intent.intent_id != intent_id {
                return Err(ReconciliationError::IntentMismatch);
            }
            match pending.phase {
                PendingPhase::Prepared => {
                    pending.phase = PendingPhase::SubmissionArmed;
                    let tx_id = pending.intent.tx_id.clone();
                    append_event(state, "submission_armed", &tx_id)?;
                    Ok(true)
                }
                PendingPhase::SubmissionArmed => Ok(false),
                PendingPhase::AcceptanceReady => Err(ReconciliationError::InvalidPhase),
            }
        })
    }

    pub fn record_validated_effect(
        &self,
        effect: ValidatedChainEffect,
    ) -> Result<(), ReconciliationError> {
        self.ensure_bound_nft(effect.protocol_nft_id())?;
        self.mutate(|state| {
            let pending = state
                .pending
                .as_mut()
                .ok_or(ReconciliationError::NoTicket)?;
            if pending.intent.intent_id != effect.intent_id || pending.intent.tx_id != effect.tx_id
            {
                return Err(ReconciliationError::IntentMismatch);
            }
            if pending.phase == PendingPhase::AcceptanceReady {
                return if pending.accepted_effect.as_ref() == Some(&effect) {
                    Ok(false)
                } else {
                    Err(ReconciliationError::DuplicateTransactionConflict)
                };
            }
            if pending.phase != PendingPhase::SubmissionArmed {
                return Err(ReconciliationError::InvalidPhase);
            }
            pending.phase = PendingPhase::AcceptanceReady;
            pending.accepted_effect = Some(effect.clone());
            append_event(state, "policy_accepted", &effect.tx_id)?;
            Ok(true)
        })
    }

    pub fn mark_applied(&self, effect: &ValidatedChainEffect) -> Result<(), ReconciliationError> {
        self.mutate(|state| {
            if state.accepted.as_ref().is_some_and(|anchor| {
                anchor.applied && anchor.effect == *effect && anchor.rollback.is_none()
            }) && state.pending.is_none()
            {
                return Ok(false);
            }
            let pending = state
                .pending
                .as_ref()
                .ok_or(ReconciliationError::NoTicket)?;
            if pending.phase != PendingPhase::AcceptanceReady
                || pending.accepted_effect.as_ref() != Some(effect)
            {
                return Err(ReconciliationError::InvalidPhase);
            }
            state.accepted = Some(AcceptedAnchor {
                effect: effect.clone(),
                rollback: None,
                applied: true,
                retirement: None,
            });
            state.pending = None;
            append_event(state, "applied", &effect.tx_id)?;
            Ok(true)
        })
    }

    pub fn record_rollback(&self, rollback: ValidatedRollback) -> Result<(), ReconciliationError> {
        self.mutate(|state| {
            let anchor = state
                .accepted
                .as_mut()
                .ok_or(ReconciliationError::NoTicket)?;
            if anchor.retirement.is_some() {
                return Err(ReconciliationError::InvalidPhase);
            }
            if anchor.effect.intent_id != rollback.intent_id
                || anchor.effect.tx_id != rollback.tx_id
                || anchor.effect.block_id != rollback.removed_block_id
            {
                return Err(ReconciliationError::IntentMismatch);
            }
            if anchor.rollback.as_ref() == Some(&rollback) {
                return Ok(false);
            }
            if anchor.rollback.is_some() {
                return Err(ReconciliationError::DuplicateTransactionConflict);
            }
            anchor.rollback = Some(rollback.clone());
            append_event(state, "rollback_detected", &rollback.tx_id)?;
            Ok(true)
        })
    }

    /// Stop active reorg monitoring only after a bounded selected-chain window
    /// proved that this anchor survived the configured application horizon.
    pub fn retire_accepted(
        &self,
        retirement: &ValidatedRetirement,
    ) -> Result<(), ReconciliationError> {
        self.mutate(|state| {
            let anchor = state
                .accepted
                .as_mut()
                .ok_or(ReconciliationError::NoTicket)?;
            if anchor.effect.intent_id != retirement.intent_id
                || anchor.effect.tx_id != retirement.tx_id
                || anchor.effect.block_id != retirement.block_id
                || anchor.effect.inclusion_height != retirement.inclusion_height
                || !anchor.applied
                || anchor.rollback.is_some()
            {
                return Err(ReconciliationError::IntentMismatch);
            }
            if let Some(existing) = &anchor.retirement {
                return if existing == retirement {
                    Ok(false)
                } else {
                    Err(ReconciliationError::DuplicateTransactionConflict)
                };
            }
            anchor.retirement = Some(retirement.clone());
            append_event(state, "reorg_horizon_retired", &retirement.tx_id)?;
            Ok(true)
        })
    }

    pub fn mark_rollback_applied(
        &self,
        rollback: &ValidatedRollback,
    ) -> Result<(), ReconciliationError> {
        self.mutate(|state| {
            let anchor = state
                .accepted
                .as_ref()
                .ok_or(ReconciliationError::NoTicket)?;
            if anchor.rollback.as_ref() != Some(rollback) {
                return Err(ReconciliationError::IntentMismatch);
            }
            let tx_id = rollback.tx_id.clone();
            state.accepted = None;
            append_event(state, "rollback_applied", &tx_id)?;
            Ok(true)
        })
    }

    fn read_state(&self) -> Result<JournalState, ReconciliationError> {
        let state = match self
            .partition
            .get(JOURNAL_KEY)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?
        {
            Some(bytes) => deserialize_state(bytes.as_ref()),
            None => Ok(JournalState::default()),
        }?;
        self.validate_bound_state(&state)?;
        Ok(state)
    }

    fn mutate(
        &self,
        change: impl FnOnce(&mut JournalState) -> Result<bool, ReconciliationError>,
    ) -> Result<(), ReconciliationError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ReconciliationError::Journal("journal lock is poisoned".to_string()))?;
        let mut state = self.read_state()?;
        if !change(&mut state)? {
            return Ok(());
        }
        validate_state(&state)?;
        self.validate_bound_state(&state)?;
        let bytes = serialize_state(&state)?;
        self.partition
            .insert(JOURNAL_KEY, bytes)
            .map_err(|error| ReconciliationError::OutcomeUnknown(error.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|error| ReconciliationError::OutcomeUnknown(error.to_string()))
    }

    fn ensure_bound_nft(&self, nft_id: Option<&str>) -> Result<(), ReconciliationError> {
        let expected = hex::encode(self.binding.tracker_nft_id);
        if nft_id != Some(expected.as_str()) {
            return Err(ReconciliationError::JournalBindingMismatch);
        }
        Ok(())
    }

    fn validate_bound_state(&self, state: &JournalState) -> Result<(), ReconciliationError> {
        if let Some(pending) = &state.pending {
            self.ensure_bound_nft(pending.intent.protocol_nft_id())?;
            if let Some(effect) = &pending.accepted_effect {
                self.ensure_bound_nft(effect.protocol_nft_id())?;
            }
        }
        if let Some(accepted) = &state.accepted {
            self.ensure_bound_nft(accepted.effect.protocol_nft_id())?;
        }
        Ok(())
    }
}

fn ensure_journal_manifest(
    path: &Path,
    expected: &ReconciliationJournalBinding,
    bootstrap: JournalBootstrap,
) -> Result<(), ReconciliationError> {
    let manifest_path = path.join(JOURNAL_MANIFEST_FILE);
    if manifest_path.exists() {
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        let observed = deserialize_manifest(&bytes)?;
        return if observed == *expected {
            Ok(())
        } else {
            Err(ReconciliationError::JournalBindingMismatch)
        };
    }
    if bootstrap == JournalBootstrap::ExistingRequired {
        return Err(ReconciliationError::JournalBindingRequired);
    }
    if path.exists() {
        let mut entries = std::fs::read_dir(path)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
        if entries
            .next()
            .transpose()
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?
            .is_some()
        {
            return Err(ReconciliationError::JournalBindingRequired);
        }
    } else {
        std::fs::create_dir_all(path)
            .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
    }
    let bytes = serialize_manifest(expected)?;
    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&manifest_path)
    {
        Ok(mut file) => {
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| ReconciliationError::OutcomeUnknown(error.to_string()))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let observed = std::fs::read(&manifest_path)
                .map_err(|read_error| ReconciliationError::Journal(read_error.to_string()))?;
            if deserialize_manifest(&observed)? == *expected {
                Ok(())
            } else {
                Err(ReconciliationError::JournalBindingMismatch)
            }
        }
        Err(error) => Err(ReconciliationError::Journal(error.to_string())),
    }
}

fn serialize_manifest(
    binding: &ReconciliationJournalBinding,
) -> Result<Vec<u8>, ReconciliationError> {
    let payload = serde_json::to_vec(binding)
        .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| ReconciliationError::Journal("manifest is too large".to_string()))?;
    let mut bytes = Vec::with_capacity(8 + payload.len() + 32);
    bytes.extend_from_slice(JOURNAL_MANIFEST_MAGIC);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&payload);
    let mut checksum_input =
        Vec::with_capacity(JOURNAL_MANIFEST_CHECKSUM_DOMAIN.len() + bytes.len());
    checksum_input.extend_from_slice(JOURNAL_MANIFEST_CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&bytes);
    bytes.extend_from_slice(&blake2b256_hash(&checksum_input));
    Ok(bytes)
}

fn deserialize_manifest(bytes: &[u8]) -> Result<ReconciliationJournalBinding, ReconciliationError> {
    if bytes.len() < 8 + 32 || &bytes[..4] != JOURNAL_MANIFEST_MAGIC {
        return Err(ReconciliationError::Journal(
            "journal manifest envelope is malformed".to_string(),
        ));
    }
    let length = u32::from_be_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| ReconciliationError::Journal("invalid manifest length".to_string()))?,
    ) as usize;
    if bytes.len() != 8 + length + 32 {
        return Err(ReconciliationError::Journal(
            "journal manifest envelope length mismatch".to_string(),
        ));
    }
    let checksum_offset = 8 + length;
    let mut checksum_input =
        Vec::with_capacity(JOURNAL_MANIFEST_CHECKSUM_DOMAIN.len() + checksum_offset);
    checksum_input.extend_from_slice(JOURNAL_MANIFEST_CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&bytes[..checksum_offset]);
    if blake2b256_hash(&checksum_input) != bytes[checksum_offset..] {
        return Err(ReconciliationError::Journal(
            "journal manifest checksum mismatch".to_string(),
        ));
    }
    serde_json::from_slice(&bytes[8..checksum_offset])
        .map_err(|error| ReconciliationError::Journal(error.to_string()))
}

fn append_event(
    state: &mut JournalState,
    kind: &str,
    tx_id: &str,
) -> Result<(), ReconciliationError> {
    state.sequence = state
        .sequence
        .checked_add(1)
        .ok_or_else(|| ReconciliationError::Journal("journal sequence overflow".to_string()))?;
    let event_material = format!("{}:{}:{}", state.sequence, kind, tx_id);
    state.history.push(JournalEvent {
        sequence: state.sequence,
        event_id: hex::encode(blake2b256_hash(event_material.as_bytes())),
        kind: kind.to_string(),
        tx_id: tx_id.to_string(),
    });
    if state.history.len() > MAX_HISTORY_ENTRIES {
        let remove = state.history.len() - MAX_HISTORY_ENTRIES;
        state.history.drain(..remove);
    }
    Ok(())
}

fn validate_state(state: &JournalState) -> Result<(), ReconciliationError> {
    let mut prior = 0u64;
    for event in &state.history {
        if event.sequence <= prior
            || event.sequence > state.sequence
            || normalize_id(event.event_id.clone(), "event id")? != event.event_id
            || normalize_id(event.tx_id.clone(), "event tx id")? != event.tx_id
        {
            return Err(ReconciliationError::Journal(
                "journal history is malformed".to_string(),
            ));
        }
        prior = event.sequence;
    }
    if let Some(pending) = &state.pending {
        pending.intent.validate()?;
        if pending.phase == PendingPhase::AcceptanceReady && pending.accepted_effect.is_none() {
            return Err(ReconciliationError::Journal(
                "acceptance-ready ticket lacks evidence".to_string(),
            ));
        }
    }
    if let Some(accepted) = &state.accepted {
        normalize_id(accepted.effect.intent_id.clone(), "accepted intent id")?;
    }
    Ok(())
}

fn serialize_state(state: &JournalState) -> Result<Vec<u8>, ReconciliationError> {
    let payload = serde_json::to_vec(state)
        .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| ReconciliationError::Journal("journal payload is too large".to_string()))?;
    let mut bytes = Vec::with_capacity(8 + payload.len() + 32);
    bytes.extend_from_slice(JOURNAL_MAGIC);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&payload);
    let mut checksum_input = Vec::with_capacity(JOURNAL_CHECKSUM_DOMAIN.len() + bytes.len());
    checksum_input.extend_from_slice(JOURNAL_CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&bytes);
    bytes.extend_from_slice(&blake2b256_hash(&checksum_input));
    Ok(bytes)
}

fn deserialize_state(bytes: &[u8]) -> Result<JournalState, ReconciliationError> {
    if bytes.len() < 8 + 32 || &bytes[..4] != JOURNAL_MAGIC {
        return Err(ReconciliationError::Journal(
            "journal envelope is malformed".to_string(),
        ));
    }
    let length = u32::from_be_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| ReconciliationError::Journal("invalid journal length".to_string()))?,
    ) as usize;
    if bytes.len() != 8 + length + 32 {
        return Err(ReconciliationError::Journal(
            "journal envelope length mismatch".to_string(),
        ));
    }
    let checksum_offset = 8 + length;
    let mut checksum_input = Vec::with_capacity(JOURNAL_CHECKSUM_DOMAIN.len() + checksum_offset);
    checksum_input.extend_from_slice(JOURNAL_CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&bytes[..checksum_offset]);
    if blake2b256_hash(&checksum_input) != bytes[checksum_offset..] {
        return Err(ReconciliationError::Journal(
            "journal checksum mismatch".to_string(),
        ));
    }
    let _guard = JournalDeserializationGuard::enter();
    let state: JournalState = serde_json::from_slice(&bytes[8..checksum_offset])
        .map_err(|error| ReconciliationError::Journal(error.to_string()))?;
    validate_state(&state)?;
    Ok(state)
}

fn normalize_id(value: String, label: &str) -> Result<String, ReconciliationError> {
    let normalized = value.to_ascii_lowercase();
    if normalized.len() != 64
        || hex::decode(&normalized)
            .map(|bytes| bytes.len() != 32)
            .unwrap_or(true)
    {
        return Err(ReconciliationError::MalformedIntent(format!(
            "{label} is not a 32-byte hex id"
        )));
    }
    Ok(normalized)
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ReconciliationError {
    #[error("malformed reconciliation intent: {0}")]
    MalformedIntent(String),
    #[error("malformed canonical protocol box: {0}")]
    MalformedBox(String),
    #[error("malformed active-chain proof: {0}")]
    MalformedChainProof(String),
    #[error("malformed full block: {0}")]
    MalformedBlock(String),
    #[error("transaction identity or exact body does not match the signed intent")]
    TransactionMismatch,
    #[error("exact transaction is absent from the selected full block")]
    TransactionNotInBlock,
    #[error("full block transaction list does not match the header transactions root")]
    TransactionRootMismatch,
    #[error("transaction does not consume the exact canonical predecessor")]
    LineageMismatch,
    #[error("transaction has no exact signed protocol successor")]
    SuccessorMismatch,
    #[error("protocol singleton lineage or token position is invalid")]
    SingletonMismatch,
    #[error("successor root does not match the signed intent")]
    RootMismatch,
    #[error("settlement amount does not match the signed chain-derived delta")]
    AmountMismatch,
    #[error("ordered asset manifest changed outside the settlement delta")]
    AssetManifestMismatch,
    #[error("signed payout outputs do not exactly equal the reserve delta")]
    PayoutMismatch,
    #[error("transaction block is not selected at its inclusion height")]
    InactiveBlock,
    #[error("header id does not match canonical header bytes")]
    HeaderIdMismatch,
    #[error("selected header chain has a broken parent link")]
    AncestryMismatch,
    #[error("node tip changed while chain evidence was collected")]
    IncoherentSnapshot,
    #[error("transaction depth evidence is inconsistent")]
    DepthMismatch,
    #[error("selected-chain evidence does not cover the required ancestry boundary")]
    IncompleteAncestry,
    #[error("reconciliation policy has an invalid finality or monitoring horizon")]
    InvalidPolicy,
    #[error("successor depth {observed} is below policy depth {required}")]
    DepthTooShallow { observed: u64, required: u64 },
    #[error("chain evidence is stale")]
    StaleEvidence,
    #[error("v2 reconciliation is disabled until explicit activation")]
    GenerationDisabled,
    #[error("rollback was not proven by a coherent replacement chain")]
    RollbackNotProven,
    #[error("a different reconciliation ticket is already pending")]
    TicketInProgress,
    #[error("the same transaction id is bound to a different ticket")]
    DuplicateTransactionConflict,
    #[error("no reconciliation ticket exists")]
    NoTicket,
    #[error("reconciliation ticket identity mismatch")]
    IntentMismatch,
    #[error("reconciliation ticket is in the wrong phase")]
    InvalidPhase,
    #[error("durable reconciliation journal error: {0}")]
    Journal(String),
    #[error("confirmed-chain journal manifest is required for existing tracker history")]
    JournalBindingRequired,
    #[error("confirmed-chain journal is bound to a different tracker generation")]
    JournalBindingMismatch,
    #[error("BNS1 accounting projection and confirmed-chain journal do not join exactly")]
    AccountingProjectionMismatch,
    #[error("durable reconciliation outcome is unknown: {0}")]
    OutcomeUnknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::{
        chain::transaction::{Input, UnsignedInput},
        ergotree_interpreter::sigma_protocol::prover::ProofBytes,
        ergotree_ir::{
            chain::{
                context_extension::ContextExtension,
                ergo_box::{
                    box_value::BoxValue, BoxTokens, ErgoBoxCandidate, NonMandatoryRegisters,
                    RegisterValue,
                },
                token::{Token, TokenAmount, TokenId},
                tx_id::TxId,
            },
            ergo_tree::ErgoTree,
        },
    };

    const HEADER_JSON: &str = r#"{
      "extensionId":"d16f25b14457186df4c5f6355579cc769261ce1aebc8209949ca6feadbac5a3f",
      "votes":"040000","timestamp":1618929697400,
      "stateRoot":"8ad868627ea4f7de6e2a2fe3f98fafe57f914e0f2ef3331c006def36c697f92713",
      "height":471746,"nBits":117586360,"version":2,
      "id":"4caa17e62fe66ba7bd69597afdc996ae35b1ff12e0ba90c22ff288a4de10e91b",
      "adProofsRoot":"d882aaf42e0a95eb95fcce5c3705adf758e591532f733efe790ac3c404730c39",
      "transactionsRoot":"63eaa9aff76a1de3d71c81e4b2d92e8d97ae572a8e9ab9e66599ed0912dd2f8b",
      "extensionHash":"3f91f3c680beb26615fdec251aee3f81aaf5a02740806c167c0f3c929471df44",
      "powSolutions":{"pk":"02b3a06d6eaa8671431ba1db4dd427a77f75a5c2acbd71bfb725d38adc2b55f669","n":"5939ecfee6b0d7f4"},
      "parentId":"6481752bace5fa5acba5d5ef7124d48826664742d46c974c98a2d60ace229a34"
    }"#;

    fn id(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn tracker_registers(
        root: [u8; 33],
        r4_hex: &str,
        avl_shape: [u8; 3],
    ) -> NonMandatoryRegisters {
        let r4 = hex::decode(r4_hex).unwrap();
        let mut r5 = vec![0x64];
        r5.extend_from_slice(&root);
        r5.extend_from_slice(&avl_shape);
        NonMandatoryRegisters::try_from(vec![
            RegisterValue::sigma_parse_bytes(&r4),
            RegisterValue::sigma_parse_bytes(&r5),
        ])
        .unwrap()
    }

    fn nft() -> (String, BoxTokens) {
        let token_id = id(0xaa);
        let token = Token {
            token_id: token_id.parse::<TokenId>().unwrap(),
            amount: TokenAmount::try_from(1u64).unwrap(),
        };
        (token_id, vec![token].try_into().unwrap())
    }

    fn tracker_transaction(
        successor_r4: &str,
        successor_avl_shape: [u8; 3],
        leading_output: bool,
        duplicate_successor: bool,
    ) -> (Vec<u8>, Vec<u8>) {
        let (_, tokens) = nft();
        tracker_transaction_with_tokens(
            tokens,
            successor_r4,
            successor_avl_shape,
            leading_output,
            duplicate_successor,
        )
    }

    fn tracker_transaction_with_tokens(
        tokens: BoxTokens,
        successor_r4: &str,
        successor_avl_shape: [u8; 3],
        leading_output: bool,
        duplicate_successor: bool,
    ) -> (Vec<u8>, Vec<u8>) {
        tracker_transaction_with_tokens_and_value(
            tokens,
            None,
            successor_r4,
            successor_avl_shape,
            leading_output,
            duplicate_successor,
        )
    }

    fn tracker_transaction_with_tokens_and_value(
        tokens: BoxTokens,
        successor_value: Option<u64>,
        successor_r4: &str,
        successor_avl_shape: [u8; 3],
        leading_output: bool,
        duplicate_successor: bool,
    ) -> (Vec<u8>, Vec<u8>) {
        const TRACKER_R4: &str =
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7";
        let tree = ErgoTree::sigma_parse_bytes(
            &hex::decode(
                "0008cd02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            )
            .unwrap(),
        )
        .unwrap();
        let predecessor = ErgoBox::new(
            BoxValue::try_from(10_000_000u64).unwrap(),
            tree.clone(),
            Some(tokens.clone()),
            tracker_registers([0x11; 33], TRACKER_R4, [0x03, 0x20, 0x00]),
            100,
            TxId::zero(),
            0,
        )
        .unwrap();
        let successor = ErgoBoxCandidate {
            value: BoxValue::try_from(successor_value.unwrap_or(*predecessor.value.as_u64()))
                .unwrap(),
            ergo_tree: tree.clone(),
            tokens: Some(tokens),
            additional_registers: tracker_registers([0x44; 33], successor_r4, successor_avl_shape),
            creation_height: 101,
        };
        let fee_output = ErgoBoxCandidate {
            value: BoxValue::try_from(1_000_000u64).unwrap(),
            ergo_tree: tree,
            tokens: None,
            additional_registers: NonMandatoryRegisters::empty(),
            creation_height: 101,
        };
        let mut outputs = Vec::new();
        if leading_output {
            outputs.push(fee_output);
        }
        outputs.push(successor.clone());
        if duplicate_successor {
            outputs.push(successor);
        }
        let input = Input::from_unsigned_input(
            UnsignedInput::new(predecessor.box_id(), ContextExtension::empty()),
            ProofBytes::Some(vec![1, 2, 3].into()),
        );
        let transaction = Transaction::new_from_vec(vec![input], vec![], outputs).unwrap();
        let signed = serde_json::to_vec(&transaction).unwrap();
        let predecessor_json = serde_json::to_vec(&predecessor).unwrap();
        (signed, predecessor_json)
    }

    fn tracker_fixture() -> (ReconciliationIntent, Vec<u8>, Vec<u8>) {
        let (signed, predecessor_json) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            false,
            false,
        );
        let intent = ReconciliationIntent::tracker_publication(
            signed.clone(),
            predecessor_json.clone(),
            [0x44; 33],
        )
        .unwrap();
        (intent, signed, predecessor_json)
    }

    fn recompute_header_id(header: &mut Header) {
        header.id = derived_header_id(header).unwrap().parse().unwrap();
    }

    fn chain_with_first_root(
        start_height: u64,
        depth: u64,
        fork: u8,
        first_root: Option<Digest32>,
    ) -> ActiveChainProof {
        let template: Header = serde_json::from_str(HEADER_JSON).unwrap();
        let mut headers = Vec::new();
        let mut parent = template.parent_id;
        for offset in 0..=depth {
            let mut header = template.clone();
            header.height = (start_height + offset) as u32;
            header.parent_id = parent;
            header.timestamp += offset + fork as u64;
            header.autolykos_solution.nonce[0] ^= fork.wrapping_add(offset as u8);
            if offset == 0 {
                if let Some(root) = first_root {
                    header.transaction_root = root;
                }
            }
            recompute_header_id(&mut header);
            parent = header.id;
            headers.push(header);
        }
        let json_headers = headers
            .iter()
            .map(|header| {
                let mut value = serde_json::to_value(header).unwrap();
                if header.version > 1 {
                    if let Some(solution) = value
                        .get_mut("powSolutions")
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        solution.remove("w");
                        solution.remove("d");
                    }
                }
                value
            })
            .collect::<Vec<_>>();
        let json = serde_json::to_vec(&json_headers).unwrap();
        let tip = headers.last().unwrap();
        ActiveChainProof::from_node_responses(
            tip.id.to_string(),
            tip.height as u64,
            tip.id.to_string(),
            tip.height as u64,
            start_height,
            &json,
            1_000,
        )
        .unwrap()
    }

    fn chain(start_height: u64, depth: u64, fork: u8) -> ActiveChainProof {
        chain_with_first_root(start_height, depth, fork, None)
    }

    fn chain_for_transaction(
        start_height: u64,
        depth: u64,
        fork: u8,
        signed: &[u8],
    ) -> ActiveChainProof {
        let transaction = parse_transaction(signed).unwrap();
        let root = transaction_merkle_root(&[transaction], 2).unwrap();
        chain_with_first_root(start_height, depth, fork, Some(root))
    }

    fn bounded_chain_with_first_root(
        start_height: u64,
        selected_depth: u64,
        observed_tip_depth: u64,
        fork: u8,
        first_root: Option<Digest32>,
    ) -> ActiveChainProof {
        assert!(observed_tip_depth >= selected_depth);
        let selected = chain_with_first_root(start_height, selected_depth, fork, first_root);
        let json = serde_json::to_vec(
            &selected
                .headers
                .iter()
                .map(|header| serde_json::from_slice::<serde_json::Value>(&header.bytes).unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let observed_tip_id = if observed_tip_depth == selected_depth {
            selected.headers.last().unwrap().id.clone()
        } else {
            id(0xf0u8.wrapping_add(fork))
        };
        ActiveChainProof::from_bounded_node_responses(
            observed_tip_id.clone(),
            start_height + observed_tip_depth,
            observed_tip_id,
            start_height + observed_tip_depth,
            start_height,
            start_height + selected_depth,
            &json,
            1_000,
        )
        .unwrap()
    }

    fn bounded_chain_for_transaction(
        start_height: u64,
        selected_depth: u64,
        observed_tip_depth: u64,
        fork: u8,
        signed: &[u8],
    ) -> ActiveChainProof {
        let transaction = parse_transaction(signed).unwrap();
        let root = transaction_merkle_root(&[transaction], 2).unwrap();
        bounded_chain_with_first_root(
            start_height,
            selected_depth,
            observed_tip_depth,
            fork,
            Some(root),
        )
    }

    fn full_block_json(chain: &ActiveChainProof, transactions: &[Vec<u8>]) -> Vec<u8> {
        let header: serde_json::Value = serde_json::from_slice(&chain.headers[0].bytes).unwrap();
        let transactions = transactions
            .iter()
            .map(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).unwrap())
            .collect::<Vec<_>>();
        serde_json::to_vec(&serde_json::json!({
            "header": header,
            "blockTransactions": { "transactions": transactions }
        }))
        .unwrap()
    }

    fn evidence(
        signed: Vec<u8>,
        predecessor: Vec<u8>,
        chain: ActiveChainProof,
    ) -> TransactionChainEvidence {
        TransactionChainEvidence::from_node_snapshot(
            signed.clone(),
            full_block_json(&chain, &[signed]),
            chain.first_block_id(),
            chain.inclusion_height(),
            vec![predecessor],
            chain.clone(),
        )
        .unwrap()
    }

    fn policy() -> ReconciliationPolicy {
        ReconciliationPolicy::new(6, 100, 12)
    }

    fn journal_binding() -> ReconciliationJournalBinding {
        ReconciliationJournalBinding::tracker_v1([0xaa; 32])
    }

    fn open_test_journal(path: &Path) -> ReconciliationJournal {
        ReconciliationJournal::open(path, journal_binding(), JournalBootstrap::FreshAllowed)
            .unwrap()
    }

    #[test]
    fn signed_transaction_derives_exact_successor_and_full_register_manifest() {
        let (signed, predecessor) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        let intent =
            ReconciliationIntent::tracker_publication(signed, predecessor.clone(), [0x44; 33])
                .unwrap();
        let predecessor_json: serde_json::Value = serde_json::from_slice(&predecessor).unwrap();
        assert!(predecessor_json.get("boxId").is_some());
        assert_eq!(intent.successor_output_index, 1);
        assert_eq!(intent.successor().index(), 1);
        assert!(intent.successor().registers()[0].is_some());
        assert!(intent.successor().registers()[1].is_some());
        assert_eq!(
            intent.successor().registers()[2..],
            [None, None, None, None]
        );
        let durable_json = serde_json::to_value(&intent).unwrap();
        assert!(durable_json.get("generation").is_none());

        let mut forged = intent.clone();
        forged.successor.value -= 1;
        forged.intent_id = forged.compute_intent_id().unwrap();
        assert_eq!(
            forged.validate(),
            Err(ReconciliationError::MalformedBox(
                "stored box fields differ from canonical bytes".to_string()
            ))
        );
    }

    #[test]
    fn tracker_receiver_avl_root_and_unique_successor_fail_independently() {
        let wrong_receiver = tracker_transaction(
            "070279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            [0x03, 0x20, 0x00],
            false,
            false,
        );
        assert_eq!(
            ReconciliationIntent::tracker_publication(
                wrong_receiver.0,
                wrong_receiver.1,
                [0x44; 33],
            ),
            Err(ReconciliationError::SuccessorMismatch)
        );

        let wrong_avl_shape = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x01, 0x20, 0x00],
            false,
            false,
        );
        assert_eq!(
            ReconciliationIntent::tracker_publication(
                wrong_avl_shape.0,
                wrong_avl_shape.1,
                [0x44; 33],
            ),
            Err(ReconciliationError::RootMismatch)
        );

        let duplicate = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            false,
            true,
        );
        assert_eq!(
            ReconciliationIntent::tracker_publication(duplicate.0, duplicate.1, [0x44; 33]),
            Err(ReconciliationError::SuccessorMismatch)
        );

        let (_, signed, predecessor) = tracker_fixture();
        assert_eq!(
            ReconciliationIntent::tracker_publication(signed, predecessor, [0x45; 33]),
            Err(ReconciliationError::RootMismatch)
        );
    }

    #[test]
    fn configured_tracker_nft_cannot_be_reinterpreted_at_a_different_token_index() {
        let (tracker_nft_id, _) = nft();
        let tracker_token = Token {
            token_id: tracker_nft_id.parse::<TokenId>().unwrap(),
            amount: TokenAmount::try_from(1u64).unwrap(),
        };
        let leading_token = Token {
            token_id: id(0xbb).parse::<TokenId>().unwrap(),
            amount: TokenAmount::try_from(1u64).unwrap(),
        };
        let tokens: BoxTokens = vec![leading_token, tracker_token].try_into().unwrap();
        let (signed, predecessor) = tracker_transaction_with_tokens(
            tokens,
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            false,
            false,
        );
        let predecessor_box = ProtocolBox::from_json_bytes(&predecessor).unwrap();
        assert_eq!(predecessor_box.singleton_index(&tracker_nft_id).unwrap(), 1);

        // The intent derives index zero from signed bytes, but the journal is
        // bound independently to the configured tracker NFT and rejects this
        // attempted reinterpretation before any submission can be armed.
        let intent =
            ReconciliationIntent::tracker_publication(signed, predecessor, [0x44; 33]).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let journal = ReconciliationJournal::open(
            temp.path(),
            ReconciliationJournalBinding::tracker_v1(
                hex::decode(tracker_nft_id).unwrap().try_into().unwrap(),
            ),
            JournalBootstrap::FreshAllowed,
        )
        .unwrap();
        assert_eq!(
            journal.record_prepared(intent),
            Err(ReconciliationError::JournalBindingMismatch)
        );
    }

    #[test]
    fn signed_successor_value_must_equal_the_exact_predecessor() {
        let (_, tokens) = nft();
        let (signed, predecessor) = tracker_transaction_with_tokens_and_value(
            tokens,
            Some(9_000_000),
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            false,
            false,
        );
        assert_eq!(
            ReconciliationIntent::tracker_publication(signed, predecessor, [0x44; 33]),
            Err(ReconciliationError::SuccessorMismatch)
        );
    }

    #[test]
    fn transaction_root_matches_frozen_reference_and_rejects_old_mutants() {
        let (_, signed, _) = tracker_fixture();
        let transaction = parse_transaction(&signed).unwrap();
        let root_v2 = transaction_merkle_root(&[transaction.clone()], 2).unwrap();
        // Frozen from sigma-rust `ergo-chain-generation::transactions_root`
        // for this exact transaction fixture.
        assert_eq!(
            hex::encode(root_v2.as_ref()),
            "7950d92caa1b621ed4deed01f326883792f9e6505e99513daf737cc42431fa78"
        );

        let unsigned_id = blake2b256_hash(&transaction.bytes_to_sign().unwrap());
        assert_eq!(transaction.id().as_ref(), unsigned_id.as_slice());
        let root_v1 = transaction_merkle_root(&[transaction.clone()], 1).unwrap();
        assert_eq!(
            root_v1,
            MerkleTree::new(vec![MerkleNode::from_bytes(unsigned_id.to_vec())]).root_hash_special()
        );

        let witness = transaction
            .inputs
            .iter()
            .flat_map(|input| input.spending_proof.proof.as_ref().iter().copied())
            .collect::<Vec<_>>();
        let separated = MerkleTree::new(vec![
            MerkleNode::from_bytes(unsigned_id.to_vec()),
            MerkleNode::from_bytes(blake2b256_hash(&witness)[1..].to_vec()),
        ])
        .root_hash_special();
        let mut hashed_leaf = unsigned_id.to_vec();
        hashed_leaf.extend_from_slice(&blake2b256_hash(&witness)[1..]);
        let hashed = MerkleTree::new(vec![MerkleNode::from_bytes(hashed_leaf)]).root_hash_special();
        let mut truncated_leaf = unsigned_id.to_vec();
        truncated_leaf.extend_from_slice(&witness[1..]);
        let truncated =
            MerkleTree::new(vec![MerkleNode::from_bytes(truncated_leaf)]).root_hash_special();
        assert_ne!(root_v2, separated);
        assert_ne!(root_v2, hashed);
        assert_ne!(root_v2, truncated);
    }

    #[test]
    fn wrong_block_ancestor_tip_and_depth_fail_independently() {
        let (_, signed, predecessor) = tracker_fixture();
        let good = chain_for_transaction(100, 6, 0, &signed);
        assert_eq!(
            TransactionChainEvidence::from_node_snapshot(
                signed.clone(),
                full_block_json(&good, std::slice::from_ref(&signed)),
                id(9),
                100,
                vec![predecessor.clone()],
                good.clone(),
            ),
            Err(ReconciliationError::InactiveBlock)
        );

        let mut broken = good.clone();
        let mut broken_header: Header = serde_json::from_slice(&broken.headers[1].bytes).unwrap();
        broken_header.parent_id = id(8).parse().unwrap();
        recompute_header_id(&mut broken_header);
        broken.headers[1] = CanonicalHeader::from_header(&broken_header).unwrap();
        assert_eq!(
            broken.validate(),
            Err(ReconciliationError::AncestryMismatch)
        );

        let headers_json = serde_json::to_vec(
            &good
                .headers
                .iter()
                .map(|header| serde_json::from_slice::<serde_json::Value>(&header.bytes).unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            ActiveChainProof::from_node_responses(
                good.tip_id(),
                good.tip_height(),
                id(7),
                good.tip_height(),
                100,
                &headers_json,
                1_000,
            ),
            Err(ReconciliationError::IncoherentSnapshot)
        );

        let shallow = chain_for_transaction(100, 5, 0, &signed);
        let (intent, _, _) = tracker_fixture();
        assert_eq!(
            validate_chain_effect(
                &intent,
                &evidence(signed, predecessor, shallow),
                policy(),
                1_000,
            ),
            Err(ReconciliationError::DepthTooShallow {
                observed: 5,
                required: 6,
            })
        );
    }

    #[test]
    fn transaction_and_predecessor_fields_cannot_be_fabricated_behind_real_ids() {
        let (intent, signed, predecessor) = tracker_fixture();
        let selected = chain_for_transaction(100, 6, 0, &signed);
        let mut predecessor_json: serde_json::Value = serde_json::from_slice(&predecessor).unwrap();
        predecessor_json["value"] = serde_json::json!(9_000_000u64);
        assert!(matches!(
            TransactionChainEvidence::from_node_snapshot(
                signed.clone(),
                full_block_json(&selected, std::slice::from_ref(&signed)),
                selected.first_block_id(),
                100,
                vec![serde_json::to_vec(&predecessor_json).unwrap()],
                selected.clone(),
            ),
            Err(ReconciliationError::MalformedBox(_))
        ));

        let mut tx_json: serde_json::Value = serde_json::from_slice(&signed).unwrap();
        tx_json["outputs"][0]["additionalRegisters"]["R5"] =
            serde_json::Value::String("0e0100".to_string());
        let selected_block_id = selected.first_block_id().to_string();
        assert!(matches!(
            TransactionChainEvidence::from_node_snapshot(
                serde_json::to_vec(&tx_json).unwrap(),
                full_block_json(&selected, std::slice::from_ref(&signed)),
                selected_block_id,
                100,
                vec![predecessor],
                selected,
            ),
            Err(ReconciliationError::MalformedIntent(_))
        ));
        assert!(intent.validate().is_ok());
    }

    #[test]
    fn full_block_inclusion_is_bound_to_header_chain_and_transaction_root() {
        let (_, signed, predecessor) = tracker_fixture();
        let selected = chain_for_transaction(100, 6, 0, &signed);
        assert!(TransactionChainEvidence::from_node_snapshot(
            signed.clone(),
            full_block_json(&selected, std::slice::from_ref(&signed)),
            selected.first_block_id(),
            100,
            vec![predecessor.clone()],
            selected.clone(),
        )
        .is_ok());

        let (unrelated_signed, _) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        assert_eq!(
            TransactionChainEvidence::from_node_snapshot(
                signed.clone(),
                full_block_json(&selected, &[unrelated_signed]),
                selected.first_block_id(),
                100,
                vec![predecessor.clone()],
                selected.clone(),
            ),
            Err(ReconciliationError::TransactionNotInBlock)
        );

        let unrelated_block = chain_for_transaction(100, 6, 9, &signed);
        assert_eq!(
            TransactionChainEvidence::from_node_snapshot(
                signed.clone(),
                full_block_json(&unrelated_block, std::slice::from_ref(&signed)),
                selected.first_block_id(),
                100,
                vec![predecessor.clone()],
                selected.clone(),
            ),
            Err(ReconciliationError::InactiveBlock)
        );

        let wrong_root: Digest32 = id(0xdd).parse().unwrap();
        let wrong_root_chain = chain_with_first_root(100, 6, 4, Some(wrong_root));
        let wrong_root_block_id = wrong_root_chain.first_block_id().to_string();
        assert_eq!(
            TransactionChainEvidence::from_node_snapshot(
                signed.clone(),
                full_block_json(&wrong_root_chain, std::slice::from_ref(&signed)),
                wrong_root_block_id,
                100,
                vec![predecessor.clone()],
                wrong_root_chain,
            ),
            Err(ReconciliationError::TransactionRootMismatch)
        );

        let mut forged_block: serde_json::Value =
            serde_json::from_slice(&full_block_json(&selected, std::slice::from_ref(&signed)))
                .unwrap();
        forged_block["header"]["transactionsRoot"] = serde_json::json!(id(0xee));
        let selected_block_id = selected.first_block_id().to_string();
        assert_eq!(
            TransactionChainEvidence::from_node_snapshot(
                signed,
                serde_json::to_vec(&forged_block).unwrap(),
                selected_block_id,
                100,
                vec![predecessor],
                selected,
            ),
            Err(ReconciliationError::HeaderIdMismatch)
        );
    }

    #[test]
    fn rollback_uses_fresh_coherent_replacement_path() {
        let (intent, signed, predecessor) = tracker_fixture();
        let original_chain = chain_for_transaction(100, 6, 0, &signed);
        let effect = validate_chain_effect(
            &intent,
            &evidence(signed, predecessor, original_chain.clone()),
            policy(),
            1_000,
        )
        .unwrap();
        assert_eq!(
            validate_rollback(&effect, &original_chain, policy(), 1_000),
            Err(ReconciliationError::RollbackNotProven)
        );
        let replacement = chain(100, 7, 9);
        let rollback = validate_rollback(&effect, &replacement, policy(), 1_000).unwrap();
        assert_eq!(rollback.removed_block_id(), effect.block_id());
        assert!(serde_json::from_slice::<ValidatedChainEffect>(
            &serde_json::to_vec(&effect).unwrap()
        )
        .is_err());
        assert!(serde_json::from_slice::<ValidatedRollback>(
            &serde_json::to_vec(&rollback).unwrap()
        )
        .is_err());

        let mut forged = replacement;
        forged.headers[1].parent_id = id(3);
        assert!(validate_rollback(&effect, &forged, policy(), 1_000).is_err());
    }

    #[test]
    fn bounded_reorg_horizon_retires_or_rolls_back_one_fault_at_a_time() {
        let (intent, signed, predecessor) = tracker_fixture();
        let effect = validate_chain_effect(
            &intent,
            &evidence(
                signed.clone(),
                predecessor,
                chain_for_transaction(100, 6, 0, &signed),
            ),
            policy(),
            1_000,
        )
        .unwrap();

        // An arbitrarily old anchor needs only the configured 13-header
        // inclusive window, not inclusion..tip history.
        let old_but_active = bounded_chain_for_transaction(
            100,
            policy().reorg_monitor_depth(),
            MAX_CHAIN_HEADERS as u64 + 100,
            0,
            &signed,
        );
        let retirement =
            match validate_reorg_horizon(&effect, &old_but_active, policy(), 1_000).unwrap() {
                ReorgHorizonDecision::Retire(retirement) => retirement,
                ReorgHorizonDecision::Rollback(_) => panic!("unchanged anchor must retire"),
            };
        assert!(serde_json::from_slice::<ValidatedRetirement>(
            &serde_json::to_vec(&retirement).unwrap()
        )
        .is_err());

        let replacement = bounded_chain_with_first_root(
            100,
            policy().reorg_monitor_depth(),
            MAX_CHAIN_HEADERS as u64 + 100,
            9,
            None,
        );
        assert!(matches!(
            validate_reorg_horizon(&effect, &replacement, policy(), 1_000).unwrap(),
            ReorgHorizonDecision::Rollback(rollback)
                if rollback.removed_block_id() == effect.block_id()
        ));

        let too_short = bounded_chain_for_transaction(
            100,
            policy().reorg_monitor_depth() - 1,
            MAX_CHAIN_HEADERS as u64 + 100,
            0,
            &signed,
        );
        assert_eq!(
            validate_reorg_horizon(&effect, &too_short, policy(), 1_000),
            Err(ReconciliationError::IncompleteAncestry)
        );
        let too_long = bounded_chain_for_transaction(
            100,
            policy().reorg_monitor_depth() + 1,
            MAX_CHAIN_HEADERS as u64 + 100,
            0,
            &signed,
        );
        assert_eq!(
            validate_reorg_horizon(&effect, &too_long, policy(), 1_000),
            Err(ReconciliationError::IncompleteAncestry)
        );

        let mut truncated_json = serde_json::to_value(
            old_but_active
                .headers
                .iter()
                .map(|header| serde_json::from_slice::<serde_json::Value>(&header.bytes).unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        truncated_json.as_array_mut().unwrap().pop();
        assert_eq!(
            ActiveChainProof::from_bounded_node_responses(
                old_but_active.tip_id(),
                old_but_active.tip_height(),
                old_but_active.tip_id(),
                old_but_active.tip_height(),
                100,
                100 + policy().reorg_monitor_depth(),
                &serde_json::to_vec(&truncated_json).unwrap(),
                1_000,
            ),
            Err(ReconciliationError::DepthMismatch)
        );

        let invalid = ReconciliationPolicy::new(6, 100, MAX_REORG_MONITOR_DEPTH + 1);
        assert_eq!(
            validate_reorg_horizon(&effect, &old_but_active, invalid, 1_000),
            Err(ReconciliationError::InvalidPolicy)
        );
    }

    #[test]
    fn retired_anchor_survives_restart_and_does_not_block_newer_ticket() {
        let temp = tempfile::tempdir().unwrap();
        let (intent_a, signed_a, predecessor_a) = tracker_fixture();
        let effect_a = validate_chain_effect(
            &intent_a,
            &evidence(
                signed_a.clone(),
                predecessor_a,
                chain_for_transaction(100, 6, 0, &signed_a),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        let selected = bounded_chain_for_transaction(
            100,
            policy().reorg_monitor_depth(),
            MAX_CHAIN_HEADERS as u64 + 100,
            0,
            &signed_a,
        );
        let retirement =
            match validate_reorg_horizon(&effect_a, &selected, policy(), 1_000).unwrap() {
                ReorgHorizonDecision::Retire(retirement) => retirement,
                ReorgHorizonDecision::Rollback(_) => panic!("same anchor"),
            };

        {
            let journal = open_test_journal(temp.path());
            journal.record_prepared(intent_a).unwrap();
            journal.arm_submission(effect_a.intent_id()).unwrap();
            journal.record_validated_effect(effect_a.clone()).unwrap();
            journal.mark_applied(&effect_a).unwrap();
            journal.retire_accepted(&retirement).unwrap();
        }
        let journal = ReconciliationJournal::open(
            temp.path(),
            journal_binding(),
            JournalBootstrap::ExistingRequired,
        )
        .unwrap();
        assert!(matches!(
            journal.recovery_action().unwrap(),
            RecoveryAction::RestoreRetired(found) if found == effect_a
        ));
        assert!(journal.accepted_effect().unwrap().is_none());

        let (signed_b, predecessor_b) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        let intent_b =
            ReconciliationIntent::tracker_publication(signed_b, predecessor_b, [0x44; 33]).unwrap();
        journal.record_prepared(intent_b.clone()).unwrap();
        journal.arm_submission(intent_b.intent_id()).unwrap();
        assert!(matches!(
            journal.recovery_action().unwrap(),
            RecoveryAction::QueryExactTransaction(found) if found == intent_b
        ));
    }

    #[test]
    fn journal_manifest_requires_explicit_fresh_approval_and_never_rebinds() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("missing");
        assert!(matches!(
            ReconciliationJournal::open(
                &missing,
                journal_binding(),
                JournalBootstrap::ExistingRequired,
            ),
            Err(ReconciliationError::JournalBindingRequired)
        ));
        assert!(!missing.exists());

        let journal_path = parent.path().join("journal");
        {
            let _journal = ReconciliationJournal::open(
                &journal_path,
                journal_binding(),
                JournalBootstrap::FreshAllowed,
            )
            .unwrap();
        }
        let manifest_path = journal_path.join(JOURNAL_MANIFEST_FILE);
        let before = std::fs::read(&manifest_path).unwrap();
        let wrong = ReconciliationJournalBinding::tracker_v1([0xbb; 32]);
        assert!(matches!(
            ReconciliationJournal::open(&journal_path, wrong, JournalBootstrap::ExistingRequired,),
            Err(ReconciliationError::JournalBindingMismatch)
        ));
        assert_eq!(std::fs::read(&manifest_path).unwrap(), before);
        assert!(ReconciliationJournal::open(
            &journal_path,
            journal_binding(),
            JournalBootstrap::ExistingRequired,
        )
        .is_ok());

        let orphan = parent.path().join("orphan");
        std::fs::create_dir(&orphan).unwrap();
        let sentinel = orphan.join("old-state");
        std::fs::write(&sentinel, b"do-not-rewrite").unwrap();
        assert!(matches!(
            ReconciliationJournal::open(&orphan, journal_binding(), JournalBootstrap::FreshAllowed,),
            Err(ReconciliationError::JournalBindingRequired)
        ));
        assert_eq!(std::fs::read(sentinel).unwrap(), b"do-not-rewrite");
        assert!(!orphan.join(JOURNAL_MANIFEST_FILE).exists());
    }

    #[test]
    fn same_nft_journal_with_a_different_anchor_cannot_join_accounting_history() {
        let (intent_a, signed_a, predecessor_a) = tracker_fixture();
        let effect_a = validate_chain_effect(
            &intent_a,
            &evidence(
                signed_a.clone(),
                predecessor_a,
                chain_for_transaction(100, 6, 0, &signed_a),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        let (signed_b, predecessor_b) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        let intent_b = ReconciliationIntent::tracker_publication(
            signed_b.clone(),
            predecessor_b.clone(),
            [0x44; 33],
        )
        .unwrap();
        let effect_b = validate_chain_effect(
            &intent_b,
            &evidence(
                signed_b.clone(),
                predecessor_b,
                chain_for_transaction(200, 6, 4, &signed_b),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let journal = open_test_journal(temp.path());
        journal.record_prepared(intent_b).unwrap();
        journal.arm_submission(effect_b.intent_id()).unwrap();
        journal.record_validated_effect(effect_b.clone()).unwrap();
        journal.mark_applied(&effect_b).unwrap();

        let history_a = crate::ConfirmedProjectionAnchor::from_parts(
            effect_a.tx_id().to_string(),
            effect_a.successor_box_id().to_string(),
            effect_a.block_id().to_string(),
            effect_a.inclusion_height(),
            effect_a.successor_depth(),
            effect_a.intent_id().to_string(),
            effect_a.tracker_root().unwrap(),
        );
        let candidates = journal.accounting_effect_candidates().unwrap();
        assert_eq!(candidates, vec![effect_b]);
        assert!(!candidates
            .iter()
            .any(|candidate| history_a.matches_validated_effect(candidate)));
        assert_eq!(
            journal.validate_tracker_startup_join(None, Some(&history_a)),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
    }

    fn projection_anchor(effect: &ValidatedChainEffect) -> ConfirmedProjectionAnchor {
        ConfirmedProjectionAnchor::from_parts(
            effect.tx_id().to_string(),
            effect.successor_box_id().to_string(),
            effect.block_id().to_string(),
            effect.inclusion_height(),
            effect.successor_depth(),
            effect.intent_id().to_string(),
            effect.tracker_root().unwrap(),
        )
    }

    #[test]
    fn startup_join_is_reciprocal_and_acceptance_crash_window_is_exact() {
        let (intent, signed, predecessor) = tracker_fixture();
        let effect = validate_chain_effect(
            &intent,
            &evidence(
                signed.clone(),
                predecessor,
                chain_for_transaction(100, 6, 0, &signed),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        let pending = (effect.tx_id().to_string(), effect.tracker_root().unwrap());
        let projection = projection_anchor(&effect);

        let acceptance_dir = tempfile::tempdir().unwrap();
        let acceptance = open_test_journal(acceptance_dir.path());
        acceptance.record_prepared(intent.clone()).unwrap();
        acceptance.arm_submission(intent.intent_id()).unwrap();
        acceptance.record_validated_effect(effect.clone()).unwrap();
        assert!(acceptance
            .validate_tracker_startup_join(Some(&pending), None)
            .is_ok());
        assert_eq!(
            acceptance.validate_tracker_startup_join(None, None),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
        assert!(acceptance
            .validate_tracker_startup_join(None, Some(&projection))
            .is_ok());
        assert_eq!(
            acceptance.validate_tracker_startup_join(Some(&("aa".repeat(32), pending.1)), None,),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
        assert_eq!(
            acceptance.validate_tracker_startup_join(Some(&(pending.0.clone(), [0x99; 33])), None,),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );

        acceptance.mark_applied(&effect).unwrap();
        assert_eq!(
            acceptance.validate_tracker_startup_join(None, None),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
        assert!(acceptance
            .validate_tracker_startup_join(None, Some(&projection))
            .is_ok());
    }

    #[test]
    fn rollback_startup_demotes_before_resuming_the_exact_newer_receipt() {
        let (intent_a, signed_a, predecessor_a) = tracker_fixture();
        let effect_a = validate_chain_effect(
            &intent_a,
            &evidence(
                signed_a.clone(),
                predecessor_a,
                chain_for_transaction(100, 6, 0, &signed_a),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        let (signed_b, predecessor_b) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        let intent_b =
            ReconciliationIntent::tracker_publication(signed_b, predecessor_b, [0x44; 33]).unwrap();
        let pending_b = (
            intent_b.tx_id().to_string(),
            intent_b.tracker_root().unwrap(),
        );
        let rollback = validated_rollback_for_test(&effect_a);
        let temp = tempfile::tempdir().unwrap();
        let journal = open_test_journal(temp.path());
        journal.record_prepared(intent_a).unwrap();
        journal.arm_submission(effect_a.intent_id()).unwrap();
        journal.record_validated_effect(effect_a.clone()).unwrap();
        journal.mark_applied(&effect_a).unwrap();
        journal.record_prepared(intent_b).unwrap();
        journal.record_rollback(rollback.clone()).unwrap();

        assert!(matches!(
            journal.recovery_action().unwrap(),
            RecoveryAction::ApplyRollback(found) if found == rollback
        ));
        assert!(journal
            .validate_tracker_startup_join(Some(&pending_b), None)
            .is_ok());
        assert_eq!(
            journal.validate_tracker_startup_join(None, None),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
        assert_eq!(
            journal.validate_tracker_startup_join(Some(&("aa".repeat(32), pending_b.1)), None,),
            Err(ReconciliationError::AccountingProjectionMismatch)
        );
    }

    #[test]
    fn crash_windows_restart_and_duplicate_tx_are_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let (intent, signed, predecessor) = tracker_fixture();
        {
            let journal = open_test_journal(temp.path());
            journal.record_prepared(intent.clone()).unwrap();
        }
        {
            let journal = open_test_journal(temp.path());
            assert!(matches!(
                journal.recovery_action().unwrap(),
                RecoveryAction::SubmitPrepared(found) if found == intent
            ));
            journal.arm_submission(intent.intent_id()).unwrap();
        }
        let effect = validate_chain_effect(
            &intent,
            &evidence(
                signed.clone(),
                predecessor,
                chain_for_transaction(100, 6, 0, &signed),
            ),
            policy(),
            1_000,
        )
        .unwrap();
        {
            let journal = open_test_journal(temp.path());
            assert!(matches!(
                journal.recovery_action().unwrap(),
                RecoveryAction::QueryExactTransaction(found) if found == intent
            ));
            journal.record_validated_effect(effect.clone()).unwrap();
        }
        {
            let journal = open_test_journal(temp.path());
            assert!(
                matches!(journal.recovery_action().unwrap(), RecoveryAction::ApplyAccepted(found) if found == effect)
            );
            journal.mark_applied(&effect).unwrap();
            journal.mark_applied(&effect).unwrap();
        }
        {
            let journal = open_test_journal(temp.path());
            assert!(
                matches!(journal.recovery_action().unwrap(), RecoveryAction::RevalidateAccepted(found) if found == effect)
            );
            assert_eq!(
                journal.record_prepared(intent),
                Err(ReconciliationError::DuplicateTransactionConflict)
            );
        }
    }

    #[test]
    fn validated_reorg_preempts_a_newer_pending_ticket_then_resumes_it() {
        let temp = tempfile::tempdir().unwrap();
        let (intent_a, signed_a, predecessor_a) = tracker_fixture();
        let original_chain = chain_for_transaction(100, 6, 0, &signed_a);
        let effect_a = validate_chain_effect(
            &intent_a,
            &evidence(signed_a, predecessor_a, original_chain),
            policy(),
            1_000,
        )
        .unwrap();
        let (signed_b, predecessor_b) = tracker_transaction(
            "0702dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7",
            [0x03, 0x20, 0x00],
            true,
            false,
        );
        let intent_b =
            ReconciliationIntent::tracker_publication(signed_b, predecessor_b, [0x44; 33]).unwrap();
        assert_ne!(intent_a.tx_id(), intent_b.tx_id());

        let journal = open_test_journal(temp.path());
        journal.record_prepared(intent_a.clone()).unwrap();
        journal.arm_submission(intent_a.intent_id()).unwrap();
        journal.record_validated_effect(effect_a.clone()).unwrap();
        journal.mark_applied(&effect_a).unwrap();
        journal.record_prepared(intent_b.clone()).unwrap();
        journal.arm_submission(intent_b.intent_id()).unwrap();

        let replacement = chain(100, 7, 9);
        let rollback = validate_rollback(&effect_a, &replacement, policy(), 1_000).unwrap();
        journal.record_rollback(rollback.clone()).unwrap();
        assert!(matches!(
            journal.recovery_action().unwrap(),
            RecoveryAction::ApplyRollback(found) if found == rollback
        ));
        journal.mark_rollback_applied(&rollback).unwrap();
        assert!(matches!(
            journal.recovery_action().unwrap(),
            RecoveryAction::QueryExactTransaction(found) if found == intent_b
        ));
    }
}
