//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides blockchain integration using /blockchain endpoints (no node scans).

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::Client;

/// Response from `POST /blockchain/box/unspent/byAddress` is a JSON array of IndexedErgoBox.
pub(crate) type ByAddressResponse = Vec<IndexedErgoBox>;

/// Box representation returned by Ergo `/blockchain/*` endpoints (IndexedErgoBox).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndexedErgoBox {
    #[serde(rename = "boxId")]
    box_id: String,
    value: u64,
    #[serde(rename = "ergoTree")]
    ergo_tree: String,
    #[serde(rename = "creationHeight")]
    creation_height: u64,
    #[serde(rename = "transactionId")]
    transaction_id: String,
    #[serde(rename = "additionalRegisters")]
    additional_registers: std::collections::HashMap<String, String>,
    #[serde(default)]
    assets: Vec<IndexedBoxAsset>,
    #[serde(flatten)]
    _extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Asset representation in an IndexedErgoBox.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndexedBoxAsset {
    #[serde(rename = "tokenId")]
    token_id: String,
    amount: u64,
}

impl From<IndexedErgoBox> for ScanBox {
    fn from(box_: IndexedErgoBox) -> Self {
        Self {
            box_id: box_.box_id,
            value: box_.value,
            ergo_tree: box_.ergo_tree,
            creation_height: box_.creation_height,
            transaction_id: box_.transaction_id,
            additional_registers: box_.additional_registers,
            assets: box_
                .assets
                .into_iter()
                .map(|a| BoxAsset {
                    token_id: a.token_id,
                    amount: a.amount,
                })
                .collect(),
        }
    }
}

use crate::{
    persistence::{QuarantinedReserve, ReserveStorage, ScannerMetadataStorage},
    ExtendedReserveInfo, ReserveTracker,
};

/// Normalize a box id the way `ExtendedReserveInfo.box_id` is stored:
/// hex-decode (when valid hex) and re-encode, so comparisons with node-provided
/// box ids are case- and encoding-insensitive.
fn normalized_box_id(box_id: &str) -> String {
    hex::encode(hex::decode(box_id).unwrap_or_else(|_| box_id.as_bytes().to_vec()))
}

/// The ergoTree of a pay-to-public-key (P2PK) box: `0008cd` followed by the
/// 33-byte compressed public key (see `basis_offchain::ergo_tx::pubkey_to_address`).
fn p2pk_ergo_tree(pubkey_hex: &str) -> String {
    format!("0008cd{}", pubkey_hex)
}

/// Maximum generations walked backwards when verifying that a new reserve box
/// descends from the tracked one (issue #19). Covers tracker downtime spanning
/// several reserve actions (top-up of a top-up, etc.).
const LINEAGE_WALK_DEPTH: usize = 8;

/// Admission decision for a parsed reserve box (issue #19).
///
/// Only one box per (owner, denomination) lineage is ever tracked: a
/// C4-validated root, or a verified successor of the currently tracked box.
/// Everything else is quarantined before it reaches the tracker, so an
/// attacker cannot poison `ReserveTracker::resolve_issuer_reserve` with a dust
/// duplicate carrying the victim's key in R4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReserveAdmission {
    /// The box may be tracked.
    Admitted,
    /// The box must not be tracked; `reason` is persisted on the quarantine
    /// record.
    Quarantined { reason: String },
    /// Admission could not be decided this cycle (e.g. a node error during the
    /// lineage check); retry next cycle. The box is neither admitted nor
    /// quarantined, and the tracked box for the same (owner, denomination)
    /// must not be removed as spent while a successor candidate is pending.
    Retry,
}

/// The outcome of ingesting one scanned reserve box (parse + admission +
/// tracking), used by both scan ingestion paths (issue #19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanBoxIngest {
    /// The box was admitted and (re-)stored; carries the normalized box id.
    Tracked(String),
    /// The box was rejected as unparseable or quarantined; carries the
    /// normalized box id. Rejected boxes never trigger removal of themselves
    /// and must not block removal of other genuinely-spent reserves.
    Rejected(String),
    /// The lineage check was inconclusive this cycle; carries the owner pubkey
    /// and denomination of the pending lineage so the caller keeps the
    /// currently tracked box until the check resolves.
    Pending(String, crate::DenomKey),
}

/// Pure admission verdict (issue #19), factored out for unit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionVerdict {
    Admit,
    Quarantine,
    Retry,
}

/// Decide whether a parsed box may be tracked.
///
/// - `tracked`: a reserve for the same (owner, denomination) is already tracked.
/// - `proof`: for a root, whether the creating transaction spends a P2PK input
///   of the R4 owner key; for a non-root, whether the box verifiably spends the
///   tracked box (possibly across missed generations). `None` when the node
///   could not be queried.
/// - `setup_ok`: the C4 setup checks (R7 == 0, empty R5 tree) pass.
fn decide_admission(tracked: bool, proof: Option<bool>, setup_ok: bool) -> AdmissionVerdict {
    if !tracked {
        // Root candidate: only C4-clean boxes with an owner-key P2PK input in
        // the creating transaction may open a lineage (issue #19).
        return match proof {
            Some(true) if setup_ok => AdmissionVerdict::Admit,
            Some(_) => AdmissionVerdict::Quarantine,
            None => AdmissionVerdict::Retry,
        };
    }
    match proof {
        Some(true) => AdmissionVerdict::Admit,
        Some(false) => AdmissionVerdict::Quarantine,
        None => AdmissionVerdict::Retry,
    }
}

/// Page size for `POST /blockchain/box/unspent/byAddress`. The node defaults to
/// 5 boxes (newest first) when no limit is given, which silently dropped older
/// reserves from tracking (issue: scanner only saw 5 reserve boxes per address).
///
/// Each page is read through `read_json_capped` (2 MiB). A box can serialize to
/// 4 KB and the node renders it as pretty-printed hex plus the P2S address, so
/// a page must stay small enough that a full page of maximum-size boxes fits:
/// with 1000 per page, a few hundred junk boxes at the reserve address stopped
/// every scan for good (follow-up review N4).
const RESERVE_BOX_PAGE_LIMIT: u64 = 100;

#[derive(Error, Debug)]
pub enum ScannerError {
    #[error("Scanner error: {0}")]
    Generic(String),
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Node error: {0}")]
    NodeError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Box error: {0}")]
    BoxError(String),
    #[error("Invalid transaction {0}")]
    InvalidTransaction(String),
    #[error("Reserve box validation failed at TX id: {0}")]
    InvalidReserveBox(String),
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("JSON parse error: {0}")]
    JsonError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Reserves,
}

impl ScanType {
    pub fn to_str(&self) -> &'static str {
        match self {
            ScanType::Reserves => "reserves",
        }
    }
}

/// Which reserve contract a scanned box was found at. Determines how collateral and
/// denomination are derived (specs/ergo/multi_collateral_reserves.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveContractKind {
    /// ERG-collateralized reserve (`contract/basis.es`)
    Erg,
    /// Token-collateralized reserve (`contract/basis-token.es`); the token id is
    /// read per box from `tokens(1)`
    Token,
}

/// A scanned reserve box tagged with the contract slot and address it was found at.
pub struct TaggedScanBox {
    pub scan_box: ScanBox,
    pub kind: ReserveContractKind,
    /// The reserve contract P2S address this box was found at (its contract address).
    pub contract_address: String,
}

/// Configuration for scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Basis reserve contract P2S address (optional)
    pub reserve_contract_p2s: Option<String>,
    /// Basis token reserve contract P2S address (optional). Reserves found at this
    /// address are token-backed; the denomination is derived per box from `tokens(1)`
    /// (specs/ergo/multi_collateral_reserves.md).
    pub token_reserve_contract_p2s: Option<String>,
    /// This tracker's NFT ID (hex-encoded, 32 bytes). When set, reserve boxes whose R6
    /// register does not reference this NFT are rejected (C4 validation).
    pub tracker_nft_id: Option<String>,
    /// Ergo node URL
    pub node_url: String,
    /// Scan registration name
    pub scan_name: Option<String>,
    /// API key for Ergo node authentication
    pub api_key: Option<String>,
}

/// Inner state for scanner that requires synchronization
#[derive(Clone)]
pub struct ServerStateInner {
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
}

/// Server state for scanner
/// Uses real blockchain integration with proper synchronization
#[derive(Clone)]
pub struct ServerState {
    pub config: NodeConfig,
    pub inner: Arc<Mutex<ServerStateInner>>,
    pub client: Client,
    pub reserve_tracker: ReserveTracker,
    pub metadata_storage: ScannerMetadataStorage,
    pub reserve_storage: ReserveStorage,
}

impl ServerState {
    /// Create HTTP request builder with API key header if configured
    fn request_builder(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        debug!("Request method: {}, URL: {}", method, url);

        let mut request = self.client.request(method, url);

        // Add API key header if configured (never log the key itself)
        if let Some(api_key) = &self.config.api_key {
            debug!("Using configured API key for request to: {}", url);
            request = request.header("api_key", api_key);
        } else {
            debug!("No API key configured for request to: {}", url);
        }

        request
    }

