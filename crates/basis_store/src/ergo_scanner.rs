//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides modern blockchain integration using /scan and /blockchain APIs
//! Adopted from chaincash-rs scanner implementation, modified for reserves-only scanning

use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use ergo_lib::ergotree_ir::address::AddressEncoder;
use ergo_lib::ergotree_ir::address::NetworkPrefix;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::Client;



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
struct ServerStateInner {
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
    pub scan_id: Option<i32>,
    pub last_scan_verification: Option<std::time::SystemTime>,
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
    pub fn new(config: NodeConfig) -> Result<Self, ScannerError> {
        let start_height = config.start_height.unwrap_or(0);
        let client = Client::new();

        // Log which Ergo node is being used (INFO level)
        info!("Initializing Ergo scanner with node: {}", config.node_url);
        if let Some(api_key) = &config.api_key {
            info!("Using API key: {}", api_key);
        } else {
            warn!("No API key configured for Ergo node");
        }

        // Open scanner metadata storage - create directory if it doesn't exist
        let storage_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/basis_server/data/scanner_metadata");

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
        let reserve_storage_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/basis_server/data/reserves");

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
            scan_id: None,
            last_scan_verification: None,
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

    /// Get current blockchain height from Ergo node
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
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

        let info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;

        info["fullHeight"].as_u64().ok_or_else(|| {
            ScannerError::NodeError("Failed to parse fullHeight from node info".to_string())
        })
    }

    /// Get unspent reserve boxes
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        // This would use the scan API to get actual reserve boxes
        // For now, return empty vector as placeholder
        Ok(vec![])
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

        // Update inner state
        {
            let mut inner = self.inner.lock().await;
            inner.scan_active = true;
        }

