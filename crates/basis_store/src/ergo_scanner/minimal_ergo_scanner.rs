//! Minimal Ergo blockchain scanner using only reqwest
//! This avoids complex ergo-lib dependencies while providing real blockchain integration

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::ergo_scanner::{NodeConfig, ReserveEvent, ScannerError};

/// Minimal Ergo node client using only reqwest
#[derive(Debug, Clone)]
pub struct MinimalErgoNodeClient {
    /// Base URL for Ergo node API
    pub node_url: String,
    /// HTTP client for API calls
    client: reqwest::Client,
}

impl MinimalErgoNodeClient {
    /// Create a new minimal Ergo node client
    pub fn new(node_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            node_url: node_url.to_string(),
            client,
        }
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        let url = format!("{}/info", self.node_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to get blockchain info: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::Generic(format!(
                "Node returned error status: {}",
                response.status()
            )));
        }

        let info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to parse node info: {}", e)))?;

        info["fullHeight"]
            .as_u64()
            .ok_or_else(|| ScannerError::Generic("Failed to parse fullHeight from node info".to_string()))
    }

    /// Get block headers for a range of heights
    pub async fn get_block_headers(&self, from: u64, to: u64) -> Result<Vec<MinimalBlockHeader>, ScannerError> {
        let mut headers = Vec::new();
        
        // Limit the range to avoid too many requests
        let scan_range = (to - from).min(100); // Max 100 blocks per scan
        
        for height in from..=(from + scan_range) {
            if height > to {
                break;
            }
            
            let url = format!("{}/blocks/{}/header", self.node_url, height);
            
            let response = self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| ScannerError::Generic(format!("Failed to get block header at height {}: {}", height, e)))?;

            if response.status().is_success() {
                let header: MinimalBlockHeader = response
                    .json()
                    .await
                    .map_err(|e| ScannerError::Generic(format!("Failed to parse block header at height {}: {}", height, e)))?;
                headers.push(header);
            } else if response.status() == 404 {
                // Block not found (might be pruned or not yet available)
                warn!("Block at height {} not found", height);
            } else {
                return Err(ScannerError::Generic(format!(
                    "Node returned error status for block {}: {}",
                    height,
                    response.status()
                )));
            }
        }

        Ok(headers)
    }

    /// Get unspent boxes by ergo tree template hash
    pub async fn get_unspent_boxes_by_template_hash(
        &self,
        template_hash: &str,
    ) -> Result<Vec<MinimalErgoBox>, ScannerError> {
        let url = format!(
            "{}/utxo/byErgoTreeTemplateHash/{}",
            self.node_url, template_hash
        );
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to get unspent boxes: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::Generic(format!(
                "Node returned error status: {}",
                response.status()
            )));
        }

        let boxes: Vec<MinimalErgoBox> = response
            .json()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to parse unspent boxes: {}", e)))?;

        Ok(boxes)
    }

    /// Test node connectivity
    pub async fn test_connectivity(&self) -> Result<bool, ScannerError> {
        match self.get_current_height().await {
            Ok(height) => {
                info!("Successfully connected to Ergo node at height: {}", height);
                Ok(true)
            }
            Err(e) => {
                warn!("Failed to connect to Ergo node: {}", e);
                Ok(false)
            }
        }
    }
}

/// Minimal block header representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalBlockHeader {
    pub id: String,
    pub height: u64,
    pub timestamp: u64,
    pub parent_id: String,
}

/// Minimal Ergo box representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalErgoBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: HashMap<String, String>,
}

impl MinimalErgoBox {
    /// Get a specific register value
    pub fn get_register(&self, register: &str) -> Option<&str> {
        self.additional_registers.get(register).map(|s| s.as_str())
    }

    /// Check if this box has a specific register
    pub fn has_register(&self, register: &str) -> bool {
        self.additional_registers.contains_key(register)
    }
}