    /// Create a server state that uses real Ergo scanner
    pub fn new(config: NodeConfig, data_dir: impl AsRef<Path>) -> Result<Self, ScannerError> {
        let start_height = config.start_height.unwrap_or(0);
        let client = crate::http::bounded_client();
        let data_dir = data_dir.as_ref();

        // Log which Ergo node is being used (INFO level); never log the API key
        info!("Initializing Ergo scanner with node: {}", config.node_url);
        if config.api_key.is_some() {
            info!("Ergo node API key is configured");
        } else {
            warn!("No API key configured for Ergo node");
        }

        // Open scanner metadata storage - create directory if it doesn't exist
        let storage_path = data_dir.join("scanner_metadata");

        // Create directory if it doesn't exist
        if let Some(parent) = storage_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ScannerError::StoreError(format!(
                    "Failed to create scanner metadata directory: {}",
                    e
                ))
            })?;
        }

        let metadata_storage = ScannerMetadataStorage::open(&storage_path).map_err(|e| {
            ScannerError::StoreError(format!("Failed to open scanner metadata storage: {:?}", e))
        })?;

        // Open reserve storage - create directory if it doesn't exist
        let reserve_storage_path = data_dir.join("reserves");

        // Create directory if it doesn't exist
        if let Some(parent) = reserve_storage_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ScannerError::StoreError(format!(
                    "Failed to create reserve storage directory: {}",
                    e
                ))
            })?;
        }

        let reserve_storage = ReserveStorage::open(&reserve_storage_path).map_err(|e| {
            ScannerError::StoreError(format!("Failed to open reserve storage: {:?}", e))
        })?;

        // Create reserve tracker and load existing reserves from database
        let reserve_tracker = ReserveTracker::new();

        // Load existing reserves from database
        if let Ok(existing_reserves) = reserve_storage.get_all_reserves() {
            let reserves_count = existing_reserves.len();
            for reserve in existing_reserves {
                if let Err(e) = reserve_tracker.update_reserve(reserve) {
                    warn!("Failed to load reserve from database: {}", e);
                }
            }
            info!("Loaded {} reserves from database", reserves_count);
        }

        // Create synchronized inner state
        let inner = Arc::new(Mutex::new(ServerStateInner {
            current_height: 0,
            last_scanned_height: start_height,
            scan_active: false,
        }));

        Ok(Self {
            config,
            inner,
            client,
            reserve_tracker,
            metadata_storage,
            reserve_storage,
        })
    }

    /// Fetch current blockchain height directly from the node, bypassing cache.
    /// Used by the scanner loop so it detects new blocks immediately instead of
    /// reusing a potentially stale cached value for up to 10 minutes.
    pub async fn fetch_current_height(&self) -> Result<u64, ScannerError> {
        let url = format!("{}/info", self.config.node_url);
        info!("Fetching current blockchain height from: {}", url);

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to connect to node: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!(
                "Node returned status: {}",
                response.status()
            )));
        }

        let info: serde_json::Value = crate::http::read_json_capped(response)
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;

        let height = info["fullHeight"].as_u64().ok_or_else(|| {
            ScannerError::NodeError("Failed to parse fullHeight from node info".to_string())
        })?;

        // Keep the cache fresh for other callers that don't need the latest height.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if let Err(e) = self.metadata_storage.store_blockchain_height(height, now) {
            warn!("Failed to cache blockchain height: {:?}", e);
        }

        Ok(height)
    }

    /// Get current blockchain height from cache or Ergo node.
    /// Uses cached value if less than 10 minutes old, otherwise fetches from node.
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        const CACHE_TTL_MS: u64 = 600_000; // 10 minutes in milliseconds

        // Check if we have a cached height
        match self.metadata_storage.get_blockchain_height() {
            Ok(Some((cached_height, cached_timestamp))) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                if now.saturating_sub(cached_timestamp) < CACHE_TTL_MS {
                    debug!("Using cached blockchain height: {}", cached_height);
                    return Ok(cached_height);
                }

                debug!("Cached blockchain height expired, fetching from node");
            }
            Ok(None) => {
                debug!("No cached blockchain height found, fetching from node");
            }
            Err(e) => {
                warn!("Failed to read cached blockchain height: {:?}", e);
            }
        }

        // Fetch from node
        let url = format!("{}/info", self.config.node_url);

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to connect to node: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!(
                "Node returned status: {}",
                response.status()
            )));
        }

        let info: serde_json::Value = crate::http::read_json_capped(response)
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;

        let height = info["fullHeight"].as_u64().ok_or_else(|| {
            ScannerError::NodeError("Failed to parse fullHeight from node info".to_string())
        })?;

        // Store in cache with current timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Err(e) = self.metadata_storage.store_blockchain_height(height, now) {
            warn!("Failed to cache blockchain height: {:?}", e);
        }

        Ok(height)
    }

    /// Get unspent reserve boxes via `POST /blockchain/box/unspent/byAddress`,
    /// tagged with the contract slot and address they were found at.
    /// Queries both the ERG-backed and token-backed reserve contract addresses when
    /// configured, merging the results and removing duplicates.
    pub async fn get_unspent_reserve_boxes_tagged(
        &self,
    ) -> Result<Vec<TaggedScanBox>, ScannerError> {
        let mut slots = Vec::new();
        if let Some(ref p2s) = self.config.reserve_contract_p2s {
            if !p2s.is_empty() {
                slots.push((p2s.clone(), ReserveContractKind::Erg));
            }
        }
        if let Some(ref p2s) = self.config.token_reserve_contract_p2s {
            if !p2s.is_empty() {
                slots.push((p2s.clone(), ReserveContractKind::Token));
            }
        }

        if slots.is_empty() {
            warn!("No reserve contract P2S specified, scanner will have no boxes to fetch");
            return Ok(Vec::new());
        }

        let url = format!("{}/blockchain/box/unspent/byAddress", self.config.node_url);
        let mut all_boxes: Vec<TaggedScanBox> = Vec::new();

        for (address, kind) in slots {
            info!("Fetching unspent reserve boxes for contract: {}", address);

            // The node applies a default limit of 5 boxes (newest first) when no
            // `limit` is given, which silently drops older reserves. Page through
            // the full result set: keep fetching until a page comes back short.
            let mut offset: u64 = 0;
            let mut address_box_count = 0usize;
            loop {
                let response = self
                    .request_builder(reqwest::Method::POST, &url)
                    .query(&[
                        ("offset", offset.to_string()),
                        ("limit", RESERVE_BOX_PAGE_LIMIT.to_string()),
                    ])
                    .json(&address)
                    .send()
                    .await
                    .map_err(|e| {
                        ScannerError::HttpError(format!(
                            "Failed to fetch reserve boxes for {}: {}",
                            address, e
                        ))
                    })?;

                let status = response.status();
                if !status.is_success() {
                    let response_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unable to read response".to_string());
                    error!(
                        "Failed to get reserve boxes for {} with status {}: {}",
                        address, status, response_text
                    );
                    return Err(ScannerError::NodeError(format!(
                        "Failed to get reserve boxes for {} with status: {}",
                        address, status
                    )));
                }

                let parsed: ByAddressResponse = crate::http::read_json_capped(response)
                    .await
                    .map_err(|e| {
                        ScannerError::JsonError(format!(
                            "Failed to parse reserve boxes response for {}: {}",
                            address, e
                        ))
                    })
                    .and_then(|value| {
                        serde_json::from_value(value).map_err(|e| {
                            ScannerError::JsonError(format!(
                                "Failed to parse reserve boxes response for {}: {}",
                                address, e
                            ))
                        })
                    })?;

                let page_len = parsed.len();
                address_box_count += page_len;
                all_boxes.extend(parsed.into_iter().map(|b| TaggedScanBox {
                    scan_box: b.into(),
                    kind,
                    contract_address: address.clone(),
                }));

                if (page_len as u64) < RESERVE_BOX_PAGE_LIMIT {
                    break;
                }
                offset += RESERVE_BOX_PAGE_LIMIT;
            }

            info!("Found {} reserve boxes for {}", address_box_count, address);
        }

        // Deduplicate by box_id in case both contracts somehow return the same box.
        let mut seen = std::collections::HashSet::new();
        all_boxes.retain(|b| seen.insert(b.scan_box.box_id.clone()));

        info!("Total unique reserve boxes: {}", all_boxes.len());
        Ok(all_boxes)
    }

    /// Get unspent reserve boxes without contract-slot tags.
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        Ok(self
            .get_unspent_reserve_boxes_tagged()
            .await?
            .into_iter()
            .map(|t| t.scan_box)
            .collect())
    }

    /// Check if scanner is active
    pub async fn is_active(&self) -> bool {
        // Use async lock for async context
        let inner = self.inner.lock().await;
        inner.scan_active
    }

    /// Start scanning with real blockchain integration
    pub async fn start_scanning(&mut self) -> Result<(), ScannerError> {
        info!("Starting Ergo blockchain scanner for reserves");

        if self.config.reserve_contract_p2s.is_none()
            && self.config.token_reserve_contract_p2s.is_none()
        {
            warn!("No reserve contract P2S specified, scanner will have no boxes to fetch");
        }

        // Update inner state
        {
            let mut inner = self.inner.lock().await;
            inner.scan_active = true;
        }

        Ok(())
    }

    /// Get last scanned height
    pub async fn last_scanned_height(&self) -> u64 {
        let inner = self.inner.lock().await;
        inner.last_scanned_height
    }

    /// Get the reserve tracker
    pub fn reserve_tracker(&self) -> &ReserveTracker {
        &self.reserve_tracker
    }

    /// Get the reserve storage for direct database access
    pub fn reserve_storage(&self) -> &ReserveStorage {
        &self.reserve_storage
    }

    /// Get unspent boxes from the blockchain via the reserve contract address.
    pub async fn get_scan_boxes(&self) -> Result<Vec<TaggedScanBox>, ScannerError> {
        self.get_unspent_reserve_boxes_tagged().await
    }

    /// Parse reserve box into ExtendedReserveInfo.
    ///
    /// Collateral and denomination are derived **per box** from the contract slot the
    /// box was found at (specs/ergo/multi_collateral_reserves.md):
    /// - Erg slot: collateral is the box ERG value.
    /// - Token slot: the box must hold exactly two tokens (reserve NFT at #0, reserve
    ///   token at #1); collateral is the #1 amount and the denomination is its id.
    pub fn parse_reserve_box(
        &self,
        scan_box: &ScanBox,
        kind: ReserveContractKind,
        contract_address: &str,
    ) -> Result<ExtendedReserveInfo, ScannerError> {
        let box_id = scan_box.box_id.clone();
        let value = scan_box.value;
        let creation_height = scan_box.creation_height;

        // Extract owner public key from R4 register
        let owner_pubkey_raw = scan_box
            .additional_registers
            .get("R4")
            .ok_or_else(|| {
                ScannerError::InvalidReserveBox(format!("Missing R4 register in box {}", box_id))
            })?
            .clone();

        // Strip the 0x07 prefix if present (GroupElement type identifier from Ergo registers)
        let owner_pubkey = if owner_pubkey_raw.starts_with("07") && owner_pubkey_raw.len() >= 66 {
            // Extract the actual 33-byte public key (66 hex chars) after the 0x07 prefix
            owner_pubkey_raw[2..].to_string()
        } else {
            // Use as-is if no prefix or wrong length
            owner_pubkey_raw
        };

        // Extract tracker NFT ID from R6 register (required according to spec)
        let tracker_nft_id_raw = scan_box
            .additional_registers
            .get("R6")
            .ok_or_else(|| {
                ScannerError::InvalidReserveBox(format!("Missing R6 register in box {}", box_id))
            })?
            .clone();

        // Create extended reserve info
        // Decode the hex-encoded public key to actual bytes
        let owner_pubkey_bytes = hex::decode(&owner_pubkey).map_err(|_| {
            ScannerError::InvalidReserveBox(format!(
                "Invalid hex in owner pubkey for box {}",
                box_id
            ))
        })?;

        // C4: R4 must be a valid compressed secp256k1 point, otherwise the
        // reserve can never be redeemed against on-chain.
        let owner_pubkey_array: &[u8; 33] =
            owner_pubkey_bytes.as_slice().try_into().map_err(|_| {
                ScannerError::InvalidReserveBox(format!(
                    "Invalid owner pubkey length in box {}: expected 33 bytes, got {}",
                    box_id,
                    owner_pubkey_bytes.len()
                ))
            })?;
        basis_core::impls::validate_public_key(owner_pubkey_array).map_err(|_| {
            ScannerError::InvalidReserveBox(format!(
                "Invalid owner pubkey (not a valid curve point) in box {}",
                box_id
            ))
        })?;

        // Decode the hex-encoded tracker NFT ID to actual bytes
        // R6 contains a Coll[Byte] value with Ergo serialization prefix: 0e20 (type + length)
        // We need to strip the first 2 bytes (4 hex chars) to get the actual data
        let tracker_nft_hex = if tracker_nft_id_raw.len() >= 4 {
            &tracker_nft_id_raw[4..]
        } else {
            tracker_nft_id_raw.as_str()
        };
        let tracker_nft_id_bytes = hex::decode(tracker_nft_hex).map_err(|_| {
            ScannerError::InvalidReserveBox(format!(
                "Invalid hex in tracker NFT ID for box {}",
                box_id
            ))
        })?;

        // Validate that the tracker NFT ID is exactly 32 bytes (the actual tracker NFT ID)
        if tracker_nft_id_bytes.len() != 32 {
            return Err(ScannerError::InvalidReserveBox(format!(
                "Invalid tracker NFT ID length in box {}: expected 32 bytes, got {}",
                box_id,
                tracker_nft_id_bytes.len()
            )));
        }

        // C4: when this tracker's NFT ID is configured, R6 must reference it,
        // otherwise the reserve is bound to a different (or fake) tracker.
        if let Some(configured_nft) = self
            .config
            .tracker_nft_id
            .as_deref()
            .filter(|id| !id.is_empty())
        {
            if !tracker_nft_hex.eq_ignore_ascii_case(configured_nft) {
                return Err(ScannerError::InvalidReserveBox(format!(
                    "Tracker NFT ID mismatch in box {}: R6 references {}, this tracker is {}",
                    box_id, tracker_nft_hex, configured_nft
                )));
            }
        }

        // C4: R7 must be a serialized Long (type byte 0x05) when present. Any
        // other type (e.g. Int) makes SELF.R7[Long] throw on-chain, bricking
        // the box, while the scanner would silently read it as 0.
        if let Some(r7_raw) = scan_box.additional_registers.get("R7") {
            if !r7_raw.starts_with("05") {
                return Err(ScannerError::InvalidReserveBox(format!(
                    "R7 register in box {} is not a Long (expected 0x05 prefix): {}",
                    box_id, r7_raw
                )));
            }
        }

        // Extract refund initiation height from R7 register (optional; absent = 0)
        let refund_initiation_height =
            decode_ergo_long_register(scan_box.additional_registers.get("R7"));

        // Derive collateral and denomination per box from the contract slot.
        let (collateral_amount, reserve_token_id_bytes) = match kind {
            ReserveContractKind::Erg => (value, None),
            ReserveContractKind::Token => {
                // The token reserve contract pins positions: tokens(0) is the reserve
                // NFT (amount 1), tokens(1) is the collateral token. Anything else is
                // malformed and cannot be redeemed on-chain.
                if scan_box.assets.len() != 2 {
                    return Err(ScannerError::InvalidReserveBox(format!(
                        "Token reserve box {} must hold exactly 2 tokens (reserve NFT + collateral token), got {}",
                        box_id,
                        scan_box.assets.len()
                    )));
                }
                let collateral_asset = &scan_box.assets[1];
                let token_bytes = hex::decode(&collateral_asset.token_id).map_err(|_| {
                    ScannerError::InvalidReserveBox(format!(
                        "Invalid collateral token id hex in box {}",
                        box_id
                    ))
                })?;
                if token_bytes.len() != 32 {
                    return Err(ScannerError::InvalidReserveBox(format!(
                        "Collateral token id in box {} is not 32 bytes",
                        box_id
                    )));
                }
                (collateral_asset.amount, Some(token_bytes))
            }
        };

        // NOTE (issue #19): the C4 setup checks (R7 == 0, empty R5 tree) are NOT
        // done here. They depend on tracker state (they apply only to root boxes,
        // never to legitimate top-up/redemption successors), so they live in
        // `admit_reserve_box`. This function is purely per-box structural
        // validation.

        let box_id_bytes = hex::decode(&box_id).unwrap_or_else(|_| box_id.as_bytes().to_vec());
        let mut reserve_info = ExtendedReserveInfo::new(
            &box_id_bytes,
            &owner_pubkey_bytes,
            collateral_amount,
            Some(&tracker_nft_id_bytes),
            creation_height,
            refund_initiation_height,
            reserve_token_id_bytes.as_deref(),
        );
        // The contract address is where the box was actually found, not a global
        // config value (ERG and token reserves live at different addresses).
        reserve_info.set_contract_address(contract_address.to_string());

        Ok(reserve_info)
    }

    /// C4 setup checks for a root candidate (the first box of an
    /// (owner, denomination) lineage). Returns the violation reason, or `None`
    /// when the box is a cleanly created reserve.
    ///
    /// A reserve created with a pending refund (R7 != 0) can be withdrawn
    /// immediately, and a reserve created with a non-empty tree (R5) can block
    /// creditors from ever redeeming, so neither must be counted as collateral.
    /// These checks must NOT run on successor boxes: a redemption legitimately
    /// leaves R5 non-empty and refund initiation legitimately sets R7.
    fn root_setup_violation(
        scan_box: &ScanBox,
        reserve_info: &ExtendedReserveInfo,
    ) -> Option<String> {
        if reserve_info.base_info.refund_initiation_height != 0 {
            return Some(format!(
                "Reserve box {} created with a pending refund (R7 = {})",
                reserve_info.box_id, reserve_info.base_info.refund_initiation_height
            ));
        }
        let empty_tree_register = empty_reserve_tree_register();
        match scan_box.additional_registers.get("R5") {
            Some(r5) if r5.eq_ignore_ascii_case(&empty_tree_register) => None,
            other => Some(format!(
                "Reserve box {} created with a non-empty AVL tree (R5 = {:?})",
                reserve_info.box_id, other
            )),
        }
    }

    /// Fetch the inputs of a transaction as `(box_id, address, creating_tx_id,
    /// ergo_tree)` tuples via `GET /blockchain/transaction/byId/{txId}`.
    async fn fetch_tx_inputs(
        &self,
        tx_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, ScannerError> {
        let url = format!(
            "{}/blockchain/transaction/byId/{}",
            self.config.node_url, tx_id
        );

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| {
                ScannerError::HttpError(format!("Failed to fetch transaction {}: {}", tx_id, e))
            })?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!(
                "Node returned status {} for transaction {}",
                response.status(),
                tx_id
            )));
        }

        let tx: serde_json::Value = crate::http::read_json_capped(response).await.map_err(|e| {
            ScannerError::JsonError(format!("Failed to parse transaction {}: {}", tx_id, e))
        })?;

        let inputs = tx.get("inputs").and_then(|i| i.as_array()).ok_or_else(|| {
            ScannerError::JsonError(format!("Transaction {} has no inputs array", tx_id))
        })?;

        Ok(inputs
            .iter()
            .filter_map(|input| {
                let box_id = input.get("boxId")?.as_str()?.to_string();
                let address = input
                    .get("address")
                    .and_then(|a| a.as_str())
                    .unwrap_or_default()
                    .to_string();
                let creating_tx = input
                    .get("transactionId")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                let ergo_tree = input
                    .get("ergoTree")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                Some((box_id, address, creating_tx, ergo_tree))
            })
            .collect())
    }

    /// Check whether `scan_box` descends from the tracked reserve box
    /// `known_box_id`: walk creating transactions backwards until an input is
    /// the known box (possibly across generations missed while the tracker was
    /// down), following only inputs at the reserve contract address. The
    /// reserve contract enforces register continuity for successors
    /// (`contract/basis.es`: selfOut.R4 == SELF.R4, per-action R5/R7 rules), so
    /// a verified successor needs no further validation.
    async fn is_lineage_successor(
        &self,
        scan_box: &ScanBox,
        known_box_id: &str,
        contract_address: &str,
    ) -> Result<bool, ScannerError> {
        let mut tx_id = scan_box.transaction_id.clone();
        for _ in 0..LINEAGE_WALK_DEPTH {
            let inputs = self.fetch_tx_inputs(&tx_id).await?;
            if inputs
                .iter()
                .any(|(box_id, _, _, _)| normalized_box_id(box_id) == known_box_id)
            {
                return Ok(true);
            }
            // Follow the reserve-contract input (the previous generation), if any.
            match inputs
                .into_iter()
                .find(|(_, address, _, _)| address == contract_address)
            {
                Some((_, _, parent_tx, _)) if !parent_tx.is_empty() => tx_id = parent_tx,
                _ => return Ok(false),
            }
        }
        Ok(false)
    }

    /// Check whether the transaction creating `scan_box` spends an input (at any
    /// position) locked by P2PK of `owner_pubkey` — ergoTree
    /// `0008cd || 33-byte pubkey`. Only the holder of the R4 key can produce
    /// such a transaction, so this authenticates the otherwise-unauthenticated
    /// R4 register at reserve creation (issue #19).
    async fn has_owner_p2pk_input(
        &self,
        scan_box: &ScanBox,
        owner_pubkey: &str,
    ) -> Result<bool, ScannerError> {
        let expected = p2pk_ergo_tree(owner_pubkey);
        let inputs = self.fetch_tx_inputs(&scan_box.transaction_id).await?;
        Ok(inputs
            .iter()
            .any(|(_, _, _, tree)| tree.eq_ignore_ascii_case(&expected)))
    }

    /// Admission control for a parsed reserve box (issue #19).
    ///
    /// A box is admitted only when it is
    /// 1. the currently tracked box itself (refresh), or
    /// 2. a C4-clean root for an (owner, denomination) with no tracked reserve,
    ///    whose creating transaction spends a P2PK input of the R4 owner key
    ///    (authenticating the otherwise-unauthenticated R4 register), or
    /// 3. a verified successor of the tracked box for its (owner, denomination).
    ///
    /// Anything else is quarantined, so a dust duplicate with the victim's key
    /// in R4 can never turn `resolve_issuer_reserve` ambiguous and block the
    /// issuer. A box already persisted as quarantined stays quarantined (an
    /// attacker cannot race a successor into the same scan cycle to get
    /// re-evaluated).
    pub async fn admit_reserve_box(
        &self,
        scan_box: &ScanBox,
        reserve_info: &ExtendedReserveInfo,
        contract_address: &str,
    ) -> ReserveAdmission {
        let box_id = reserve_info.box_id.clone();

        // The tracked box itself is always admitted (periodic refresh).
        if self.reserve_tracker.get_reserve(&box_id).is_ok() {
            return ReserveAdmission::Admitted;
        }

        // Once quarantined, always quarantined.
        match self.reserve_storage.get_quarantined(&box_id) {
            Ok(Some(record)) => {
                return ReserveAdmission::Quarantined {
                    reason: record.reason,
                }
            }
            Ok(None) => {}
            Err(e) => {
                warn!("Failed to read quarantine record for {}: {:?}", box_id, e);
            }
        }

        let denomination = reserve_info.denomination();
        let tracked = self
            .reserve_tracker
            .get_reserves_by_owner(&reserve_info.owner_pubkey)
            .into_iter()
            .find(|r| r.denomination().as_ref() == denomination.as_ref());

        match tracked {
            None => {
                // Root candidate: cheap C4 setup checks first, then the P2PK
                // ownership proof (requires a node call).
                if let Some(violation) = Self::root_setup_violation(scan_box, reserve_info) {
                    return ReserveAdmission::Quarantined { reason: violation };
                }
                let proof = match self
                    .has_owner_p2pk_input(scan_box, &reserve_info.owner_pubkey)
                    .await
                {
                    Ok(v) => Some(v),
                    Err(e) => {
                        warn!(
                            "P2PK ownership check for reserve root {} failed ({}); retrying next cycle",
                            box_id, e
                        );
                        None
                    }
                };
                match decide_admission(false, proof, true) {
                    AdmissionVerdict::Admit => ReserveAdmission::Admitted,
                    AdmissionVerdict::Quarantine => ReserveAdmission::Quarantined {
                        reason: format!(
                            "reserve root {} for owner {} was created by a transaction spending no P2PK input of the R4 key; \
                             R4 is not authenticated on-chain, so this is likely a dust-box blocking attempt (issue #19)",
                            box_id, reserve_info.owner_pubkey
                        ),
                    },
                    AdmissionVerdict::Retry => ReserveAdmission::Retry,
                }
            }
            Some(tracked) => {
                let lineage = match self
                    .is_lineage_successor(scan_box, &tracked.box_id, contract_address)
                    .await
                {
                    Ok(v) => Some(v),
                    Err(e) => {
                        warn!(
                            "Lineage check for reserve box {} failed ({}); retrying next cycle",
                            box_id, e
                        );
                        None
                    }
                };
                match decide_admission(true, lineage, false) {
                    AdmissionVerdict::Admit => ReserveAdmission::Admitted,
                    AdmissionVerdict::Quarantine => ReserveAdmission::Quarantined {
                        reason: format!(
                            "duplicate reserve box {} for owner {} in denomination {} does not spend the tracked box {}; \
                             R4 is not authenticated on-chain, so this is likely a dust-box blocking attempt (issue #19)",
                            box_id,
                            reserve_info.owner_pubkey,
                            denomination
                                .map(|d| d.denomination_id())
                                .unwrap_or_else(|| "unknown".to_string()),
                            tracked.box_id
                        ),
                    },
                    AdmissionVerdict::Retry => ReserveAdmission::Retry,
                }
            }
        }
    }

    /// Parse, admit, and (when admitted) track and persist one scanned reserve
    /// box. Both ingestion paths (`process_scan_boxes` and the server's scanner
    /// sync) go through here so admission control cannot be bypassed
    /// (issue #19).
    pub async fn ingest_scan_box(&self, tagged: &TaggedScanBox) -> ScanBoxIngest {
        let scan_box = &tagged.scan_box;
        let normalized_id = normalized_box_id(&scan_box.box_id);

        let reserve_info =
            match self.parse_reserve_box(scan_box, tagged.kind, &tagged.contract_address) {
                Ok(info) => info,
                Err(e) => {
                    warn!(
                        "Skipping unparseable reserve box {}: {} - registers: {:?}",
                        scan_box.box_id, e, scan_box.additional_registers
                    );
                    return ScanBoxIngest::Rejected(normalized_id);
                }
            };

        match self
            .admit_reserve_box(scan_box, &reserve_info, &tagged.contract_address)
            .await
        {
            ReserveAdmission::Admitted => {
                if let Err(e) = self.reserve_tracker.update_reserve(reserve_info.clone()) {
                    warn!("Failed to update reserve {}: {}", scan_box.box_id, e);
                    return ScanBoxIngest::Rejected(normalized_id);
                }
                if let Err(e) = self.reserve_storage.store_reserve(&reserve_info) {
                    warn!(
                        "Failed to persist reserve {} to database: {:?}",
                        scan_box.box_id, e
                    );
                } else {
                    info!("Updated and persisted reserve: {}", scan_box.box_id);
                }
                ScanBoxIngest::Tracked(reserve_info.box_id)
            }
            ReserveAdmission::Quarantined { reason } => {
                warn!("Quarantining reserve box {}: {}", scan_box.box_id, reason);
                let record = QuarantinedReserve {
                    box_id: reserve_info.box_id.clone(),
                    owner_pubkey: reserve_info.owner_pubkey.clone(),
                    denomination: reserve_info
                        .denomination()
                        .map(|d| d.denomination_id())
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason,
                    quarantined_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };
                if let Err(e) = self.reserve_storage.store_quarantined(&record) {
                    warn!(
                        "Failed to persist quarantine record for {}: {:?}",
                        scan_box.box_id, e
                    );
                }
                ScanBoxIngest::Rejected(reserve_info.box_id)
            }
            ReserveAdmission::Retry => {
                let denomination = reserve_info.denomination().unwrap_or(crate::DenomKey::Erg);
                ScanBoxIngest::Pending(reserve_info.owner_pubkey.clone(), denomination)
            }
        }
    }

    /// Process scan boxes and update reserve tracker
    pub async fn process_scan_boxes(&self) -> Result<(), ScannerError> {
        info!("Starting to process scan boxes...");
        let scan_boxes = self.get_scan_boxes().await?;
        info!("Retrieved {} scan boxes to process", scan_boxes.len());

        let mut current_box_ids = Vec::new();
        // Box IDs that failed admission, normalized the same way
        // `ExtendedReserveInfo.box_id` is stored. A rejected box is skipped but
        // never triggers removal of itself, and it must not block removal of
        // OTHER genuinely-spent reserves.
        let mut quarantined_box_ids: Vec<String> = Vec::new();
        // (Owner, denomination) pairs with a successor candidate whose lineage
        // check was inconclusive this cycle (issue #19): the tracked box must
        // not be removed as spent, or a transient node error would orphan the
        // lineage and brick the issuer.
        let mut pending_lineages: Vec<(String, crate::DenomKey)> = Vec::new();

        for tagged in &scan_boxes {
            let scan_box = &tagged.scan_box;
            debug!(
                "Processing scan box: ID={}, value={}, registers={:?}",
                scan_box.box_id, scan_box.value, scan_box.additional_registers
            );

            match self.ingest_scan_box(tagged).await {
                ScanBoxIngest::Tracked(box_id) => {
                    debug!("Successfully tracked reserve box: box_id={}", box_id);
                    current_box_ids.push(box_id);
                }
                ScanBoxIngest::Rejected(box_id) => {
                    quarantined_box_ids.push(box_id);
                }
                ScanBoxIngest::Pending(owner, denomination) => {
                    pending_lineages.push((owner, denomination));
                }
            }
        }

        // Remove reserves that are no longer in the scan
        // NOTE: Disabled for testing to prevent manually-inserted reserves from being deleted
        // when they don't exist on the local test node.
        let all_reserves = self.reserve_tracker.get_all_reserves();
        info!(
            "Current tracker has {} reserves, {} are still active in scan",
            all_reserves.len(),
            current_box_ids.len()
        );

        // Only remove reserves if we actually found VALID boxes in the scan.
        // If no valid reserves were parsed (e.g., all failed validation), don't remove manually-inserted reserves.
        if !current_box_ids.is_empty() {
            for reserve in all_reserves {
                if !current_box_ids.contains(&reserve.box_id)
                    && !quarantined_box_ids.contains(&reserve.box_id)
                {
                    // A successor candidate is pending a lineage retry for this
                    // (owner, denomination): keep the tracked box so the lineage
                    // anchor is not lost (issue #19).
                    let denomination = reserve.denomination();
                    if pending_lineages.iter().any(|(owner, pending_denom)| {
                        *owner == reserve.owner_pubkey
                            && Some(pending_denom) == denomination.as_ref()
                    }) {
                        info!(
                            "Keeping reserve {} (successor lineage check pending)",
                            reserve.box_id
                        );
                        continue;
                    }
                    info!(
                        "Removing spent reserve: {} (not found in current scan)",
                        reserve.box_id
                    );
                    // Remove from in-memory tracker
                    if let Err(e) = self.reserve_tracker.remove_reserve(&reserve.box_id) {
                        warn!("Failed to remove reserve {}: {}", reserve.box_id, e);
                    } else {
                        // Remove from database
                        if let Err(e) = self.reserve_storage.remove_reserve(&reserve.box_id) {
                            warn!(
                                "Failed to remove reserve {} from database: {:?}",
                                reserve.box_id, e
                            );
                        } else {
                            info!("Removed spent reserve: {}", reserve.box_id);
                        }
                    }
                }
            }
        } else {
            info!("Scan returned 0 boxes, skipping reserve removal to preserve manually-inserted reserves");
        }

        debug!(
            "Finished processing scan boxes: {} processed, {} in tracker after processing",
            scan_boxes.len(),
            self.reserve_tracker.get_all_reserves().len()
        );

        Ok(())
    }
}