        if let Some(reserve_contract_p2s) = &self.config.reserve_contract_p2s {
            info!("Using reserve contract P2S: {}", reserve_contract_p2s);
            // Register the scan for reserves
            self.register_reserve_scan().await?;
        } else {
            warn!("No reserve contract P2S specified, using polling mode");
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



    /// Register reserve scan with Ergo node
    pub async fn register_reserve_scan(&mut self) -> Result<(), ScannerError> {
        let reserve_contract_p2s =
            self.config.reserve_contract_p2s.as_ref().ok_or_else(|| {
                ScannerError::Generic("Reserve contract P2S not configured".to_string())
            })?;

        let scan_name = self
            .config
            .scan_name
            .as_deref()
            .unwrap_or("Basis Reserve Scanner");

        // Check if scan ID already exists in database
        debug!(
            "Checking for existing scan ID in database for scan name: '{}'",
            scan_name
        );
        match self.metadata_storage.get_scan_id(scan_name) {
            Ok(Some(stored_scan_id)) => {
                info!("Found existing scan ID in database: {}", stored_scan_id);

                // Verify the scan still exists on the node (only every 4 hours)
                let should_verify = self.should_verify_scan().await;
                if should_verify {
                    debug!("Verifying scan ID {} exists on Ergo node", stored_scan_id);
                    match self.verify_scan_exists(stored_scan_id).await {
                        Ok(true) => {
                            info!("Using existing scan ID: {}", stored_scan_id);
                            // Update verification timestamp
                            self.update_scan_verification_time().await;
                            // Update inner state with the validated scan ID
                            {
                                let mut inner = self.inner.lock().await;
                                inner.scan_id = Some(stored_scan_id);
                            }
                            return Ok(());
                        }
                        Ok(false) => {
                            warn!(
                                "Stored scan ID {} no longer exists on node, re-registering",
                                stored_scan_id
                            );
                            self.metadata_storage
                                .remove_scan_id(scan_name)
                                .map_err(|e| {
                                    ScannerError::StoreError(format!(
                                        "Failed to remove invalid scan ID: {:?}",
                                        e
                                    ))
                                })?;
                            info!(
                                "Removed invalid scan ID {} from database, proceeding to registration",
                                stored_scan_id
                            );
                        }
                        Err(e) => {
                            // If verification fails due to scan list endpoint being unavailable (400/404),
                            // or JSON parsing errors, assume the scan still exists and continue using it
                            if e.to_string().contains("400") || e.to_string().contains("404") || e.to_string().contains("Failed to parse scan list") {
                                warn!("Scan list endpoint unavailable or JSON parsing failed ({}), assuming scan ID {} still exists", e, stored_scan_id);
                                // Update verification timestamp
                                self.update_scan_verification_time().await;
                                // Update inner state with the existing scan ID
                                {
                                    let mut inner = self.inner.lock().await;
                                    inner.scan_id = Some(stored_scan_id);
                                }
                                return Ok(());
                            } else {
                                error!(
                                    "Failed to verify existing scan ID {}: {}",
                                    stored_scan_id, e
                                );
                                warn!("Unable to verify scan ID, forcing re-registration");
                                self.metadata_storage
                                    .remove_scan_id(scan_name)
                                    .map_err(|e| {
                                        ScannerError::StoreError(format!(
                                            "Failed to remove scan ID: {:?}",
                                            e
                                        ))
                                    })?;
                                info!("Forcing scan registration due to verification failure");
                            }
                        }
                    }
                } else {
                    debug!("Skipping scan ID verification (last verified less than 4 hours ago)");
                    // Update inner state with the existing scan ID without verification
                    {
                        let mut inner = self.inner.lock().await;
                        inner.scan_id = Some(stored_scan_id);
                    }
                    return Ok(());
                }
            }
            Ok(None) => {
                info!("No existing scan ID found in database for scan name: '{}', proceeding with new registration", scan_name);
            }
            Err(e) => {
                error!("Failed to get scan ID from database: {:?}", e);
                info!("Database error, proceeding with new registration");
            }
        }

        // Create the ErgoTree and serialize it right before use (don't hold it across await)
        let serialized_contract_bytes = {
            let tree: ErgoTree = AddressEncoder::new(NetworkPrefix::Mainnet)
                .parse_address_from_str(reserve_contract_p2s)
                .unwrap()
                .script()
                .unwrap();
            tree.sigma_serialize_bytes()
        };

        let contract_bytes_hex = hex::encode(&serialized_contract_bytes);

        // Register new scan
        let scan_payload = serde_json::json!({
            "scanName": scan_name,
            "walletInteraction": "off",
            "trackingRule": {
                "predicate": "contains",
                "register": "R1",
                "value": contract_bytes_hex
            },
            "removeOffchain": false
        });

        // Log scan registration (INFO level) and JSON payload (DEBUG level)
        info!("Registering new reserve scan with name: {}", scan_name);
        debug!("Reserve scan registration JSON payload: {}", scan_payload);

        let url = format!("{}/scan/register", self.config.node_url);

        // Create request builder and log request details
        let request_builder = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&scan_payload);

        // Log exact HTTP request details
        info!("Sending HTTP POST request to Ergo node: {}", url);
        info!("Request headers: API key present: {}", self.config.api_key.is_some());
        info!("Request body (JSON): {}", scan_payload);
        debug!("Sending scan registration request to: {}", url);
        debug!(
            "Request headers include: {}",
            if self.config.api_key.is_some() {
                "API key"
            } else {
                "NO API key"
            }
        );

        let response = request_builder.send().await.map_err(|e| {
            error!("HTTP request failed: {}", e);
            error!(
                "Request details - URL: {}, Method: POST, Headers: API key present: {}",
                url,
                self.config.api_key.is_some()
            );
            ScannerError::HttpError(format!("Failed to register scan: {}", e))
        })?;

        // Log response details
        let status = response.status();
        debug!("Response status: {}", status);
        debug!("Response headers: {:?}", response.headers());

        if !status.is_success() {
            // Try to read response body for more details
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            error!(
                "Scan registration failed with status: {}. Response body: {}",
                status, response_text
            );
            error!("Full request details:");
            error!("  URL: {}", url);
            error!("  Method: POST");
            error!("  API key present: {}", self.config.api_key.is_some());
            error!("  Payload: {}", scan_payload);
            error!("  Response status: {}", status);
            error!("  Response body: {}", response_text);
            return Err(ScannerError::NodeError(format!(
                "Scan registration failed with status: {}. Response: {}",
                status, response_text
            )));
        }

        let result: serde_json::Value = response.json().await.map_err(|e| {
            error!("Failed to parse scan registration response JSON: {}", e);
            ScannerError::JsonError(format!("Failed to parse scan registration response: {}", e))
        })?;

        debug!(
            "Scan registration successful response: {}",
            serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Unable to format JSON".to_string())
        );

        let scan_id = result["scanId"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok());

        // Store scan ID in database and update inner state
        if let Some(scan_id) = scan_id {
            self.metadata_storage
                .store_scan_id(scan_name, scan_id)
                .map_err(|e| {
                    ScannerError::StoreError(format!("Failed to store scan ID: {:?}", e))
                })?;

            // Update inner state with the new scan ID
            {
                let mut inner = self.inner.lock().await;
                inner.scan_id = Some(scan_id);
            }

            info!("Registered and stored reserve scan with ID: {}", scan_id);
        } else {
            error!(
                "Failed to get scan ID from registration response. Response was: {}",
                result
            );
            return Err(ScannerError::Generic(
                "Failed to get scan ID from registration response".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if scan verification is needed (every 4 hours)
    async fn should_verify_scan(&self) -> bool {
        let inner = self.inner.lock().await;
        match inner.last_scan_verification {
            Some(last_verification) => {
                let now = std::time::SystemTime::now();
                let duration_since_last = now.duration_since(last_verification).unwrap_or_default();
                duration_since_last >= std::time::Duration::from_secs(4 * 60 * 60) // 4 hours
            }
            None => true, // Never verified before
        }
    }

    /// Update the last scan verification timestamp
    async fn update_scan_verification_time(&self) {
        let mut inner = self.inner.lock().await;
        inner.last_scan_verification = Some(std::time::SystemTime::now());
    }

    /// Verify that a scan ID still exists on the Ergo node
    pub async fn verify_scan_exists(&self, scan_id: i32) -> Result<bool, ScannerError> {
        let url = format!("{}/scan/listAll", self.config.node_url);
        debug!("Verifying scan exists - URL: {}", url);
        debug!("Looking for scan ID: {}", scan_id);
        info!("Sending HTTP GET request to Ergo node: {}", url);
        info!("Looking for scan ID: {}", scan_id);

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to send scan list request: {}", e);
                // If we can't even connect, assume the scan exists to avoid re-registration
                warn!(
                    "Network error connecting to scan list endpoint, assuming scan ID {} exists",
                    scan_id
                );
                return Ok(true);
            }
        };

        let status = response.status();
        debug!("Scan list response status: {}", status);

        // If scan list endpoint is not available (400/404), assume scan exists
        // This handles nodes that don't support scan listing
        if status == 400 || status == 404 {
            info!(
                "Scan list endpoint not available (status: {}), assuming scan ID {} exists",
                status, scan_id
            );
            return Ok(true);
        }

        if !status.is_success() {
            // Try to read response body for more details
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            // Log as warning instead of error since we're handling this gracefully
            warn!(
                "Scan list request failed with status: {}. Response body: {}",
                status, response_text
            );
            // For any other non-success status, assume scan exists to prevent re-registration
            warn!("Scan list request failed (status: {}), assuming scan ID {} exists to prevent re-registration", status, scan_id);
            return Ok(true);
        }

        let scans: serde_json::Value = response.json().await.map_err(|e| {
            error!("Failed to parse scan list JSON: {}", e);
            ScannerError::JsonError(format!("Failed to parse scan list: {}", e))
        })?;

        debug!(
            "Scan list response: {}",
            serde_json::to_string_pretty(&scans)
                .unwrap_or_else(|_| "Unable to format JSON".to_string())
        );

        // Check if our scan ID exists in the list
        if let Some(scans_array) = scans.as_array() {
            debug!("Found {} scans in list", scans_array.len());
            for scan in scans_array {
                if let Some(id) = scan["scanId"].as_i64() {
                    debug!("Checking scan ID: {} against target: {}", id, scan_id);
                    if id == scan_id as i64 {
                        debug!("Scan ID {} found in scan list", scan_id);
                        return Ok(true);
                    }
                } else {
                    debug!("Scan entry missing scanId: {:?}", scan);
                }
            }
        } else {
            debug!("Scan list is not an array or is empty");
        }

        debug!("Scan ID {} not found in scan list", scan_id);
        Ok(false)
    }

    /// Get unspent boxes from registered scan
    pub async fn get_scan_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        let scan_id = {
            let inner = self.inner.lock().await;
            inner.scan_id
        };

        let scan_id =
            scan_id.ok_or_else(|| ScannerError::Generic("Scan not registered".to_string()))?;

        let url = format!("{}/scan/unspentBoxes/{}", self.config.node_url, scan_id);

        info!("Sending HTTP GET request to Ergo node: {}", url);
        info!("Requesting unspent boxes for scan ID: {}", scan_id);

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to fetch scan boxes: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!(
                "Failed to get scan boxes with status: {}",
                response.status()
            )));
        }

        let boxes: Vec<ScanBox> = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse scan boxes: {}", e)))?;

        Ok(boxes)
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
        let owner_pubkey = scan_box
            .additional_registers
            .get("R4")
            .ok_or_else(|| {
                ScannerError::InvalidReserveBox(format!("Missing R4 register in box {}", box_id))
            })?
            .clone();

        // Extract tracker NFT from R5 register (optional)
        let tracker_nft_id = scan_box.additional_registers.get("R5").map(|s| s.clone());

        // Create extended reserve info
        let reserve_info = ExtendedReserveInfo::new(
            box_id.as_bytes(),
            owner_pubkey.as_bytes(),
            value,
            tracker_nft_id.as_deref().map(|s| s.as_bytes()),
            creation_height,
        );

        Ok(reserve_info)
    }

    /// Process scan boxes and update reserve tracker
    pub async fn process_scan_boxes(&self) -> Result<(), ScannerError> {
        let scan_boxes = self.get_scan_boxes().await?;
        let mut current_box_ids = Vec::new();

        for scan_box in &scan_boxes {
            match self.parse_reserve_box(scan_box) {
                Ok(reserve_info) => {
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
                    warn!("Failed to parse reserve box {}: {}", scan_box.box_id, e);
                }
            }
        }

        // Remove reserves that are no longer in the scan
        let all_reserves = self.reserve_tracker.get_all_reserves();
        for reserve in all_reserves {
            if !current_box_ids.contains(&reserve.box_id) {
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

        Ok(())
    }

    /// Get the reserve contract P2S for scan registration
    async fn get_reserve_contract_p2s(&self) -> Result<String, ScannerError> {
        self.config.reserve_contract_p2s.clone().ok_or_else(|| {
            ScannerError::Generic("No reserve contract P2S configured in node config".to_string())
        })
    }
}

/// Start the scanner in background
pub async fn start_scanner(state: ServerState) -> Result<(), ScannerError> {
    let state = Arc::new(state);
    tokio::spawn(reserve_scanner_loop(state.clone()));
    Ok(())
}

/// Create a scanner with default configuration
pub fn create_default_scanner() -> Result<ServerState, ScannerError> {
    let config = NodeConfig::default();
    ServerState::new(config)
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
            api_key: Some("hello".to_string()),
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
        match state.get_current_height().await {
            Ok(height) => {
                // Check if we have a valid scan ID before processing
                let has_valid_scan = {
                    let inner = state.inner.lock().await;
                    inner.scan_id.is_some() && inner.scan_active
                };

                if !has_valid_scan {
                    warn!("Scanner has no valid scan ID, attempting to register scan...");
                    let mut state_mut = state.as_ref().clone();
                    match state_mut.register_reserve_scan().await {
                        Ok(()) => {
                            info!("Scan registration successful, resuming normal operation");
                            consecutive_failures = 0;
                        }
                        Err(e) => {
                            error!("Failed to register scan: {}", e);
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
                } else {
                    // Process scan boxes if we have a valid scan
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

                                // If we get "scan not registered" error, reset scan state
                                if e.to_string().contains("Scan not registered") {
                                    warn!("Scan registration lost, resetting scan state");
                                    {
                                        let mut inner = state.inner.lock().await;
                                        inner.scan_id = None;
                                    }
                                }

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
