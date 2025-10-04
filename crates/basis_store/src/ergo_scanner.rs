//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides modern blockchain integration using /scan and /blockchain APIs

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod ergo_scanner;

#[cfg(feature = "ergo_scanner")]
pub mod real_ergo_scanner;

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

/// Configuration for scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Basis reserve contract template hash (optional)
    pub contract_template: Option<String>,
}

/// Server state for scanner
/// Can use either mock implementation or real blockchain integration
pub struct ServerState {
    pub config: NodeConfig,
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub use_real_scanner: bool,
    pub node_url: Option<String>,
}

impl ServerState {
    pub fn new(config: NodeConfig) -> Self {
        let start_height = config.start_height.unwrap_or(0);
        Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
            use_real_scanner: false,
            node_url: None,
        }
    }

    /// Create a server state that uses real Ergo scanner
    pub fn new_real_scanner(config: NodeConfig, node_url: String) -> Self {
        let start_height = config.start_height.unwrap_or(0);
        Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
            use_real_scanner: true,
            node_url: Some(node_url),
        }
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        if self.use_real_scanner {
            #[cfg(feature = "ergo_scanner")]
            {
                if let Some(node_url) = &self.node_url {
                    let mut real_scanner = crate::ergo_scanner::real_ergo_scanner::create_real_ergo_scanner(node_url);
                    real_scanner.get_current_height().await
                } else {
                    Err(ScannerError::Generic("No node URL configured for real scanner".to_string()))
                }
            }
            #[cfg(not(feature = "ergo_scanner"))]
            {
                Err(ScannerError::Generic("ergo_scanner feature not enabled".to_string()))
            }
        } else {
            // Return a mock height for testing
            Ok(1000)
        }
    }

    /// Scan for new events
    pub async fn scan_new_blocks(&mut self) -> Result<Vec<ReserveEvent>, ScannerError> {
        if self.use_real_scanner {
            #[cfg(feature = "ergo_scanner")]
            {
                if let Some(node_url) = &self.node_url {
                    let mut real_scanner = crate::ergo_scanner::real_ergo_scanner::create_real_ergo_scanner(node_url);
                    real_scanner.scan_new_blocks().await
                } else {
                    Err(ScannerError::Generic("No node URL configured for real scanner".to_string()))
                }
            }
            #[cfg(not(feature = "ergo_scanner"))]
            {
                Err(ScannerError::Generic("ergo_scanner feature not enabled".to_string()))
            }
        } else {
            // Simplified implementation - returns mock events for testing
            if self.current_height < self.last_scanned_height + 10 {
                self.current_height += 1;
                Ok(vec![])
            } else {
                // Simulate finding a reserve event occasionally
                if self.current_height % 100 == 0 {
                    Ok(vec![ReserveEvent::ReserveCreated {
                        box_id: format!("mock_box_{}", self.current_height),
                        owner_pubkey: "mock_pubkey".to_string(),
                        collateral_amount: 1000000000, // 1 ERG
                        height: self.current_height,
                    }])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    /// Get unspent reserve boxes
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        if self.use_real_scanner {
            #[cfg(feature = "ergo_scanner")]
            {
                if let Some(node_url) = &self.node_url {
                    let real_scanner = crate::ergo_scanner::real_ergo_scanner::create_real_ergo_scanner(node_url);
                    real_scanner.get_unspent_reserve_boxes().await
                } else {
                    Err(ScannerError::Generic("No node URL configured for real scanner".to_string()))
                }
            }
            #[cfg(not(feature = "ergo_scanner"))]
            {
                Err(ScannerError::Generic("ergo_scanner feature not enabled".to_string()))
            }
        } else {
            // Simplified implementation - returns mock boxes for testing
            Ok(vec![])
        }
    }

    /// Check if scanner is active
    pub fn is_active(&self) -> bool {
        true
    }

    /// Start scanning
    pub async fn start_scanning(&mut self) -> Result<(), ScannerError> {
        if self.use_real_scanner {
            #[cfg(feature = "ergo_scanner")]
            {
                if let Some(node_url) = &self.node_url {
                    let mut real_scanner = crate::ergo_scanner::real_ergo_scanner::create_real_ergo_scanner(node_url);
                    real_scanner.start_scanning().await
                } else {
                    Err(ScannerError::Generic("No node URL configured for real scanner".to_string()))
                }
            }
            #[cfg(not(feature = "ergo_scanner"))]
            {
                Err(ScannerError::Generic("ergo_scanner feature not enabled".to_string()))
            }
        } else {
            // Simplified implementation for testing
            Ok(())
        }
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }
}

/// Start the scanner
pub async fn start_scanner(_state: ServerState) -> Result<(), ScannerError> {
    // Background scanning would be implemented here
    // For now, just return success
    Ok(())
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
            start_height: None,
            contract_template: None,
        }
    }
}
