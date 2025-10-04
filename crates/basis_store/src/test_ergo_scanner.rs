//! Test-only mock Ergo scanner implementation
//! This module provides mock blockchain data for testing purposes

use serde::{Deserialize, Serialize};

use crate::ergo_scanner::{ErgoBox, NodeConfig, ReserveEvent, ScannerError};

/// Mock server state for scanner (test-only)
/// This provides mock blockchain data for testing without requiring network access
pub struct MockServerState {
    pub config: NodeConfig,
    pub current_height: u64,
    pub last_scanned_height: u64,
}

impl MockServerState {
    /// Create a new mock server state
    pub fn new(config: NodeConfig) -> Self {
        let start_height = config.start_height.unwrap_or(0);
        Self {
            config,
            current_height: 0,
            last_scanned_height: start_height,
        }
    }

    /// Get current blockchain height (mock)
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        // Return a mock height for testing
        Ok(1000)
    }

    /// Scan for new events (mock)
    pub async fn scan_new_blocks(&mut self) -> Result<Vec<ReserveEvent>, ScannerError> {
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

    /// Get unspent reserve boxes (mock)
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ErgoBox>, ScannerError> {
        // Simplified implementation - returns mock boxes for testing
        Ok(vec![])
    }

    /// Check if scanner is active
    pub fn is_active(&self) -> bool {
        true
    }

    /// Start scanning (mock)
    pub async fn start_scanning(&mut self) -> Result<(), ScannerError> {
        // Simplified implementation for testing
        Ok(())
    }

    /// Get last scanned height
    pub fn last_scanned_height(&self) -> u64 {
        self.last_scanned_height
    }
}

/// Create a mock scanner with default configuration (test-only)
pub fn create_mock_scanner() -> MockServerState {
    let config = NodeConfig::default();
    MockServerState::new(config)
}