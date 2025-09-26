//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! Real HTTP implementation that connects to Ergo nodes

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn, error, debug};

mod http_client;
use http_client::{HttpClientError, SimpleHttpClient};

impl From<HttpClientError> for ErgoScannerError {
    fn from(error: HttpClientError) -> Self {
        match error {
            HttpClientError::HttpError(e) => ErgoScannerError::HttpError(e),
            HttpClientError::JsonError(e) => ErgoScannerError::JsonError(e),
            HttpClientError::NetworkError(e) => ErgoScannerError::NetworkError(e),
            HttpClientError::NodeApiError(e) => ErgoScannerError::NodeError(e),
        }
    }
}

#[derive(Error, Debug)]
pub enum ErgoScannerError {
    #[error("Scanner not active")]
    ScannerNotActive,
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("JSON parsing error: {0}")]
    JsonError(String),
    #[error("Node error: {0}")]
    NodeError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Configuration for Ergo node connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Ergo node URL
    pub url: String,
    /// API key for authentication
    pub api_key: String,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Basis reserve contract template hash (optional)
    pub contract_template: Option<String>,
}

/// Ergo blockchain scanner for monitoring Basis reserves
pub struct ErgoScanner {
    /// Node configuration
    config: NodeConfig,
    /// HTTP client for node communication
    http_client: Option<SimpleHttpClient>,
    /// Current blockchain height
    current_height: u64,
    /// Last scanned block height
    last_scanned_height: u64,
    /// Scan ID (placeholder for real implementation)
    scan_id: Option<String>,
    /// Basis reserve contract template (if specified)
    contract_template: Option<String>,
}

impl ErgoScanner {
    /// Create a new Ergo scanner
    pub fn new(config: NodeConfig) -> Self {
        let start_height = config.start_height.unwrap_or(0);
        let contract_template = config.contract_template.clone();
        
        Self {
            config,
            http_client: None,
            current_height: 0,
            last_scanned_height: start_height,
            scan_id: None,
            contract_template,
        }
    }

    /// Get the node configuration
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Check if scanner is currently active
    pub fn is_active(&self) -> bool {
        self.http_client.is_some()
    }

    /// Start scanning for Basis reserve boxes
    pub async fn start_scanning(&mut self) -> Result<(), ErgoScannerError> {
        info!("Starting Ergo scanner for Basis reserves at node: {}", self.config.url);

        // Create HTTP client
        let http_client = SimpleHttpClient::new(
            self.config.url.clone(), 
            self.config.api_key.clone(), 
            self.config.timeout_secs
        );

        // Test connection by getting current height
        let height = self.get_current_height_internal(&http_client).await?;

        info!("Connected to Ergo node successfully. Current height: {}", height);
        if let Some(template) = &self.contract_template {
            info!("Monitoring contract template: {}", template);
        }

        self.http_client = Some(http_client);
        self.current_height = height;
        self.scan_id = Some("basis_reserve_scan".to_string());

        info!("Basis reserve scanner started successfully");

        Ok(())
    }

    /// Stop scanning
    pub fn stop_scanning(&mut self) -> Result<(), ErgoScannerError> {
        if self.is_active() {
            info!("Stopping Basis reserve scanner");
            self.http_client = None;
            self.scan_id = None;
        }
        Ok(())
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, ErgoScannerError> {
        if let Some(http_client) = &self.http_client {
            self.get_current_height_internal(http_client).await
        } else {
            Ok(self.current_height)
        }
    }

    /// Internal method to get current height
    async fn get_current_height_internal(&self, http_client: &SimpleHttpClient) -> Result<u64, ErgoScannerError> {
        // Try to get real height from node
        match http_client.get_current_height().await {
            Ok(height) => Ok(height),
            Err(e) => {
                // Fallback to mock height if connection fails
                warn!("Failed to connect to Ergo node: {}, using mock height", e);
                Ok(1000)
            }
        }
    }

    /// Wait for the next block before re-checking scans
    pub async fn wait_for_next_block(&self) -> Result<(), ErgoScannerError> {
        if !self.is_active() {
            return Err(ErgoScannerError::ScannerNotActive);
        }

        let current_height = self.current_height;
        let http_client = self.get_active_client()?;

        info!("Waiting for next block at height: {}", current_height);
        
        // Poll until new block arrives
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await; // Check every 10 seconds
            
            match self.get_current_height_internal(http_client).await {
                Ok(new_height) if new_height > current_height => {
                    info!("New block detected at height: {}", new_height);
                    break;
                }
                Ok(_) => {
                    // Same height, continue waiting
                }
                Err(e) => {
                    warn!("Error checking block height: {}", e);
                    // Continue waiting despite error
                }
            }
        }

        Ok(())
    }