/// Start the scanner in background
pub async fn start_scanner(state: ServerState) -> Result<(), ScannerError> {
    let state = Arc::new(state);
    tokio::spawn(reserve_scanner_loop(state.clone()));
    Ok(())
}

/// Create a scanner with default configuration
pub fn create_default_scanner(data_dir: impl AsRef<Path>) -> Result<ServerState, ScannerError> {
    let config = NodeConfig::default();
    ServerState::new(config, data_dir)
}

/// Ergo box representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: std::collections::HashMap<String, String>,
}

impl ErgoBox {
    /// Get a specific register value
    pub fn get_register(&self, register: &str) -> Option<&str> {
        self.additional_registers.get(register).map(|s| s.as_str())
    }

    /// Check if this box has a specific register
    pub fn has_register(&self, register: &str) -> bool {
        self.additional_registers.contains_key(register)
    }
}

/// Scan box representation from Ergo node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: std::collections::HashMap<String, String>,
    /// Assets in the box (tokens)
    #[serde(default)]
    pub assets: Vec<BoxAsset>,
}

/// Asset in an Ergo box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxAsset {
    /// Token ID
    pub token_id: String,
    /// Amount
    pub amount: u64,
}

/// Events related to reserve activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReserveEvent {
    /// A new reserve was created
    ReserveCreated {
        box_id: String,
        owner_pubkey: String,
        collateral_amount: u64,
        height: u64,
    },
    /// An existing reserve was topped up
    ReserveToppedUp {
        box_id: String,
        additional_collateral: u64,
        height: u64,
    },
    /// A redemption occurred from a reserve
    ReserveRedeemed {
        box_id: String,
        redeemed_amount: u64,
        height: u64,
    },
    /// A reserve was spent/closed
    ReserveSpent { box_id: String, height: u64 },
}

