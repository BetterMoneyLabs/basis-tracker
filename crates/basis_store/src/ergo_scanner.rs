//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! Following chaincash-rs pattern with simplified HTTP client

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum ScannerError {
    #[error("Scanner error: {0}")]
    Generic(String),
    #[error("Store error: {0}")]
    StoreError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Reserves,
    Notes,
}

impl ScanType {
    pub fn to_str(&self) -> &'static str {
        match self {
            ScanType::Reserves => "reserves",
            ScanType::Notes => "notes",
        }
    }
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

/// Simplified server state for scanner
pub struct ServerState {
    pub config: NodeConfig,
    pub current_height: u64,
    pub last_scanned_height: u64,
}

impl ServerState {
    pub fn new(config: NodeConfig) -> Self {
        let start_height = config.start_height.unwrap_or(0);
        Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
        }
    }

    /// Get current blockchain height (placeholder implementation)
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        // In real implementation, this would connect to the Ergo node
        // For now, return a mock height
        Ok(1000)
    }

    /// Scan blocks from last scanned height to current height
    pub async fn scan_new_blocks(&mut self) -> Result<Vec<ReserveEvent>, ScannerError> {
        let current_height = self.get_current_height().await?;

        if current_height <= self.last_scanned_height {
            return Ok(vec![]);
        }

        info!(
            "Scanning blocks from {} to {}",
            self.last_scanned_height + 1,
            current_height
        );

        let mut events = Vec::new();

        // In real implementation, this would scan actual blocks
        // For now, return mock events
        for height in (self.last_scanned_height + 1)..=current_height {
            // Simulate finding some reserve events
            if height % 10 == 0 {
                events.push(ReserveEvent::ReserveCreated {
                    box_id: format!("box_{}", height),
                    owner_pubkey: format!("owner_{}", height),
                    collateral_amount: 1000000 + height * 1000,
                    height,
                });
            }

            self.last_scanned_height = height;
        }

        info!("Scanning complete. Found {} events", events.len());
        Ok(events)
    }

    /// Wait for the next block before re-checking scans
    pub async fn wait_for_next_block(&self) -> Result<(), ScannerError> {
        info!("Waiting for next block at height: {}", self.current_height);

        // Poll until new block arrives
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            match self.get_current_height().await {
                Ok(new_height) if new_height > self.current_height => {
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

    /// Get unspent reserve boxes from the blockchain
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        // In real implementation, this would query the Ergo node
        // For now, return mock boxes
        let mut boxes = Vec::new();

        for i in 0..5 {
            boxes.push(ErgoBox {
                box_id: format!("unspent_box_{}", i),
                value: 1000000 + i * 100000,
                ergo_tree: "0008cd...".to_string(),
                creation_height: 1000,
                transaction_id: format!("tx_{}", i),
                additional_registers: std::collections::HashMap::new(),
            });
        }

        Ok(boxes)
    }
}

/// Start the scanner following chaincash-rs pattern
pub async fn start_scanner(mut state: ServerState) -> Result<(), ScannerError> {
    info!("Starting Basis scanner (chaincash-rs pattern)...");

    // Initialize scanner state
    state.current_height = state.get_current_height().await?;

    info!("Scanner started successfully");

    // Start background scanning tasks
    tokio::spawn(reserve_scanner(state));

    Ok(())
}

async fn reserve_scanner(mut state: ServerState) {
    info!("Starting reserve scanner...");

    loop {
        // Scan for new reserve events
        match state.scan_new_blocks().await {
            Ok(events) => {
                for event in events {
                    match event {
                        ReserveEvent::ReserveCreated {
                            box_id,
                            owner_pubkey,
                            collateral_amount,
                            height,
                        } => {
                            info!(
                                "Reserve created: {} by {} with {} nanoERG at height {}",
                                box_id, owner_pubkey, collateral_amount, height
                            );
                        }
                        ReserveEvent::ReserveToppedUp {
                            box_id,
                            additional_collateral,
                            height,
                        } => {
                            info!(
                                "Reserve topped up: {} +{} nanoERG at height {}",
                                box_id, additional_collateral, height
                            );
                        }
                        ReserveEvent::ReserveRedeemed {
                            box_id,
                            redeemed_amount,
                            height,
                        } => {
                            info!(
                                "Reserve redeemed: {} -{} nanoERG at height {}",
                                box_id, redeemed_amount, height
                            );
                        }
                        ReserveEvent::ReserveSpent { box_id, height } => {
                            info!("Reserve spent: {} at height {}", box_id, height);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Error scanning blocks: {}", e);
            }
        }

        // Wait for next block
        if let Err(e) = state.wait_for_next_block().await {
            warn!("Error waiting for next block: {}", e);
        }

        // Small delay to prevent tight loop
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Create a scanner with default configuration
pub fn create_default_scanner() -> ServerState {
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
            url: "http://213.239.193.208:9052".to_string(), // Test node provided by user
            api_key: "".to_string(),
            timeout_secs: 30,
            start_height: None,
            contract_template: None,
        }
    }
}