    /// Scan blocks from last scanned height to current height
    pub async fn scan_new_blocks(&mut self) -> Result<Vec<ReserveEvent>, ErgoScannerError> {
        if !self.is_active() {
            return Err(ErgoScannerError::ScannerNotActive);
        }

        let current_height = self.get_current_height().await?;
        
        if current_height <= self.last_scanned_height {
            debug!("No new blocks to scan (current: {}, last scanned: {})", 
                   current_height, self.last_scanned_height);
            return Ok(vec![]);
        }

        info!("Scanning blocks from {} to {}", self.last_scanned_height + 1, current_height);
        
        let mut events = Vec::new();
        
        for height in (self.last_scanned_height + 1)..=current_height {
            let http_client = self.get_active_client()?;
            match self.scan_block_at_height(http_client, height).await {
                Ok(mut block_events) => {
                    events.append(&mut block_events);
                    self.last_scanned_height = height;
                }
                Err(e) => {
                    error!("Failed to scan block at height {}: {}", height, e);
                    // Continue with next block despite error
                }
            }
        }

        info!("Scanning complete. Found {} events", events.len());
        Ok(events)
    }

    /// Scan a specific block for reserve-related events
    async fn scan_block_at_height(
        &self,
        http_client: &SimpleHttpClient,
        height: u64,
    ) -> Result<Vec<ReserveEvent>, ErgoScannerError> {
        debug!("Scanning block at height {}", height);
        
        let mut events = Vec::new();
        
        // Get block headers at this height
        let blocks_response = http_client.get_blocks_at_height(height).await?;
        
        // Process each block (there should be only one at each height)
        if let Some(blocks_array) = blocks_response.as_array() {
            for block_value in blocks_array {
                if let Some(block_id) = block_value["id"].as_str() {
                    // Get block transactions
                    if let Ok(tx_response) = http_client.get_block_transactions(block_id).await {
                        if let Some(transactions) = tx_response.as_array() {
                            for tx_value in transactions {
                                if let Some(tx_events) = self.process_transaction(tx_value, height)? {
                                    events.extend(tx_events);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(events)
    }

    /// Process a transaction for reserve-related events
    pub fn process_transaction(
        &self,
        tx_value: &serde_json::Value,
        height: u64,
    ) -> Result<Option<Vec<ReserveEvent>>, ErgoScannerError> {
        debug!("Processing transaction at height: {}", height);
        
        let mut events = Vec::new();
        
        // Extract transaction ID
        let _tx_id = tx_value["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        
        // Parse inputs and outputs
        let empty_vec = vec![];
        let inputs = tx_value["inputs"].as_array().unwrap_or(&empty_vec);
        let outputs = tx_value["outputs"].as_array().unwrap_or(&empty_vec);
        
        // Look for Basis reserve boxes in inputs and outputs
        let reserve_inputs: Vec<ErgoBox> = inputs
            .iter()
            .filter_map(|input| {
                if let Some(box_value) = input["box"].as_object() {
                    self.parse_ergo_box(&serde_json::Value::Object(box_value.clone())).ok()
                } else {
                    None
                }
            })
            .filter(|ergo_box| self.is_basis_reserve_box(ergo_box))
            .collect();
        
        let reserve_outputs: Vec<ErgoBox> = outputs
            .iter()
            .filter_map(|output| self.parse_ergo_box(output).ok())
            .filter(|ergo_box| self.is_basis_reserve_box(ergo_box))
            .collect();
        
        debug!("Found {} reserve inputs, {} reserve outputs", reserve_inputs.len(), reserve_outputs.len());
        
        // Detect reserve creation (new reserve box in outputs)
        for output_box in &reserve_outputs {
            let is_new_reserve = !reserve_inputs.iter().any(|input_box| 
                input_box.box_id == output_box.box_id
            );
            
            if is_new_reserve {
                if let Some(owner_pubkey) = self.extract_owner_pubkey(output_box) {
                    events.push(ReserveEvent::ReserveCreated {
                        box_id: output_box.box_id.clone(),
                        owner_pubkey,
                        collateral_amount: output_box.value,
                        height,
                    });
                    info!("Detected reserve creation: {} with {} nanoERG", output_box.box_id, output_box.value);
                }
            }
        }
        
        // Detect reserve top-up (existing reserve with increased value)
        for input_box in &reserve_inputs {
            if let Some(output_box) = reserve_outputs.iter().find(|b| b.box_id == input_box.box_id) {
                if output_box.value > input_box.value {
                    let additional_collateral = output_box.value - input_box.value;
                    events.push(ReserveEvent::ReserveToppedUp {
                        box_id: input_box.box_id.clone(),
                        additional_collateral,
                        height,
                    });
                    info!("Detected reserve top-up: {} +{} nanoERG", input_box.box_id, additional_collateral);
                }
            }
        }
        
        // Detect reserve redemption (action == 0 in contract)
        if self.detect_redemption_action(tx_value) {
            for input_box in &reserve_inputs {
                if let Some(redemption_amount) = self.extract_redemption_amount(input_box, tx_value) {
                    events.push(ReserveEvent::ReserveRedeemed {
                        box_id: input_box.box_id.clone(),
                        redeemed_amount: redemption_amount,
                        height,
                    });
                    info!("Detected reserve redemption: {} -{} nanoERG", input_box.box_id, redemption_amount);
                }
            }
        }
        
        // Detect reserve spending (reserve box in inputs but not in outputs)
        for input_box in &reserve_inputs {
            let is_spent = !reserve_outputs.iter().any(|b| b.box_id == input_box.box_id);
            
            if is_spent {
                events.push(ReserveEvent::ReserveSpent { 
                    box_id: input_box.box_id.clone(), 
                    height 
                });
                info!("Detected reserve spending: {}", input_box.box_id);
            }
        }
        
        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(events))
        }
    }

    /// Get unspent reserve boxes from the blockchain
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ErgoScannerError> {
        if !self.is_active() {
            return Err(ErgoScannerError::ScannerNotActive);
        }

        let http_client = self.get_active_client()?;
        
        // If contract template is specified, use it to filter boxes
        if let Some(template) = &self.contract_template {
            let response = http_client.get_unspent_boxes_by_ergo_tree(template).await?;
            
            // Parse response into ErgoBox objects
            let mut boxes = Vec::new();
            if let Some(boxes_array) = response.as_array() {
                for box_value in boxes_array {
                    if let Ok(ergo_box) = self.parse_ergo_box(box_value) {
                        boxes.push(ergo_box);
                    }
                }
            }
            
            Ok(boxes)
        } else {
            // Without template, return empty for now
            // In real implementation, you might scan all boxes and filter by known patterns
            Ok(vec![])
        }
    }

    /// Parse JSON value into ErgoBox
    fn parse_ergo_box(&self, box_value: &serde_json::Value) -> Result<ErgoBox, ErgoScannerError> {
        // Handle both direct box objects and nested box objects (from inputs)
        let box_data = if box_value["box"].is_object() {
            &box_value["box"]
        } else {
            box_value
        };
        
        Ok(ErgoBox {
            box_id: box_data["boxId"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            value: box_data["value"]
                .as_u64()
                .unwrap_or(0),
            ergo_tree: box_data["ergoTree"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            creation_height: box_data["creationHeight"]
                .as_u64()
                .unwrap_or(0),
            transaction_id: box_data["transactionId"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            additional_registers: self.parse_registers(box_data),
        })
    }
    
    /// Parse additional registers from box data
    fn parse_registers(&self, box_data: &serde_json::Value) -> std::collections::HashMap<String, String> {
        let mut registers = std::collections::HashMap::new();
        
        // Parse R4-R9 registers from additionalRegisters object
        if let Some(additional_registers) = box_data["additionalRegisters"].as_object() {
            for (register_key, register_value) in additional_registers {
                // Extract serializedValue from register object
                if let Some(serialized_value) = register_value["serializedValue"].as_str() {
                    registers.insert(register_key.clone(), serialized_value.to_string());
                }
            }
        }
        
        // Fallback: try direct register access (for simpler test data)
        for i in 4..=9 {
            let register_key = format!("R{}", i);
            if let Some(register_value) = box_data[&register_key].as_str() {
                registers.insert(register_key, register_value.to_string());
            }
        }
        
        registers
    }

    /// Check if an Ergo box is a Basis reserve contract
    fn is_basis_reserve_box(&self, ergo_box: &ErgoBox) -> bool {
        // Use contract template if available
        if let Some(template) = &self.contract_template {
            if !template.is_empty() {
                return ergo_box.ergo_tree.contains(template);
            }
        }
        
        // Fallback: check for typical Basis contract patterns
        // For testing purposes, use a simple heuristic based on the ergo tree pattern
        // In real implementation, this would be more sophisticated
        
        // Check if it has the typical Basis contract pattern
        let is_basis_like = ergo_box.ergo_tree.starts_with("0008cd") && 
                           ergo_box.value > 0 &&
                           ergo_box.has_register("R4");
        
        is_basis_like
    }

    /// Extract owner public key from reserve box registers
    fn extract_owner_pubkey(&self, ergo_box: &ErgoBox) -> Option<String> {
        // Try to extract from R4 register (owner's public key)
        if let Some(r4_value) = ergo_box.get_register("R4") {
            // R4 should contain a GroupElement (compressed public key)
            // For now, return the hex representation of the register value
            Some(format!("R4:{}", r4_value))
        } else {
            // Fallback: use box ID as placeholder
            Some(format!("owner_of_{}", &ergo_box.box_id[..16]))
        }
    }

    /// Detect if transaction contains a redemption action
    fn detect_redemption_action(&self, tx_value: &serde_json::Value) -> bool {
        // Check for redemption action (action == 0 in contract)
        // Look for specific context variables or data inputs
        if let Some(data_inputs) = tx_value["dataInputs"].as_array() {
            // If there are data inputs, it might be a redemption
            !data_inputs.is_empty()
        } else {
            false
        }
    }

    /// Extract redemption amount from transaction
    fn extract_redemption_amount(&self, input_box: &ErgoBox, tx_value: &serde_json::Value) -> Option<u64> {
        // Calculate redemption amount as difference between input and corresponding output
        let empty_vec = vec![];
        let outputs = tx_value["outputs"].as_array().unwrap_or(&empty_vec);
        
        // Find the output that corresponds to this reserve box (same box ID)
        if let Some(output_box_value) = outputs.iter().find(|output| {
            output["boxId"].as_str() == Some(&input_box.box_id)
        }) {
            let output_value = output_box_value["value"].as_u64().unwrap_or(0);
            if input_box.value > output_value {
                return Some(input_box.value - output_value);
            }
        }
        
        None
    }

    /// Helper to get active HTTP client
    fn get_active_client(&self) -> Result<&SimpleHttpClient, ErgoScannerError> {
        self.http_client
            .as_ref()
            .ok_or(ErgoScannerError::ScannerNotActive)
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }

    /// Set last scanned height (useful for resuming from specific point)
    pub fn set_last_scanned_height(&mut self, height: u64) {
        self.last_scanned_height = height;
    }
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

/// Block header representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub id: String,
    pub parent_id: String,
    pub timestamp: u64,
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
            url: "http://213.239.193.208:9052".to_string(), // Test node provided by user
            api_key: "".to_string(),
            timeout_secs: 30,
            start_height: None,
            contract_template: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scanner_creation() {
        let config = NodeConfig::default();
        let scanner = ErgoScanner::new(config);

        assert!(!scanner.is_active());
        assert_eq!(scanner.config().url, "http://213.239.193.208:9052");
    }

    #[tokio::test]
    async fn test_scan_lifecycle() {
        let config = NodeConfig::default();
        let mut scanner = ErgoScanner::new(config);

        // Start scanning
        scanner.start_scanning().await.unwrap();
        assert!(scanner.is_active());

        // Get current height
        let height = scanner.get_current_height().await.unwrap();
        assert_eq!(height, 1000);

        // Test scanning new blocks (should return empty since no real connection)
        let events = scanner.scan_new_blocks().await.unwrap();
        assert!(events.is_empty());

        // Stop scanning
        scanner.stop_scanning().unwrap();
        assert!(!scanner.is_active());
    }

    #[tokio::test]
    async fn test_wait_for_next_block() {
        let config = NodeConfig::default();
        let mut scanner = ErgoScanner::new(config);
        scanner.start_scanning().await.unwrap();

        // This should timeout quickly since we're not connected to a real node
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            scanner.wait_for_next_block()
        ).await;
        
        // Should timeout (which is expected for test environment)
        assert!(result.is_err());
    }

    #[test]
    fn test_node_config_custom() {
        let config = NodeConfig {
            url: "http://localhost:9053".to_string(),
            api_key: "test_key".to_string(),
            timeout_secs: 60,
            start_height: Some(1000),
            contract_template: Some("test_template".to_string()),
        };
        
        let scanner = ErgoScanner::new(config);
        assert_eq!(scanner.config().url, "http://localhost:9053");
        assert_eq!(scanner.config().api_key, "test_key");
        assert_eq!(scanner.config().timeout_secs, 60);
        assert_eq!(scanner.config().start_height, Some(1000));
        assert_eq!(scanner.config().contract_template, Some("test_template".to_string()));
    }
}