/// Default node configuration
impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            start_height: None,
            reserve_contract_p2s: None,
            token_reserve_contract_p2s: None,
            tracker_nft_id: None,
            node_url: "http://localhost:9053".to_string(), // Local Ergo node
            scan_name: Some("Basis Reserve Scanner".to_string()),
            api_key: None,
        }
    }
}

/// Reserve scanner loop (background task)
pub async fn reserve_scanner_loop(state: Arc<ServerState>) -> Result<(), ScannerError> {
    info!("Starting reserve scanner background loop");

    let mut consecutive_failures = 0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 5;

    loop {
        // Update current height
        match state.fetch_current_height().await {
            Ok(height) => {
                // Log height update at INFO level
                let previous_height = {
                    let inner = state.inner.lock().await;
                    inner.current_height
                };

                if height != previous_height {
                    info!("Current Ergo blockchain height: {}", height);
                    // Update current height in state
                    {
                        let mut inner = state.inner.lock().await;
                        inner.current_height = height;
                    }
                }

                // Process scan boxes whenever the height has advanced
                if height > state.last_scanned_height().await {
                    match state.process_scan_boxes().await {
                        Ok(()) => {
                            consecutive_failures = 0;
                            // Update last scanned height on success
                            {
                                let mut inner = state.inner.lock().await;
                                inner.last_scanned_height = height;
                            }
                        }
                        Err(e) => {
                            error!("Failed to process scan boxes: {}", e);
                            consecutive_failures += 1;

                            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                error!(
                                    "Too many consecutive failures ({}), waiting before retry",
                                    consecutive_failures
                                );
                                tokio::time::sleep(Duration::from_secs(60)).await;
                                // Wait longer after many failures
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to get current height: {}", e);
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!(
                        "Too many consecutive failures ({}), waiting before retry",
                        consecutive_failures
                    );
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }
        }

        // Wait before next scan (shorter wait if we're recovering)
        let wait_time = if consecutive_failures > 0 {
            Duration::from_secs(10) // Shorter wait during recovery
        } else {
            Duration::from_secs(30) // Normal wait
        };
        tokio::time::sleep(wait_time).await;
    }
}

/// Serialized value of the R5 register of a freshly created reserve: the empty
/// AVL+ tree (32-byte keys, insert+update flags `0x03`, no fixed value length).
/// Format: `0x64` (AvlTree type) || 33-byte digest || `03` || `20` || `00`.
pub fn empty_reserve_tree_register() -> String {
    let tree = basis_trees::BasisAvlTree::new().expect("failed to create empty AVL tree");
    format!("64{}032000", hex::encode(tree.root_digest()))
}

/// Decode an Ergo `Long` constant from its serialized hex representation.
/// Format: type byte `05` followed by zigzag-VLQ encoded value.
/// Missing, malformed, or negative values are treated as `0`.
pub fn decode_ergo_long_register(value: Option<&String>) -> u64 {
    let hex = match value {
        Some(h) if h.starts_with("05") && h.len() > 2 => &h[2..],
        _ => return 0,
    };
    let bytes = match hex::decode(hex) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    decode_vlq_long(&bytes)
        .ok()
        .and_then(|v| if v >= 0 { Some(v as u64) } else { None })
        .unwrap_or(0)
}

/// Decode a zigzag-VLQ encoded signed long.
fn decode_vlq_long(bytes: &[u8]) -> Result<i64, String> {
    let mut zigzag: u64 = 0;
    let mut shift = 0;
    for &byte in bytes {
        let value = (byte & 0x7f) as u64;
        zigzag |= value << shift;
        if byte & 0x80 == 0 {
            let n = zigzag as i64;
            return Ok((n >> 1) ^ -(n & 1));
        }
        shift += 7;
        if shift > 63 {
            return Err("VLQ overflow".to_string());
        }
    }
    Err("Incomplete VLQ".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Valid compressed secp256k1 point (generator) with the 0x07 GroupElement prefix.
    const VALID_PREFIXED_PUBKEY: &str =
        "070279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn test_empty_reserve_tree_register_constant() {
        // The empty AVL+ tree register value is a protocol constant: reserves are
        // only accepted when first seen with exactly this tree in R5.
        assert_eq!(
            empty_reserve_tree_register(),
            "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000"
        );
    }

    /// Phase 2 acceptance (specs/ergo/multi_collateral_reserves.md §13): mixed ERG and
    /// token reserves parse per box, each with its own denomination and the contract
    /// address of the slot it was found at.
    #[test]
    fn test_parse_mixed_denomination_boxes() {
        let nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let token_a =
            "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669".to_string();
        let token_b = "b8".repeat(32);

        let state = c4_test_state(NodeConfig::default());

        // ERG reserve: one NFT, ERG value is the collateral.
        let erg_box = c4_test_box();
        let erg = state
            .parse_reserve_box(&erg_box, ReserveContractKind::Erg, "erg_p2s")
            .expect("ERG reserve parses");
        assert_eq!(erg.base_info.collateral_amount, erg_box.value);
        assert_eq!(erg.base_info.reserve_token_id, "");
        assert_eq!(erg.denomination(), Some(crate::DenomKey::Erg));
        assert_eq!(erg.base_info.contract_address, "erg_p2s");

        // Token reserve A: collateral is tokens(1) amount, denomination is tokens(1) id.
        let mut tok_a_box = c4_test_box();
        tok_a_box.box_id = "token_box_a".to_string();
        tok_a_box.assets = vec![
            BoxAsset {
                token_id: nft_id.to_string(),
                amount: 1,
            },
            BoxAsset {
                token_id: token_a.clone(),
                amount: 5000,
            },
        ];
        let tok_a = state
            .parse_reserve_box(&tok_a_box, ReserveContractKind::Token, "token_p2s")
            .expect("token reserve A parses");
        assert_eq!(tok_a.base_info.collateral_amount, 5000);
        assert_eq!(tok_a.base_info.reserve_token_id, token_a.clone());
        assert_eq!(
            tok_a.denomination(),
            Some(crate::DenomKey::Token(
                hex::decode(&token_a).unwrap().try_into().unwrap()
            ))
        );
        assert_eq!(tok_a.base_info.contract_address, "token_p2s");

        // A different token on the same contract address is a different denomination.
        let mut tok_b_box = tok_a_box.clone();
        tok_b_box.box_id = "token_box_b".to_string();
        tok_b_box.assets[1] = BoxAsset {
            token_id: token_b.to_string(),
            amount: 700,
        };
        let tok_b = state
            .parse_reserve_box(&tok_b_box, ReserveContractKind::Token, "token_p2s")
            .expect("token reserve B parses");
        assert_ne!(tok_a.denomination(), tok_b.denomination());

        // Malformed token boxes are rejected, not misclassified.
        let mut bad = c4_test_box();
        bad.assets = vec![]; // token slot but no collateral token
        assert!(state
            .parse_reserve_box(&bad, ReserveContractKind::Token, "token_p2s")
            .is_err());
        let mut bad3 = c4_test_box();
        bad3.assets = vec![
            BoxAsset {
                token_id: nft_id.to_string(),
                amount: 1,
            },
            BoxAsset {
                token_id: token_a.clone(),
                amount: 5,
            },
            BoxAsset {
                token_id: token_b.clone(),
                amount: 5,
            },
        ];
        assert!(state
            .parse_reserve_box(&bad3, ReserveContractKind::Token, "token_p2s")
            .is_err());
    }

    #[test]
    fn test_parse_reserve_box_with_r6_register() {
        // Create a mock scan box with a public key that has the 0x07 prefix
        // and a valid 32-byte tracker NFT ID in R6 register
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = VALID_PREFIXED_PUBKEY;
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // Fresh reserves must carry the empty AVL+ tree in R5 (C4)
        registers.insert("R5".to_string(), empty_reserve_tree_register());
        // This is a 32-byte tracker NFT ID with Ergo Coll[Byte] serialization prefix (0e20 + 64 hex chars)
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let tracker_nft_id_serialized = format!("0e20{}", tracker_nft_id);
        registers.insert("R6".to_string(), tracker_nft_id_serialized);

        let scan_box = ScanBox {
            box_id: "test_box_id".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: "test_ergo_tree".to_string(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = NodeConfig::default();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function
        let result =
            server_state.parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s");

        match result {
            Ok(reserve_info) => {
                // The owner_pubkey should have the 0x07 prefix stripped
                let expected_pubkey = &prefixed_pubkey[2..]; // Remove first 2 characters (07)

                assert_eq!(reserve_info.owner_pubkey, expected_pubkey);
                // The tracker_nft_id should match the one from R6 register
                assert_eq!(reserve_info.base_info.tracker_nft_id, tracker_nft_id);
                println!(
                    "SUCCESS: Prefix was correctly stripped. Original: {}, Stripped: {}",
                    prefixed_pubkey, reserve_info.owner_pubkey
                );
                println!(
                    "SUCCESS: Tracker NFT ID correctly extracted from R6 register: {}",
                    reserve_info.base_info.tracker_nft_id
                );
            }
            Err(e) => {
                panic!("Failed to parse reserve box: {:?}", e);
            }
        }
    }

    #[test]
    fn test_parse_reserve_box_missing_r6_register() {
        // Create a mock scan box with a public key but missing R6 register
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = VALID_PREFIXED_PUBKEY;
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // Note: R6 register is intentionally missing

        let scan_box = ScanBox {
            box_id: "test_box_id_2".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: "test_ergo_tree".to_string(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = NodeConfig::default();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function - should return an error
        let result =
            server_state.parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s");

        match result {
            Ok(_) => {
                panic!("Expected error when R6 register is missing, but parsing succeeded");
            }
            Err(e) => {
                // Check that the error message mentions the missing R6 register
                assert!(e.to_string().contains("Missing R6 register"));
                println!(
                    "SUCCESS: Correctly returned error for missing R6 register: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_parse_reserve_box_invalid_r6_length() {
        // Create a mock scan box with an invalid R6 register (not 32 bytes)
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = VALID_PREFIXED_PUBKEY;
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // This is an invalid tracker NFT ID with wrong length (only 16 bytes = 32 hex chars, should be 32 bytes = 64 hex chars)
        let invalid_tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f304";
        registers.insert("R6".to_string(), invalid_tracker_nft_id.to_string());

        let scan_box = ScanBox {
            box_id: "test_box_id_3".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: "test_ergo_tree".to_string(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = NodeConfig::default();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function - should return an error
        let result =
            server_state.parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s");

        match result {
            Ok(_) => {
                panic!("Expected error when R6 register has invalid length, but parsing succeeded");
            }
            Err(e) => {
                // Check that the error message mentions the invalid length
                assert!(e.to_string().contains("Invalid tracker NFT ID length"));
                println!(
                    "SUCCESS: Correctly returned error for invalid R6 register length: {:?}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_process_scan_boxes_removes_absent_reserves_on_clean_scan() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Mock node returns one valid reserve box. A pre-existing stored reserve
        // that is absent from the scan should be removed because the snapshot is
        // fully parseable and therefore authoritative.
        let prefixed_pubkey = VALID_PREFIXED_PUBKEY;
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let empty_tree = empty_reserve_tree_register();
        let body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"valid_box_1\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx1\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                "]"
            ),
            prefixed_pubkey, empty_tree, tracker_nft_id
        );

        // The scanned box is a root: its creating tx must spend a P2PK input of
        // the R4 owner key (issue #19).
        let owner_tree = p2pk_ergo_tree(&VALID_PREFIXED_PUBKEY[2..]);
        let txs = HashMap::from([(
            "tx1".to_string(),
            tx_json(
                "tx1",
                &[(
                    &"33".repeat(32),
                    "some_address",
                    &"44".repeat(32),
                    &owner_tree,
                )],
            ),
        )]);
        let addr = spawn_mock_node(body, std::sync::Arc::new(txs)).await;

        let config = NodeConfig {
            node_url: format!("http://{}", addr),
            reserve_contract_p2s: Some("test_p2s".to_string()),
            ..Default::default()
        };
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_remove_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Pre-existing reserve that is NOT in the scan result.
        let preexisting = ExtendedReserveInfo {
            base_info: crate::ReserveInfo {
                collateral_amount: 500,
                last_updated_height: 0,
                contract_address: "test".to_string(),
                tracker_nft_id: "test".to_string(),
                refund_initiation_height: 0,
                reserve_token_id: String::new(),
            },
            total_debt: 0,
            box_id: "preexisting_box".to_string(),
            owner_pubkey: "deadbeef".to_string(),
            last_updated_timestamp: 0,
        };
        server_state
            .reserve_tracker
            .update_reserve(preexisting.clone())
            .unwrap();
        server_state
            .reserve_storage
            .store_reserve(&preexisting)
            .unwrap();

        server_state.process_scan_boxes().await.unwrap();

        let remaining: Vec<String> = server_state
            .reserve_tracker
            .get_all_reserves()
            .into_iter()
            .map(|r| r.box_id)
            .collect();
        assert!(
            remaining.contains(&hex::encode("valid_box_1")),
            "valid scanned box should be tracked (hex-encoded box id): {:?}",
            remaining
        );
        assert!(
            !remaining.contains(&"preexisting_box".to_string()),
            "absent pre-existing reserve must be removed on a clean scan: {:?}",
            remaining
        );
    }

    #[tokio::test]
    async fn test_process_scan_boxes_preserves_reserves_on_empty_scan() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Mock node returns an empty array. No reserves should be removed because
        // an empty scan is not treated as authoritative proof that all reserves
        // were spent (this also protects manually-inserted test reserves).
        let body = "[]";

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let body = body.to_string();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(), body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });

        let config = NodeConfig {
            node_url: format!("http://{}", addr),
            reserve_contract_p2s: Some("test_p2s".to_string()),
            ..Default::default()
        };
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_empty_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        let preexisting = ExtendedReserveInfo {
            base_info: crate::ReserveInfo {
                collateral_amount: 500,
                last_updated_height: 0,
                contract_address: "test".to_string(),
                tracker_nft_id: "test".to_string(),
                refund_initiation_height: 0,
                reserve_token_id: String::new(),
            },
            total_debt: 0,
            box_id: "preexisting_box".to_string(),
            owner_pubkey: "deadbeef".to_string(),
            last_updated_timestamp: 0,
        };
        server_state
            .reserve_tracker
            .update_reserve(preexisting.clone())
            .unwrap();
        server_state
            .reserve_storage
            .store_reserve(&preexisting)
            .unwrap();

        server_state.process_scan_boxes().await.unwrap();

        let remaining: Vec<String> = server_state
            .reserve_tracker
            .get_all_reserves()
            .into_iter()
            .map(|r| r.box_id)
            .collect();
        assert!(
            remaining.contains(&"preexisting_box".to_string()),
            "pre-existing reserve must be preserved on an empty scan: {:?}",
            remaining
        );
    }

    #[tokio::test]
    async fn test_process_scan_boxes_removes_spent_reserve_despite_garbage_box() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Mock node returning one valid reserve box and one malformed box
        // (missing R4). The malformed box is quarantined individually: it is
        // skipped with a warning, but it must NOT block removal of a tracked
        // reserve that is genuinely spent (absent from the scan).
        let prefixed_pubkey = VALID_PREFIXED_PUBKEY;
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let empty_tree = empty_reserve_tree_register();
        let body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"valid_box_1\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx1\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                ",",
                "{{\"boxId\":\"bad_box_2\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx2\",\"additionalRegisters\":{{}}}}",
                "]"
            ),
            prefixed_pubkey, empty_tree, tracker_nft_id
        );

        // The valid box is a root: its creating tx must spend a P2PK input of
        // the R4 owner key (issue #19).
        let owner_tree = p2pk_ergo_tree(&VALID_PREFIXED_PUBKEY[2..]);
        let txs = HashMap::from([(
            "tx1".to_string(),
            tx_json(
                "tx1",
                &[(
                    &"33".repeat(32),
                    "some_address",
                    &"44".repeat(32),
                    &owner_tree,
                )],
            ),
        )]);
        let addr = spawn_mock_node(body, std::sync::Arc::new(txs)).await;

        let config = NodeConfig {
            node_url: format!("http://{}", addr),
            reserve_contract_p2s: Some("test_p2s".to_string()),
            ..Default::default()
        };
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Pre-existing reserve that is NOT in the scan result (genuinely spent).
        let preexisting = ExtendedReserveInfo {
            base_info: crate::ReserveInfo {
                collateral_amount: 500,
                last_updated_height: 0,
                contract_address: "test".to_string(),
                tracker_nft_id: "test".to_string(),
                refund_initiation_height: 0,
                reserve_token_id: String::new(),
            },
            total_debt: 0,
            box_id: "preexisting_box".to_string(),
            owner_pubkey: "deadbeef".to_string(),
            last_updated_timestamp: 0,
        };
        server_state
            .reserve_tracker
            .update_reserve(preexisting.clone())
            .unwrap();
        server_state
            .reserve_storage
            .store_reserve(&preexisting)
            .unwrap();

        server_state.process_scan_boxes().await.unwrap();

        let remaining: Vec<String> = server_state
            .reserve_tracker
            .get_all_reserves()
            .into_iter()
            .map(|r| r.box_id)
            .collect();
        assert!(
            remaining.contains(&hex::encode("valid_box_1")),
            "valid scanned box should be tracked (hex-encoded box id): {:?}",
            remaining
        );
        assert!(
            !remaining.contains(&hex::encode("bad_box_2")),
            "unparseable box must be skipped, not tracked: {:?}",
            remaining
        );
        assert!(
            !remaining.contains(&"preexisting_box".to_string()),
            "one garbage box must not block removal of a genuinely spent reserve: {:?}",
            remaining
        );
    }

    #[tokio::test]
    async fn test_get_unspent_reserve_boxes_paginates_all_pages() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // The node defaults to 5 boxes per page; the scanner must pass an
        // explicit limit and keep paging until a short page comes back.
        // Mock node: page 1 is full (RESERVE_BOX_PAGE_LIMIT boxes), page 2 has
        // 7 more; ALL of them must end up tracked.
        let full_page = RESERVE_BOX_PAGE_LIMIT as usize;
        let extra_page = 7usize;
        let total = full_page + extra_page;

        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let empty_tree = empty_reserve_tree_register();

        // Each box gets a distinct owner key: admission control (issue #19)
        // tracks only one reserve per (owner, denomination), so same-owner
        // duplicates would be quarantined instead of tracked.
        let owners: Vec<String> = (0..total)
            .map(|_| hex::encode(basis_core::impls::generate_keypair().1))
            .collect();

        let box_json = |i: usize| {
            format!(
                "{{\"boxId\":\"{:064x}\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                i
            ) + &format!(
                "\"transactionId\":\"{:064x}\",\"additionalRegisters\":{{\"R4\":\"07{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                i, owners[i], empty_tree, tracker_nft_id
            )
        };
        let page = |from: usize, count: usize| {
            format!(
                "[{}]",
                (from..from + count)
                    .map(box_json)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let page1 = page(0, full_page);
        let page2 = page(full_page, extra_page);

        // Every box is a root (distinct owner per box), so its creating tx must
        // spend a P2PK input of the owner key (issue #19).
        let txs: std::sync::Arc<HashMap<String, String>> = std::sync::Arc::new(
            (0..total)
                .map(|i| {
                    let tx_id = format!("{:064x}", i);
                    let tree = p2pk_ergo_tree(&owners[i]);
                    (
                        tx_id.clone(),
                        tx_json(
                            &tx_id,
                            &[(&"33".repeat(32), "some_address", &"44".repeat(32), &tree)],
                        ),
                    )
                })
                .collect(),
        );

        let seen_offsets = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
        let seen_offsets_server = seen_offsets.clone();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let page1 = page1.clone();
                let page2 = page2.clone();
                let seen_offsets = seen_offsets_server.clone();
                let txs = txs.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let request_line = request.lines().next().unwrap_or_default();
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string();

                    // Transaction lookups (issue #19 admission) are answered
                    // from the tx map; box-list queries are paginated below.
                    if let Some(tx_id) = path.strip_prefix("/blockchain/transaction/byId/") {
                        let (status, body) = match txs.get(tx_id) {
                            Some(body) => ("200 OK", body.clone()),
                            None => ("404 Not Found", "{}".to_string()),
                        };
                        let response = format!(
                            "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            status,
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        return;
                    }

                    let offset: u64 = request_line
                        .split('&')
                        .flat_map(|part| part.split('?'))
                        .find_map(|part| part.strip_prefix("offset="))
                        .and_then(|v| {
                            v.chars()
                                .take_while(|c| c.is_ascii_digit())
                                .collect::<String>()
                                .parse()
                                .ok()
                        })
                        .unwrap_or(0);
                    seen_offsets.lock().unwrap().push(offset);

                    let body = match offset {
                        0 => page1,
                        o if o == RESERVE_BOX_PAGE_LIMIT => page2,
                        _ => "[]".to_string(),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });

        let config = NodeConfig {
            node_url: format!("http://{}", addr),
            reserve_contract_p2s: Some("test_p2s".to_string()),
            ..Default::default()
        };
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_pagination_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        server_state.process_scan_boxes().await.unwrap();

        assert_eq!(
            seen_offsets.lock().unwrap().as_slice(),
            &[0u64, RESERVE_BOX_PAGE_LIMIT],
            "scanner must request page 2 after a full first page"
        );

        let tracked = server_state.reserve_tracker.get_all_reserves();
        assert_eq!(
            tracked.len(),
            total,
            "all reserves across every page must be tracked"
        );
        for i in 0..total {
            let box_id = format!("{:064x}", i);
            assert!(
                tracked.iter().any(|r| r.box_id == box_id),
                "reserve box {} from the scan must be tracked",
                box_id
            );
        }
    }

    /// Follow-up review N4: anyone can create maximum-size boxes at the reserve
    /// address for ~0.0015 ERG each. A full page of them, rendered the way the
    /// node returns indexed boxes, must still fit under the body cap, or every
    /// scan fails from then on.
    #[test]
    fn full_page_of_maximum_size_boxes_fits_the_body_cap() {
        let tree_hex = crate::contract_compiler::get_basis_token_reserve_ergo_tree_hex().unwrap();
        let address = crate::contract_compiler::get_basis_token_reserve_contract_p2s().unwrap();
        // Ergo caps a serialized box at 4096 bytes; let registers fill what the tree leaves.
        let per_register = (4096 - tree_hex.len() / 2 - 100) / 6;
        let register = || format!("0e{}", "ab".repeat(per_register));
        let box_json = |i: usize| {
            serde_json::json!({
                "boxId": format!("{:064x}", i),
                "transactionId": format!("{:064x}", i),
                "blockId": format!("{:064x}", i),
                "value": u64::MAX,
                "index": 0,
                "globalIndex": i,
                "creationHeight": 1_900_000,
                "settlementHeight": 1_900_000,
                "inclusionHeight": 1_900_000,
                "ergoTree": tree_hex,
                "address": address,
                "assets": [],
                "additionalRegisters": {
                    "R4": register(), "R5": register(), "R6": register(),
                    "R7": register(), "R8": register(), "R9": register()
                },
                "spentTransactionId": null,
                "spendingHeight": null
            })
        };
        let page: Vec<_> = (0..RESERVE_BOX_PAGE_LIMIT as usize).map(box_json).collect();
        let body = serde_json::to_string_pretty(&page).unwrap();
        assert!(
            body.len() <= crate::http::MAX_BODY_BYTES,
            "a full page of {} maximum-size boxes is {} bytes, over the {} byte cap",
            RESERVE_BOX_PAGE_LIMIT,
            body.len(),
            crate::http::MAX_BODY_BYTES
        );
    }

    #[test]
    fn test_parse_reserve_box_uses_token_collateral_per_box() {
        let reserve_token_id =
            "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669".to_string();
        let nft_id = "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string();
        let owner_pubkey = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let mut registers = std::collections::HashMap::new();
        registers.insert("R4".to_string(), format!("07{}", owner_pubkey));
        registers.insert("R5".to_string(), empty_reserve_tree_register());
        registers.insert("R6".to_string(), format!("0e20{}", nft_id));
        registers.insert("R7".to_string(), "05000000000000000000".to_string());

        let scan_box = ScanBox {
            box_id: "token_reserve_box".to_string(),
            value: 10000000, // 0.01 ERG min-box-value
            creation_height: 1000,
            ergo_tree: "test".to_string(),
            transaction_id: "test_tx".to_string(),
            additional_registers: registers,
            assets: vec![
                BoxAsset {
                    token_id: nft_id.clone(),
                    amount: 1,
                },
                BoxAsset {
                    token_id: reserve_token_id.clone(),
                    amount: 5000,
                },
            ],
        };

        let config = NodeConfig::default();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_token_scanner_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        let reserve_info = server_state
            .parse_reserve_box(&scan_box, ReserveContractKind::Token, "test_p2s")
            .expect("should parse token reserve box");

        assert_eq!(
            reserve_info.base_info.collateral_amount, 5000,
            "collateral should be taken from reserve token amount"
        );
        assert_eq!(
            reserve_info.base_info.reserve_token_id, reserve_token_id,
            "reserve_token_id should be recorded"
        );
        assert_eq!(reserve_info.base_info.tracker_nft_id, nft_id);
    }

    /// Build a minimal valid first-seen reserve box for the C4 rejection tests.
    fn c4_test_box() -> ScanBox {
        let nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), VALID_PREFIXED_PUBKEY.to_string());
        registers.insert("R5".to_string(), empty_reserve_tree_register());
        registers.insert("R6".to_string(), format!("0e20{}", nft_id));
        ScanBox {
            box_id: "c4_test_box".to_string(),
            value: 1000000000,
            creation_height: 1000,
            ergo_tree: "test_ergo_tree".to_string(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        }
    }

    fn c4_test_state(config: NodeConfig) -> ServerState {
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_c4_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        ServerState::new(config, &data_dir).expect("Failed to create server state")
    }

    #[test]
    fn test_parse_reserve_box_rejects_non_long_r7() {
        // An Int-typed R7 would make SELF.R7[Long] throw on-chain, bricking the
        // box, while the scanner would read it as 0.
        let mut scan_box = c4_test_box();
        scan_box
            .additional_registers
            .insert("R7".to_string(), "0401".to_string()); // Int 1
        let state = c4_test_state(NodeConfig::default());
        let err = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .unwrap_err();
        assert!(
            err.to_string().contains("not a Long"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_admission_quarantines_root_with_pending_refund() {
        // R7 = Long 1 (zigzag 2 -> VLQ 0x02): refund pending from creation.
        // Parsing succeeds (the box is structurally valid); the C4 setup check
        // rejects it at admission (issue #19).
        let mut scan_box = c4_test_box();
        scan_box
            .additional_registers
            .insert("R7".to_string(), "0502".to_string());
        let state = c4_test_state(NodeConfig::default());
        let info = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("structurally valid box parses");
        match state.admit_reserve_box(&scan_box, &info, "test_p2s").await {
            ReserveAdmission::Quarantined { reason } => {
                assert!(
                    reason.contains("pending refund"),
                    "unexpected reason: {}",
                    reason
                );
            }
            other => panic!("expected Quarantined, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_admission_quarantines_root_with_prefilled_tree() {
        let mut scan_box = c4_test_box();
        scan_box.additional_registers.insert(
            "R5".to_string(),
            "64ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff032000"
                .to_string(),
        );
        let state = c4_test_state(NodeConfig::default());
        let info = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("structurally valid box parses");
        match state.admit_reserve_box(&scan_box, &info, "test_p2s").await {
            ReserveAdmission::Quarantined { reason } => {
                assert!(
                    reason.contains("non-empty AVL tree"),
                    "unexpected reason: {}",
                    reason
                );
            }
            other => panic!("expected Quarantined, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_admission_admits_clean_root() {
        // A clean root whose creating transaction spends a P2PK input of the
        // R4 owner key is admitted (issue #19 ownership proof).
        let owner = &VALID_PREFIXED_PUBKEY[2..];
        let scan_box = c4_test_box();
        let owner_tree = p2pk_ergo_tree(owner);
        let txs = HashMap::from([(
            scan_box.transaction_id.clone(),
            tx_json(
                &scan_box.transaction_id,
                &[(
                    &"33".repeat(32),
                    "some_address",
                    &"44".repeat(32),
                    &owner_tree,
                )],
            ),
        )]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);
        let info = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("valid box parses");
        assert_eq!(
            state.admit_reserve_box(&scan_box, &info, "test_p2s").await,
            ReserveAdmission::Admitted
        );
    }

    #[tokio::test]
    async fn test_admission_quarantines_root_without_owner_p2pk_input() {
        // Front-run attempt (issue #19): a C4-clean box carrying the victim's
        // key in R4, but its creating transaction spends no P2PK input of that
        // key, so the box is never tracked.
        let scan_box = c4_test_box();
        let txs = HashMap::from([(
            scan_box.transaction_id.clone(),
            tx_json(
                &scan_box.transaction_id,
                &[(&"33".repeat(32), "some_address", &"44".repeat(32), "00")],
            ),
        )]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);
        let info = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("valid box parses");
        match state.admit_reserve_box(&scan_box, &info, "test_p2s").await {
            ReserveAdmission::Quarantined { reason } => {
                assert!(
                    reason.contains("no P2PK input"),
                    "unexpected reason: {}",
                    reason
                );
            }
            other => panic!("expected Quarantined, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_admission_quarantines_root_with_foreign_p2pk_input() {
        // The creating tx spends a P2PK input, but of a DIFFERENT key than R4.
        let scan_box = c4_test_box();
        let other_key = hex::encode(basis_core::impls::generate_keypair().1);
        let foreign_tree = p2pk_ergo_tree(&other_key);
        let txs = HashMap::from([(
            scan_box.transaction_id.clone(),
            tx_json(
                &scan_box.transaction_id,
                &[(
                    &"33".repeat(32),
                    "some_address",
                    &"44".repeat(32),
                    &foreign_tree,
                )],
            ),
        )]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);
        let info = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("valid box parses");
        match state.admit_reserve_box(&scan_box, &info, "test_p2s").await {
            ReserveAdmission::Quarantined { reason } => {
                assert!(
                    reason.contains("no P2PK input"),
                    "unexpected reason: {}",
                    reason
                );
            }
            other => panic!("expected Quarantined, got {:?}", other),
        }
    }

    #[test]
    fn test_decide_admission_matrix() {
        // Root candidates: only C4-clean boxes whose creating tx spends a P2PK
        // input of the R4 key may open a lineage (issue #19).
        assert_eq!(
            decide_admission(false, Some(true), true),
            AdmissionVerdict::Admit
        );
        assert_eq!(
            decide_admission(false, Some(true), false),
            AdmissionVerdict::Quarantine
        );
        assert_eq!(
            decide_admission(false, Some(false), true),
            AdmissionVerdict::Quarantine
        );
        assert_eq!(decide_admission(false, None, true), AdmissionVerdict::Retry);
        // With a tracked reserve: only verified successors are admitted.
        assert_eq!(
            decide_admission(true, Some(true), false),
            AdmissionVerdict::Admit
        );
        assert_eq!(
            decide_admission(true, Some(false), false),
            AdmissionVerdict::Quarantine
        );
        assert_eq!(decide_admission(true, None, false), AdmissionVerdict::Retry);
    }

    #[test]
    fn test_parse_reserve_box_rejects_foreign_tracker_nft() {
        let scan_box = c4_test_box();
        let config = NodeConfig {
            tracker_nft_id: Some(
                "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string(),
            ),
            ..Default::default()
        };
        let state = c4_test_state(config);
        let err = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .unwrap_err();
        assert!(
            err.to_string().contains("Tracker NFT ID mismatch"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_parse_reserve_box_accepts_own_tracker_nft() {
        let scan_box = c4_test_box();
        let config = NodeConfig {
            tracker_nft_id: Some(
                "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
            ),
            ..Default::default()
        };
        let state = c4_test_state(config);
        state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .expect("reserve bound to the configured tracker NFT should parse");
    }

    #[test]
    fn test_parse_reserve_box_rejects_invalid_pubkey() {
        let mut scan_box = c4_test_box();
        scan_box.additional_registers.insert(
            "R4".to_string(),
            "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4".to_string(),
        );
        let state = c4_test_state(NodeConfig::default());
        let err = state
            .parse_reserve_box(&scan_box, ReserveContractKind::Erg, "test_p2s")
            .unwrap_err();
        assert!(
            err.to_string().contains("Invalid owner pubkey"),
            "unexpected error: {}",
            err
        );
    }

    /// Spawn a mock node that serves `boxes_body` for box-list queries and
    /// answers `GET /blockchain/transaction/byId/{txId}` from `txs`
    /// (404 when the transaction is unknown).
    async fn spawn_mock_node(
        boxes_body: String,
        txs: std::sync::Arc<HashMap<String, String>>,
    ) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let boxes_body = boxes_body.clone();
                let txs = txs.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let request_line = request.lines().next().unwrap_or_default();
                    let path = request_line.split_whitespace().nth(1).unwrap_or_default();
                    let (status, body) = match path.strip_prefix("/blockchain/transaction/byId/") {
                        Some(tx_id) => match txs.get(tx_id) {
                            Some(body) => ("200 OK", body.clone()),
                            None => ("404 Not Found", "{}".to_string()),
                        },
                        None => ("200 OK", boxes_body),
                    };
                    let response = format!(
                        "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        status,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        addr
    }

    fn mock_state(addr: std::net::SocketAddr) -> ServerState {
        c4_test_state(NodeConfig {
            node_url: format!("http://{}", addr),
            reserve_contract_p2s: Some("test_p2s".to_string()),
            ..Default::default()
        })
    }

    /// Insert a tracked reserve (in-memory + persisted), ERG denomination.
    fn insert_tracked_reserve(state: &ServerState, box_id: &str, owner_pubkey: &str) {
        let info = ExtendedReserveInfo {
            base_info: crate::ReserveInfo {
                collateral_amount: 1_000_000_000,
                last_updated_height: 0,
                contract_address: "test_p2s".to_string(),
                tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304"
                    .to_string(),
                refund_initiation_height: 0,
                reserve_token_id: String::new(),
            },
            total_debt: 0,
            box_id: normalized_box_id(box_id),
            owner_pubkey: owner_pubkey.to_string(),
            last_updated_timestamp: 0,
        };
        state.reserve_tracker.update_reserve(info.clone()).unwrap();
        state.reserve_storage.store_reserve(&info).unwrap();
    }

    fn tx_json(tx_id: &str, inputs: &[(&str, &str, &str, &str)]) -> String {
        let inputs_json = inputs
            .iter()
            .map(|(box_id, address, creating_tx, ergo_tree)| {
                format!(
                    "{{\"boxId\":\"{}\",\"address\":\"{}\",\"transactionId\":\"{}\",\"ergoTree\":\"{}\"}}",
                    box_id, address, creating_tx, ergo_tree
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"id\":\"{}\",\"inputs\":[{}],\"outputs\":[]}}",
            tx_id, inputs_json
        )
    }

    #[tokio::test]
    async fn test_admission_quarantines_duplicate_dust_box() {
        // Issue #19: an attacker creates a dust box at the reserve contract
        // with the victim's key in R4 while the victim's reserve is unspent.
        // The box is C4-clean, so only the lineage check can reject it.
        let tracked_box = "11".repeat(32);
        let attacker_box = "22".repeat(32);
        let attacker_tx = "aa".repeat(32);

        // The attacker's creating transaction does NOT spend the tracked box.
        let txs = HashMap::from([(
            attacker_tx.clone(),
            tx_json(
                &attacker_tx,
                &[(
                    &"33".repeat(32),
                    "some_other_address",
                    &"44".repeat(32),
                    "00",
                )],
            ),
        )]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);

        let owner = &VALID_PREFIXED_PUBKEY[2..];
        insert_tracked_reserve(&state, &tracked_box, owner);

        let mut scan_box = c4_test_box();
        scan_box.box_id = attacker_box.clone();
        scan_box.transaction_id = attacker_tx.clone();
        scan_box.value = 1_000_000; // dust

        let tagged = TaggedScanBox {
            scan_box,
            kind: ReserveContractKind::Erg,
            contract_address: "test_p2s".to_string(),
        };
        assert_eq!(
            state.ingest_scan_box(&tagged).await,
            ScanBoxIngest::Rejected(attacker_box.clone()),
            "the dust duplicate must be quarantined"
        );

        // The quarantine record is persisted with the reason...
        let record = state
            .reserve_storage
            .get_quarantined(&attacker_box)
            .unwrap()
            .expect("quarantine record persisted");
        assert!(
            record.reason.contains("does not spend the tracked box"),
            "unexpected reason: {}",
            record.reason
        );

        // ...and the victim's reserve still resolves unambiguously.
        let resolved = state
            .reserve_tracker
            .resolve_issuer_reserve(owner, &crate::DenomKey::Erg);
        assert!(
            resolved.is_ok(),
            "issuer must not be blocked: {:?}",
            resolved
        );
        assert_eq!(resolved.unwrap().box_id, tracked_box);

        // Once quarantined, always quarantined (no re-evaluation).
        assert_eq!(
            state.ingest_scan_box(&tagged).await,
            ScanBoxIngest::Rejected(attacker_box)
        );
    }

    #[tokio::test]
    async fn test_admission_admits_lineage_successor() {
        // A top-up/redemption successor: its creating transaction spends the
        // tracked box, and the contract guarantees register continuity.
        let tracked_box = "11".repeat(32);
        let successor_box = "55".repeat(32);
        let successor_tx = "bb".repeat(32);

        let txs = HashMap::from([(
            successor_tx.clone(),
            tx_json(
                &successor_tx,
                &[(&tracked_box, "test_p2s", &"cc".repeat(32), "00")],
            ),
        )]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);

        let owner = &VALID_PREFIXED_PUBKEY[2..];
        insert_tracked_reserve(&state, &tracked_box, owner);

        let mut scan_box = c4_test_box();
        scan_box.box_id = successor_box.clone();
        scan_box.transaction_id = successor_tx.clone();
        // A redemption successor legitimately has a non-empty R5 tree.
        scan_box.additional_registers.insert(
            "R5".to_string(),
            "64aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa032000"
                .to_string(),
        );

        let tagged = TaggedScanBox {
            scan_box,
            kind: ReserveContractKind::Erg,
            contract_address: "test_p2s".to_string(),
        };
        assert_eq!(
            state.ingest_scan_box(&tagged).await,
            ScanBoxIngest::Tracked(successor_box)
        );
    }

    #[tokio::test]
    async fn test_admission_admits_successor_across_missed_generations() {
        // The tracker was down while the reserve moved tracked -> mid -> tip;
        // the lineage walk must follow the reserve-contract inputs backwards.
        let tracked_box = "11".repeat(32);
        let mid_box = "66".repeat(32);
        let tip_box = "77".repeat(32);
        let tip_tx = "dd".repeat(32);
        let mid_tx = "ee".repeat(32);

        let txs = HashMap::from([
            (
                tip_tx.clone(),
                tx_json(&tip_tx, &[(&mid_box, "test_p2s", &mid_tx, "00")]),
            ),
            (
                mid_tx.clone(),
                tx_json(
                    &mid_tx,
                    &[(&tracked_box, "test_p2s", &"ff".repeat(32), "00")],
                ),
            ),
        ]);
        let state = mock_state(spawn_mock_node("[]".to_string(), std::sync::Arc::new(txs)).await);

        let owner = &VALID_PREFIXED_PUBKEY[2..];
        insert_tracked_reserve(&state, &tracked_box, owner);

        let mut scan_box = c4_test_box();
        scan_box.box_id = tip_box.clone();
        scan_box.transaction_id = tip_tx.clone();

        let tagged = TaggedScanBox {
            scan_box,
            kind: ReserveContractKind::Erg,
            contract_address: "test_p2s".to_string(),
        };
        assert_eq!(
            state.ingest_scan_box(&tagged).await,
            ScanBoxIngest::Tracked(tip_box)
        );
    }

    #[tokio::test]
    async fn test_process_scan_boxes_keeps_tracked_reserve_on_lineage_retry() {
        // The scan returns a would-be successor plus an unrelated valid box
        // (so the removal loop runs). The successor's tx lookup fails (404),
        // making it Pending: the tracked box must NOT be removed as spent.
        let tracked_box = "11".repeat(32);
        let successor_box = "55".repeat(32);
        let successor_tx = "bb".repeat(32);
        let other_box = "99".repeat(32);
        let other_tx = "12".repeat(32);
        let other_owner = hex::encode(basis_core::impls::generate_keypair().1);
        let nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let empty_tree = empty_reserve_tree_register();

        let boxes_body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"{}\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"{}\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                ",",
                "{{\"boxId\":\"{}\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"{}\",\"additionalRegisters\":{{\"R4\":\"07{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                "]"
            ),
            successor_box, successor_tx, VALID_PREFIXED_PUBKEY, empty_tree, nft_id,
            other_box, other_tx, other_owner, empty_tree, nft_id
        );

        // The successor's creating tx is unknown (404 -> lineage Retry), while
        // the unrelated root's tx carries its owner's P2PK input.
        let other_tree = p2pk_ergo_tree(&other_owner);
        let txs = HashMap::from([(
            other_tx.clone(),
            tx_json(
                &other_tx,
                &[(
                    &"33".repeat(32),
                    "some_address",
                    &"44".repeat(32),
                    &other_tree,
                )],
            ),
        )]);
        let state = mock_state(spawn_mock_node(boxes_body, std::sync::Arc::new(txs)).await);
        let owner = &VALID_PREFIXED_PUBKEY[2..];
        insert_tracked_reserve(&state, &tracked_box, owner);

        state.process_scan_boxes().await.unwrap();

        let remaining: Vec<String> = state
            .reserve_tracker
            .get_all_reserves()
            .into_iter()
            .map(|r| r.box_id)
            .collect();
        assert!(
            remaining.contains(&tracked_box),
            "tracked reserve must be kept while the successor lineage check is pending: {:?}",
            remaining
        );
        assert!(
            !remaining.contains(&successor_box),
            "unverified successor must not be tracked: {:?}",
            remaining
        );
        assert!(
            remaining.contains(&other_box),
            "unrelated root must be admitted: {:?}",
            remaining
        );
    }

    #[tokio::test]
    async fn test_process_scan_boxes_quarantines_duplicate_and_keeps_issuer_working() {
        // The issue #19 scenario end to end: the scan returns the victim's own
        // reserve plus the attacker's C4-clean dust duplicate in one cycle.
        let tracked_box = "11".repeat(32);
        let attacker_box = "22".repeat(32);
        let attacker_tx = "aa".repeat(32);
        let nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let empty_tree = empty_reserve_tree_register();

        let boxes_body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"{}\",\"value\":1000000000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"{}\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                ",",
                "{{\"boxId\":\"{}\",\"value\":1000000,\"ergoTree\":\"00\",\"creationHeight\":200,",
                "\"transactionId\":\"{}\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R5\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                "]"
            ),
            tracked_box, "00".repeat(32), VALID_PREFIXED_PUBKEY, empty_tree, nft_id,
            attacker_box, attacker_tx, VALID_PREFIXED_PUBKEY, empty_tree, nft_id
        );
        let txs = HashMap::from([(
            attacker_tx.clone(),
            tx_json(
                &attacker_tx,
                &[(
                    &"33".repeat(32),
                    "some_other_address",
                    &"44".repeat(32),
                    "00",
                )],
            ),
        )]);
        let state = mock_state(spawn_mock_node(boxes_body, std::sync::Arc::new(txs)).await);
        let owner = &VALID_PREFIXED_PUBKEY[2..];
        insert_tracked_reserve(&state, &tracked_box, owner);

        state.process_scan_boxes().await.unwrap();

        // The attacker box is quarantined...
        assert!(state
            .reserve_storage
            .get_quarantined(&attacker_box)
            .unwrap()
            .is_some());
        // ...and the issuer's single reserve still resolves.
        let resolved = state
            .reserve_tracker
            .resolve_issuer_reserve(owner, &crate::DenomKey::Erg)
            .expect("issuer must keep working");
        assert_eq!(resolved.box_id, tracked_box);
    }
}
