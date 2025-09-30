//! Real Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides real blockchain integration using direct node API calls

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::ergo_scanner::{NodeConfig, ReserveEvent, ScannerError};

/// Real Ergo node client for blockchain interactions
#[derive(Debug, Clone)]
pub struct ErgoNodeClient {
    /// Base URL for Ergo node API
    pub node_url: String,
    /// API key for authenticated nodes (optional)
    pub api_key: Option<String>,
    /// HTTP client for API calls
    client: reqwest::Client,
}

impl ErgoNodeClient {
    /// Create a new Ergo node client
    pub fn new(node_url: &str, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            node_url: node_url.to_string(),
            api_key,
            client,
        }
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        let url = format!("{}/info", self.node_url);
        let mut request = self.client.get(&url);

        if let Some(api_key) = &self.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
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
    pub async fn get_block_headers(&self, from: u64, to: u64) -> Result<Vec<BlockHeader>, ScannerError> {
        let mut headers = Vec::new();
        
        for height in from..=to {
            let url = format!("{}/blocks/{}/header", self.node_url, height);
            let mut request = self.client.get(&url);

            if let Some(api_key) = &self.api_key {
                request = request.header("api_key", api_key);
            }

            let response = request
                .send()
                .await
                .map_err(|e| ScannerError::Generic(format!("Failed to get block header at height {}: {}", height, e)))?;

            if response.status().is_success() {
                let header: BlockHeader = response
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

    /// Get transactions for a specific block
    pub async fn get_block_transactions(&self, block_id: &str) -> Result<Vec<Transaction>, ScannerError> {
        let url = format!("{}/blocks/{}/transactions", self.node_url, block_id);
        let mut request = self.client.get(&url);

        if let Some(api_key) = &self.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to get block transactions: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::Generic(format!(
                "Node returned error status: {}",
                response.status()
            )));
        }

        let transactions: Vec<Transaction> = response
            .json()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to parse transactions: {}", e)))?;

        Ok(transactions)
    }

    /// Get unspent boxes by ergo tree template hash
    pub async fn get_unspent_boxes_by_template_hash(
        &self,
        template_hash: &str,
    ) -> Result<Vec<ErgoBox>, ScannerError> {
        let url = format!(
            "{}/utxo/byErgoTreeTemplateHash/{}",
            self.node_url, template_hash
        );
        let mut request = self.client.get(&url);

        if let Some(api_key) = &self.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to get unspent boxes: {}", e)))?;

        if !response.status().is_success() {
            return Err(ScannerError::Generic(format!(
                "Node returned error status: {}",
                response.status()
            )));
        }

        let boxes: Vec<ErgoBox> = response
            .json()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to parse unspent boxes: {}", e)))?;

        Ok(boxes)
    }

    /// Get box by ID
    pub async fn get_box_by_id(&self, box_id: &str) -> Result<Option<ErgoBox>, ScannerError> {
        let url = format!("{}/utxo/byId/{}", self.node_url, box_id);
        let mut request = self.client.get(&url);

        if let Some(api_key) = &self.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to get box: {}", e)))?;

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(ScannerError::Generic(format!(
                "Node returned error status: {}",
                response.status()
            )));
        }

        let ergo_box: ErgoBox = response
            .json()
            .await
            .map_err(|e| ScannerError::Generic(format!("Failed to parse box: {}", e)))?;

        Ok(Some(ergo_box))
    }
}

/// Block header for scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub id: String,
    pub height: u64,
    pub timestamp: u64,
    pub parent_id: String,
}

/// Simplified transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<ErgoBox>,
}

/// Transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInput {
    pub box_id: String,
}

/// Simplified Ergo box representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: HashMap<String, String>,
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

/// Real scanner state with blockchain integration
pub struct RealScannerState {
    /// Node client for blockchain interactions
    pub node_client: ErgoNodeClient,
    /// Scanner configuration
    pub config: NodeConfig,
    /// Current blockchain height
    pub current_height: u64,
    /// Last scanned block height
    pub last_scanned_height: u64,
    /// Contract template hash for filtering
    pub contract_template_hash: Option<String>,
    /// Tracked reserve boxes
    pub tracked_reserves: Arc<RwLock<HashMap<String, ErgoBox>>>,
}

