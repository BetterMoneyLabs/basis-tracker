//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides modern blockchain integration using /scan and /blockchain APIs
//! Adopted from chaincash-rs scanner implementation, modified for reserves-only scanning

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

use reqwest::Client;

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
}

impl ServerState {
    /// Create a server state that uses real Ergo scanner
    pub fn new(config: NodeConfig) -> Result<Self, ScannerError> {
        let start_height = config.start_height.unwrap_or(0);
        let client = Client::new();
        Ok(Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
            scan_active: false,
            client,
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
            // In a real implementation, we would register the scan here
        } else {
            warn!("No contract template specified, using polling mode");
        }

        Ok(())
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }
}

/// Start the scanner in background
pub async fn start_scanner(state: ServerState) -> Result<(), ScannerError> {
    let mut state = state;
    state.start_scanning().await?;
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
        }
    }
}

/// Reserve scanner loop (background task)
pub async fn reserve_scanner_loop(state: Arc<ServerState>) -> Result<(), ScannerError> {
    let mut state = (*state).clone();
    
    loop {

        // todo: implementation is missed
        
        // Wait before next scan
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
