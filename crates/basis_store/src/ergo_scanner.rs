//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides modern blockchain integration using /scan and /blockchain APIs
//! Adopted from chaincash-rs scanner implementation, modified for reserves-only scanning

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn, error};

use reqwest::Client;

use crate::{ReserveTracker, ExtendedReserveInfo, persistence::ScannerMetadataStorage};

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
    /// Basis reserve contract template hash (optional)
    pub contract_template: Option<String>,
    /// Ergo node URL
    pub node_url: String,
    /// Scan registration name
    pub scan_name: Option<String>,
}

/// Server state for scanner
/// Uses real blockchain integration for production
#[derive(Clone)]
pub struct ServerState {
    pub config: NodeConfig,
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
    client: Client,
    reserve_tracker: ReserveTracker,
    scan_id: Option<i32>,
    metadata_storage: ScannerMetadataStorage,
}

impl ServerState {
    /// Create a server state that uses real Ergo scanner
    pub fn new(config: NodeConfig) -> Result<Self, ScannerError> {
        let start_height = config.start_height.unwrap_or(0);
        let client = Client::new();
        
        // Open scanner metadata storage - create directory if it doesn't exist
        let storage_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/basis_server/data/scanner_metadata");
        
        // Create directory if it doesn't exist
        if let Some(parent) = storage_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ScannerError::StoreError(format!("Failed to create scanner metadata directory: {}", e)))?;
        }
        
        let metadata_storage = ScannerMetadataStorage::open(&storage_path)
            .map_err(|e| ScannerError::StoreError(format!("Failed to open scanner metadata storage: {:?}", e)))?;
        
        Ok(Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
            scan_active: false,
            client,
            reserve_tracker: ReserveTracker::new(),
            scan_id: None,
            metadata_storage,
        })
    }

    /// Get current blockchain height from Ergo node
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        let url = format!("{}/info", self.config.node_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to connect to node: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!("Node returned status: {}", response.status())));
        }
        
        let info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;
        
        info["fullHeight"]
            .as_u64()
            .ok_or_else(|| ScannerError::NodeError("Failed to parse fullHeight from node info".to_string()))
    }

    /// Get unspent reserve boxes
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        // This would use the scan API to get actual reserve boxes
        // For now, return empty vector as placeholder
        Ok(vec![])
    }

    /// Check if scanner is active
    pub fn is_active(&self) -> bool {
        self.scan_active
    }

    /// Start scanning with real blockchain integration
    pub async fn start_scanning(&mut self) -> Result<(), ScannerError> {
        info!("Starting Ergo blockchain scanner for reserves");
        self.scan_active = true;
        
        if let Some(contract_template) = &self.config.contract_template {
            info!("Using reserve contract template: {}", contract_template);
            // Register the scan for reserves
            self.register_reserve_scan().await?;
        } else {
            warn!("No contract template specified, using polling mode");
        }

        Ok(())
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }

    /// Get the reserve tracker
    pub fn reserve_tracker(&self) -> &ReserveTracker {
        &self.reserve_tracker
    }

    /// Register reserve scan with Ergo node
    pub async fn register_reserve_scan(&mut self) -> Result<(), ScannerError> {
        let contract_template = self.config.contract_template.as_ref()
            .ok_or_else(|| ScannerError::Generic("Contract template not configured".to_string()))?;

        let scan_name = self.config.scan_name
            .as_deref()
            .unwrap_or("Basis Reserve Scanner");

        // Check if scan ID already exists in database
        if let Ok(Some(stored_scan_id)) = self.metadata_storage.get_scan_id(scan_name) {
            info!("Found existing scan ID in database: {}", stored_scan_id);
            self.scan_id = Some(stored_scan_id);
            
            // Verify the scan still exists on the node
            if self.verify_scan_exists(stored_scan_id).await? {
                info!("Using existing scan ID: {}", stored_scan_id);
                return Ok(());
            } else {
                warn!("Stored scan ID {} no longer exists on node, re-registering", stored_scan_id);
                self.metadata_storage.remove_scan_id(scan_name)
                    .map_err(|e| ScannerError::StoreError(format!("Failed to remove invalid scan ID: {:?}", e)))?;
            }
        }

        // Register new scan
        let scan_payload = serde_json::json!({
            "scanName": scan_name,
            "trackingRule": {
                "predicate": "contains",
                "register": "R1",
                "value": contract_template
            },
            "removeOffchain": false
        });

        let url = format!("{}/scan/register", self.config.node_url);
        
        let response = self.client
            .post(&url)
            .json(&scan_payload)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to register scan: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!("Scan registration failed with status: {}", response.status())));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse scan registration response: {}", e)))?;

        self.scan_id = result["scanId"].as_i64().and_then(|v| i32::try_from(v).ok());
        
        // Store scan ID in database
        if let Some(scan_id) = self.scan_id {
            self.metadata_storage.store_scan_id(scan_name, scan_id)
                .map_err(|e| ScannerError::StoreError(format!("Failed to store scan ID: {:?}", e)))?;
            info!("Registered and stored reserve scan with ID: {}", scan_id);
        } else {
            return Err(ScannerError::Generic("Failed to get scan ID from registration response".to_string()));
        }
        
        Ok(())
    }

    /// Verify that a scan ID still exists on the Ergo node
    pub async fn verify_scan_exists(&self, scan_id: i32) -> Result<bool, ScannerError> {
        let url = format!("{}/scan/list", self.config.node_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to list scans: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!("Failed to list scans with status: {}", response.status())));
        }

        let scans: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse scan list: {}", e)))?;

        // Check if our scan ID exists in the list
        if let Some(scans_array) = scans.as_array() {
            for scan in scans_array {
                if let Some(id) = scan["scanId"].as_i64() {
                    if id == scan_id as i64 {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Get unspent boxes from registered scan
    pub async fn get_scan_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        let scan_id = self.scan_id
            .ok_or_else(|| ScannerError::Generic("Scan not registered".to_string()))?;

        let url = format!("{}/scan/unspentBoxes/{}", self.config.node_url, scan_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ScannerError::HttpError(format!("Failed to fetch scan boxes: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::NodeError(format!("Failed to get scan boxes with status: {}", response.status())));
        }

        let boxes: Vec<ScanBox> = response
            .json()
            .await
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse scan boxes: {}", e)))?;

        Ok(boxes)
    }

    /// Parse reserve box into ExtendedReserveInfo
    pub fn parse_reserve_box(&self, scan_box: &ScanBox) -> Result<ExtendedReserveInfo, ScannerError> {
        let box_id = scan_box.box_id.clone();
        let value = scan_box.value;
        let creation_height = scan_box.creation_height;
        
        // Extract owner public key from R4 register
        let owner_pubkey = scan_box.additional_registers
            .get("R4")
            .ok_or_else(|| ScannerError::InvalidReserveBox(format!("Missing R4 register in box {}", box_id)))?
            .clone();

        // Extract tracker NFT from R5 register (optional)
        let tracker_nft_id = scan_box.additional_registers
            .get("R5")
            .map(|s| s.clone());

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
                    if let Err(e) = self.reserve_tracker.update_reserve(reserve_info) {
                        warn!("Failed to update reserve {}: {}", scan_box.box_id, e);
                    } else {
                        info!("Updated reserve: {}", scan_box.box_id);
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
                if let Err(e) = self.reserve_tracker.remove_reserve(&reserve.box_id) {
                    warn!("Failed to remove reserve {}: {}", reserve.box_id, e);
                } else {
                    info!("Removed spent reserve: {}", reserve.box_id);
                }
            }
        }

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
            contract_template: None,
            node_url: "http://213.239.193.208:9053".to_string(), // Public Ergo node
            scan_name: Some("Basis Reserve Scanner".to_string()),
        }
    }
}

/// Reserve scanner loop (background task)
pub async fn reserve_scanner_loop(state: Arc<ServerState>) -> Result<(), ScannerError> {
    info!("Starting reserve scanner background loop");
    
    loop {
        // Update current height
        match state.get_current_height().await {
            Ok(height) => {
                // Process scan boxes if we have a new block
                if height > state.last_scanned_height {
                    if let Err(e) = state.process_scan_boxes().await {
                        error!("Failed to process scan boxes: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to get current height: {}", e);
            }
        }

        // Wait before next scan
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
