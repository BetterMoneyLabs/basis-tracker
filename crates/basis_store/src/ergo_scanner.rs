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
    persistence::{ReserveStorage, ScannerMetadataStorage},
    ExtendedReserveInfo, ReserveTracker,
};

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

/// Configuration for scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Basis reserve contract P2S address (optional)
    pub reserve_contract_p2s: Option<String>,
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

        // Add API key header if configured
        if let Some(api_key) = &self.config.api_key {
            debug!("Using API key '{}' for request to: {}", api_key, url);
            info!("Adding HTTP header: api_key: {}", api_key);
            request = request.header("api_key", api_key);
        } else {
            debug!("No API key configured for request to: {}", url);
            info!("No API key header added to HTTP request");
        }

        request
    }

    /// Create a server state that uses real Ergo scanner
    pub fn new(config: NodeConfig, data_dir: impl AsRef<Path>) -> Result<Self, ScannerError> {
        let start_height = config.start_height.unwrap_or(0);
        let client = crate::http::bounded_client();
        let data_dir = data_dir.as_ref();

        // Log which Ergo node is being used (INFO level)
        info!("Initializing Ergo scanner with node: {}", config.node_url);
        if let Some(api_key) = &config.api_key {
            info!("Using API key: {}", api_key);
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

    /// Get unspent reserve boxes via `POST /blockchain/box/unspent/byAddress`.
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        let reserve_contract_p2s = self.config.reserve_contract_p2s.as_ref().ok_or_else(|| {
            ScannerError::Generic("Reserve contract P2S not configured".to_string())
        })?;

        let url = format!("{}/blockchain/box/unspent/byAddress", self.config.node_url);
        info!("Fetching unspent reserve boxes from: {}", url);

        let response = self
            .request_builder(reqwest::Method::POST, &url)
            .json(reserve_contract_p2s)
            .send()
            .await
            .map_err(|e| {
                ScannerError::HttpError(format!("Failed to fetch reserve boxes: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response".to_string());
            error!(
                "Failed to get reserve boxes with status {}: {}",
                status, response_text
            );
            return Err(ScannerError::NodeError(format!(
                "Failed to get reserve boxes with status: {}",
                status
            )));
        }

        let parsed: ByAddressResponse = crate::http::read_json_capped(response)
            .await
            .map_err(|e| {
                ScannerError::JsonError(format!("Failed to parse reserve boxes response: {}", e))
            })
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    ScannerError::JsonError(format!(
                        "Failed to parse reserve boxes response: {}",
                        e
                    ))
                })
            })?;

        info!("Found {} reserve boxes", parsed.len());
        Ok(parsed.into_iter().map(Into::into).collect())
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

        if self.config.reserve_contract_p2s.is_none() {
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
    pub async fn get_scan_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        self.get_unspent_reserve_boxes().await
    }

    /// Parse reserve box into ExtendedReserveInfo
    pub fn parse_reserve_box(
        &self,
        scan_box: &ScanBox,
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

        // Extract refund initiation height from R7 register (optional; absent = 0)
        let refund_initiation_height =
            decode_ergo_long_register(scan_box.additional_registers.get("R7"));

        let reserve_info = ExtendedReserveInfo::new(
            box_id.as_bytes(),
            &owner_pubkey_bytes,
            value,
            Some(&tracker_nft_id_bytes),
            creation_height,
            refund_initiation_height,
        );

        Ok(reserve_info)
    }

    /// Process scan boxes and update reserve tracker
    pub async fn process_scan_boxes(&self) -> Result<(), ScannerError> {
        info!("Starting to process scan boxes...");
        let scan_boxes = self.get_scan_boxes().await?;
        info!("Retrieved {} scan boxes to process", scan_boxes.len());

        let mut current_box_ids = Vec::new();
        let mut parse_failures = 0usize;

        for scan_box in &scan_boxes {
            debug!(
                "Processing scan box: ID={}, value={}, registers={:?}",
                scan_box.box_id, scan_box.value, scan_box.additional_registers
            );

            match self.parse_reserve_box(scan_box) {
                Ok(reserve_info) => {
                    debug!(
                        "Successfully parsed reserve box: box_id={}, owner={}, collateral={}",
                        reserve_info.box_id,
                        reserve_info.owner_pubkey,
                        reserve_info.base_info.collateral_amount
                    );
                    current_box_ids.push(reserve_info.box_id.clone());

                    // Update in-memory tracker
                    if let Err(e) = self.reserve_tracker.update_reserve(reserve_info.clone()) {
                        warn!("Failed to update reserve {}: {}", scan_box.box_id, e);
                    } else {
                        // Persist to database
                        if let Err(e) = self.reserve_storage.store_reserve(&reserve_info) {
                            warn!(
                                "Failed to persist reserve {} to database: {:?}",
                                scan_box.box_id, e
                            );
                        } else {
                            info!("Updated and persisted reserve: {}", scan_box.box_id);
                        }
                    }
                }
                Err(e) => {
                    parse_failures += 1;
                    warn!(
                        "Failed to parse reserve box {}: {} - registers: {:?}",
                        scan_box.box_id, e, scan_box.additional_registers
                    );
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
        if !current_box_ids.is_empty() && parse_failures == 0 {
            for reserve in all_reserves {
                if !current_box_ids.contains(&reserve.box_id) {
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
        } else if parse_failures > 0 {
            // Fail closed: a snapshot with unparseable boxes is not
            // authoritative. Removing reserves not present in
            // `current_box_ids` could delete a live reserve whose box merely
            // failed to parse (corrupted node response, new register
            // serialization), so the previous state is preserved.
            warn!(
                "Skipping reserve removal: {} of {} scan boxes failed to parse",
                parse_failures,
                scan_boxes.len()
            );
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
            node_url: "http://159.89.116.15:11088".to_string(), // Your Ergo node
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

    #[test]
    fn test_parse_reserve_box_with_r6_register() {
        // Create a mock scan box with a public key that has the 0x07 prefix
        // and a valid 32-byte tracker NFT ID in R6 register
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
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
        let result = server_state.parse_reserve_box(&scan_box);

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
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
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
        let result = server_state.parse_reserve_box(&scan_box);

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
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
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
        let result = server_state.parse_reserve_box(&scan_box);

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
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"valid_box_1\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx1\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                "]"
            ),
            prefixed_pubkey, tracker_nft_id
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let body = body.clone();
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

    async fn test_process_scan_boxes_preserves_reserves_when_boxes_fail_to_parse() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Mock node returning one valid reserve box and one malformed box
        // (missing R4). A pre-existing stored reserve must survive: the
        // malformed box makes the snapshot non-authoritative, so the removal
        // phase must be skipped (fail closed).
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let body = format!(
            concat!(
                "[",
                "{{\"boxId\":\"valid_box_1\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx1\",\"additionalRegisters\":{{\"R4\":\"{}\",\"R6\":\"0e20{}\"}}}}",
                ",",
                "{{\"boxId\":\"bad_box_2\",\"value\":1000,\"ergoTree\":\"00\",\"creationHeight\":100,",
                "\"transactionId\":\"tx2\",\"additionalRegisters\":{{}}}}",
                "]"
            ),
            prefixed_pubkey, tracker_nft_id
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let _ = socket.read(&mut buf).await;
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

        // Pre-existing reserve that is NOT in the scan result; previously it
        // would have been removed because one valid box was parsed.
        let preexisting = ExtendedReserveInfo {
            base_info: crate::ReserveInfo {
                collateral_amount: 500,
                last_updated_height: 0,
                contract_address: "test".to_string(),
                tracker_nft_id: "test".to_string(),
                refund_initiation_height: 0,
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
            remaining.contains(&"preexisting_box".to_string()),
            "pre-existing reserve must be preserved when any scan box fails to parse: {:?}",
            remaining
        );
    }
}