impl RealScannerState {
    /// Create a new real scanner state
    pub fn new(node_url: &str, config: NodeConfig, contract_template_hash: Option<String>) -> Self {
        let node_client = ErgoNodeClient::new(node_url, None); // No API key for now
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

        for header in headers {
            debug!("Processing block {} at height {}", header.id, header.height);
            
            // Get transactions for this block
            let transactions = self.node_client.get_block_transactions(&header.id).await?;
            
            // Process transactions for reserve events
            let block_events = self.process_transactions(&transactions, header.height).await?;
            events.extend(block_events);
            
            // Update last scanned height
            self.last_scanned_height = header.height;
        }

        info!("Scanned {} blocks, found {} events", headers.len(), events.len());
        Ok(events)
    }

    /// Process transactions to extract reserve events
    async fn process_transactions(
        &self,
        transactions: &[Transaction],
        height: u64,
    ) -> Result<Vec<ReserveEvent>, ScannerError> {
        let mut events = Vec::new();
        let mut tracked_reserves = self.tracked_reserves.write().await;

        for tx in transactions {
            // Check outputs for new reserve boxes
            for output in &tx.outputs {
                if self.is_reserve_box(output).await? {
                    let box_id = output.box_id.clone();
                    
                    // Check if this is a new reserve
                    if !tracked_reserves.contains_key(&box_id) {
                        events.push(ReserveEvent::ReserveCreated {
                            box_id: box_id.clone(),
                            owner_pubkey: self.extract_owner_pubkey(output).await?,
                            collateral_amount: output.value,
                            height,
                        });
                        tracked_reserves.insert(box_id, output.clone());
                    }
                }
            }

            // Check inputs for spent reserve boxes
            for input in &tx.inputs {
                let box_id = input.box_id.clone();
                
                if tracked_reserves.contains_key(&box_id) {
                    // This reserve box was spent
                    events.push(ReserveEvent::ReserveSpent {
                        box_id: box_id.clone(),
                        height,
                    });
                    tracked_reserves.remove(&box_id);
                }
            }
        }

        Ok(events)
    }

    /// Check if a box is a reserve box
    async fn is_reserve_box(&self, ergo_box: &ErgoBox) -> Result<bool, ScannerError> {
        // Check if the box matches our contract template
        if let Some(template_hash) = &self.contract_template_hash {
            // Extract template hash from ergo tree
            // This is a simplified check - in reality we'd need to parse the ergo tree
            let ergo_tree_hash = hex::encode(&ergo_box.ergo_tree);
            return Ok(ergo_tree_hash.contains(template_hash));
        }

        // If no template hash is specified, check for specific registers
        // that indicate this is a reserve box
        
        // Check for reserve-specific registers
        // This would need to be customized based on the actual Basis contract
        Ok(ergo_box.has_register("R4") || ergo_box.has_register("R5"))
    }

    /// Extract owner public key from reserve box
    async fn extract_owner_pubkey(&self, _ergo_box: &ErgoBox) -> Result<String, ScannerError> {
        // This would need to parse the actual contract to extract the owner pubkey
        // For now, return a placeholder
        Ok("placeholder_pubkey".to_string())
    }

    /// Get unspent reserve boxes
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        if let Some(template_hash) = &self.contract_template_hash {
            self.node_client
                .get_unspent_boxes_by_template_hash(template_hash)
                .await
        } else {
            // Fallback: return tracked reserves
            let tracked = self.tracked_reserves.read().await;
            Ok(tracked.values().cloned().collect())
        }
    }

    /// Start continuous scanning
    pub async fn start_continuous_scanning(&mut self) -> Result<(), ScannerError> {
        info!("Starting continuous blockchain scanning");
        
        // Initial scan to catch up
        let _ = self.scan_new_blocks().await?;
        
        // In a real implementation, this would run in a background task
        // and periodically scan for new blocks
        Ok(())
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

/// Create a real scanner with default configuration
pub fn create_real_scanner(node_url: &str, contract_template_hash: Option<String>) -> RealScannerState {
    let config = NodeConfig::default();
    RealScannerState::new(node_url, config, contract_template_hash)
}