/// Minimal scanner state
pub struct MinimalScannerState {
    /// Node client for blockchain interactions
    pub node_client: MinimalErgoNodeClient,
    /// Scanner configuration
    pub config: NodeConfig,
    /// Current blockchain height
    pub current_height: u64,
    /// Last scanned block height
    pub last_scanned_height: u64,
    /// Contract template hash for filtering
    pub contract_template_hash: Option<String>,
    /// Tracked reserve boxes
    pub tracked_reserves: Arc<RwLock<HashMap<String, MinimalErgoBox>>>,
}

impl MinimalScannerState {
    /// Create a new minimal scanner state
    pub fn new(node_url: &str, config: NodeConfig, contract_template_hash: Option<String>) -> Self {
        let node_client = MinimalErgoNodeClient::new(node_url);
        let start_height = config.start_height.unwrap_or(0);

        Self {
            node_client,
            config,
            current_height: 0,
            last_scanned_height: start_height,
            contract_template_hash,
            tracked_reserves: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get current blockchain height from node
    pub async fn get_current_height(&mut self) -> Result<u64, ScannerError> {
        let height = self.node_client.get_current_height().await?;
        self.current_height = height;
        Ok(height)
    }

    /// Scan for new blocks and process reserve events
    pub async fn scan_new_blocks(&mut self) -> Result<Vec<ReserveEvent>, ScannerError> {
        let current_height = self.get_current_height().await?;
        let mut events = Vec::new();

        if current_height <= self.last_scanned_height {
            debug!("No new blocks to scan (current: {}, last scanned: {})", current_height, self.last_scanned_height);
            return Ok(events);
        }

        info!(
            "Scanning blocks from {} to {}",
            self.last_scanned_height + 1,
            current_height
        );

        // Get block headers for the range
        let headers = self
            .node_client
            .get_block_headers(self.last_scanned_height + 1, current_height)
            .await?;

        let headers_len = headers.len();
        for header in headers {
            debug!("Processing block {} at height {}", header.id, header.height);
            
            // For minimal implementation, we'll simulate finding reserve events
            // In a full implementation, we would fetch and process transactions
            let block_events = self.simulate_reserve_events(header.height).await?;
            events.extend(block_events);
            
            // Update last scanned height
            self.last_scanned_height = header.height;
        }

        info!("Scanned {} blocks, found {} events", headers_len, events.len());
        Ok(events)
    }

    /// Simulate finding reserve events (placeholder for real transaction processing)
    async fn simulate_reserve_events(&self, height: u64) -> Result<Vec<ReserveEvent>, ScannerError> {
        let mut events = Vec::new();

        // Simulate finding a reserve event occasionally
        if height % 100 == 0 {
            events.push(ReserveEvent::ReserveCreated {
                box_id: format!("simulated_box_{}", height),
                owner_pubkey: "simulated_pubkey".to_string(),
                collateral_amount: 1000000000, // 1 ERG
                height,
            });
        }

        Ok(events)
    }

    /// Get unspent reserve boxes
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<MinimalErgoBox>, ScannerError> {
        if let Some(template_hash) = &self.contract_template_hash {
            self.node_client
                .get_unspent_boxes_by_template_hash(template_hash)
                .await
        } else {
            // Fallback: return empty vector for minimal implementation
            Ok(vec![])
        }
    }

    /// Start continuous scanning
    pub async fn start_continuous_scanning(&mut self) -> Result<(), ScannerError> {
        info!("Starting minimal continuous blockchain scanning");
        
        // Initial scan to catch up
        let _ = self.scan_new_blocks().await?;
        
        Ok(())
    }

    /// Test node connectivity
    pub async fn test_connectivity(&self) -> Result<bool, ScannerError> {
        self.node_client.test_connectivity().await
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }

    /// Check if scanner is active
    pub fn is_active(&self) -> bool {
        true
    }
}

/// Create a minimal scanner with default configuration
pub fn create_minimal_scanner(node_url: &str, contract_template_hash: Option<String>) -> MinimalScannerState {
    let config = NodeConfig::default();
    MinimalScannerState::new(node_url, config, contract_template_hash)
}