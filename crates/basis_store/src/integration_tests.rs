//! Integration tests for Basis Store with real Ergo blockchain scanner

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::ergo_scanner::{NodeConfig, ReserveEvent, ScannerError};

#[cfg(feature = "real_scanner")]
use crate::ergo_scanner::real_ergo_scanner::{self, ErgoNodeClient, RealScannerState};

/// Test configuration for integration tests
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Ergo node URL for testing
    pub node_url: String,
    /// Contract template hash to scan for
    pub contract_template_hash: Option<String>,
    /// Test timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9053".to_string(),
            contract_template_hash: None,
            timeout_seconds: 30,
        }
    }
}

/// Integration test suite for real Ergo scanner
#[cfg(feature = "real_scanner")]
pub struct IntegrationTestSuite {
    config: TestConfig,
    scanner: Option<RealScannerState>,
}

#[cfg(feature = "real_scanner")]
impl IntegrationTestSuite {
    /// Create a new integration test suite
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            scanner: None,
        }
    }

    /// Initialize the scanner for testing
    pub async fn initialize_scanner(&mut self) -> Result<(), ScannerError> {
        let scanner = real_ergo_scanner::create_real_scanner(
            &self.config.node_url,
            self.config.contract_template_hash.clone(),
        );
        
        // Test node connectivity
        let height = scanner.get_current_height().await?;
        println!("Connected to Ergo node at height: {}", height);
        
        self.scanner = Some(scanner);
        Ok(())
    }

    /// Test basic node connectivity
    pub async fn test_node_connectivity(&self) -> Result<(), ScannerError> {
        let scanner = self.scanner.as_ref().expect("Scanner not initialized");
        
        let height = scanner.get_current_height().await?;
        assert!(height > 0, "Blockchain height should be greater than 0");
        
        println!("✓ Node connectivity test passed - current height: {}", height);
        Ok(())
    }

    /// Test block scanning functionality
    pub async fn test_block_scanning(&mut self) -> Result<(), ScannerError> {
        let scanner = self.scanner.as_mut().expect("Scanner not initialized");
        
        let initial_height = scanner.last_scanned_height();
        
        // Scan for new blocks
        let events = scanner.scan_new_blocks().await?;
        
        let final_height = scanner.last_scanned_height();
        
        println!("Scanned from height {} to {}", initial_height, final_height);
        println!("Found {} events", events.len());
        
        // Verify we processed some blocks (or at least didn't error)
        assert!(final_height >= initial_height, "Scanner should advance or stay at same height");
        
        println!("✓ Block scanning test passed");
        Ok(())
    }

    /// Test unspent box querying
    pub async fn test_unspent_boxes(&self) -> Result<(), ScannerError> {
        let scanner = self.scanner.as_ref().expect("Scanner not initialized");
        
        let boxes = scanner.get_unspent_reserve_boxes().await?;
        
        println!("Found {} unspent reserve boxes", boxes.len());
        
        // This test passes as long as the query doesn't error
        // The number of boxes depends on the actual blockchain state
        
        println!("✓ Unspent boxes test passed");
        Ok(())
    }

    /// Test continuous scanning simulation
    pub async fn test_continuous_scanning(&mut self) -> Result<(), ScannerError> {
        let scanner = self.scanner.as_mut().expect("Scanner not initialized");
        
        // Start continuous scanning
        scanner.start_continuous_scanning().await?;
        
        // Do a few scans to simulate continuous operation
        for i in 0..3 {
            let events = scanner.scan_new_blocks().await?;
            println!("Scan {}: found {} events", i + 1, events.len());
            
            // Small delay between scans
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        
        println!("✓ Continuous scanning test passed");
        Ok(())
    }

    /// Test event processing
    pub async fn test_event_processing(&mut self) -> Result<(), ScannerError> {
        let scanner = self.scanner.as_mut().expect("Scanner not initialized");
        
        let events = scanner.scan_new_blocks().await?;
        
        // Process each event type
        for event in events {
            match event {
                ReserveEvent::ReserveCreated { box_id, owner_pubkey, collateral_amount, height } => {
                    println!("Reserve created: box_id={}, owner={}, collateral={}, height={}", 
                            box_id, owner_pubkey, collateral_amount, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(collateral_amount > 0, "Collateral amount should be positive");
                    assert!(height > 0, "Height should be positive");
                }
                ReserveEvent::ReserveToppedUp { box_id, additional_collateral, height } => {
                    println!("Reserve topped up: box_id={}, additional={}, height={}", 
                            box_id, additional_collateral, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(additional_collateral > 0, "Additional collateral should be positive");
                }
                ReserveEvent::ReserveRedeemed { box_id, redeemed_amount, height } => {
                    println!("Reserve redeemed: box_id={}, amount={}, height={}", 
                            box_id, redeemed_amount, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(redeemed_amount > 0, "Redeemed amount should be positive");
                }
                ReserveEvent::ReserveSpent { box_id, height } => {
                    println!("Reserve spent: box_id={}, height={}", box_id, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                }
            }
        }
        
        println!("✓ Event processing test passed");
        Ok(())
    }

    /// Run all integration tests
    pub async fn run_all_tests(&mut self) -> Result<(), ScannerError> {
        println!("Running Ergo scanner integration tests...");
        
        self.initialize_scanner().await?;
        self.test_node_connectivity().await?;
        self.test_block_scanning().await?;
        self.test_unspent_boxes().await?;
        self.test_event_processing().await?;
        self.test_continuous_scanning().await?;
        
        println!("✓ All integration tests passed!");
        Ok(())
    }
}

/// Mock integration tests for when real_scanner feature is disabled
#[cfg(not(feature = "real_scanner"))]
pub struct IntegrationTestSuite {
    config: TestConfig,
}

#[cfg(not(feature = "real_scanner"))]
impl IntegrationTestSuite {
    pub fn new(config: TestConfig) -> Self {
        Self { config }
    }

    pub async fn run_all_tests(&mut self) -> Result<(), ScannerError> {
        println!("Real scanner feature not enabled - running mock integration tests");
        
        // Mock tests that simulate scanner behavior
        self.test_mock_node_connectivity().await?;
        self.test_mock_block_scanning().await?;
        self.test_mock_event_processing().await?;
        
        println!("✓ All mock integration tests passed!");
        Ok(())
    }

    async fn test_mock_node_connectivity(&self) -> Result<(), ScannerError> {
        println!("Mock: Testing node connectivity");
        // Simulate successful connection
        Ok(())
    }

    async fn test_mock_block_scanning(&mut self) -> Result<(), ScannerError> {
        println!("Mock: Testing block scanning");
        // Simulate successful scanning
        Ok(())
    }

    async fn test_mock_event_processing(&mut self) -> Result<(), ScannerError> {
        println!("Mock: Testing event processing");
        // Simulate event processing
        Ok(())
    }
}

/// Test utilities for integration tests
pub mod test_utils {
    use super::*;

    /// Create test configuration for different networks
    pub fn testnet_config() -> TestConfig {
        TestConfig {
            node_url: "http://213.239.193.208:9052".to_string(),
            contract_template_hash: None,
            timeout_seconds: 30,
        }
    }

    pub fn mainnet_config() -> TestConfig {
        TestConfig {
            node_url: "http://213.239.193.208:9053".to_string(),
            contract_template_hash: None,
            timeout_seconds: 30,
        }
    }

    pub fn local_config() -> TestConfig {
        TestConfig::default()
    }

    /// Check if a node is reachable
    #[cfg(feature = "real_scanner")]
    pub async fn is_node_reachable(node_url: &str) -> bool {
        let client = ErgoNodeClient::new(node_url, None);
        client.get_current_height().await.is_ok()
    }

    #[cfg(not(feature = "real_scanner"))]
    pub async fn is_node_reachable(_node_url: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integration_test_suite_creation() {
        let config = TestConfig::default();
        let mut test_suite = IntegrationTestSuite::new(config);
        
        // This should not panic
        let result = test_suite.run_all_tests().await;
        assert!(result.is_ok(), "Test suite should run without error");
    }

    #[tokio::test]
    async fn test_test_config_defaults() {
        let config = TestConfig::default();
        assert_eq!(config.node_url, "http://localhost:9053");
        assert_eq!(config.timeout_seconds, 30);
    }

    #[tokio::test]
    async fn test_test_utils_network_configs() {
        let testnet = test_utils::testnet_config();
        assert!(testnet.node_url.contains("9052")); // Testnet port
        
        let mainnet = test_utils::mainnet_config();
        assert!(mainnet.node_url.contains("9053")); // Mainnet port
        
        let local = test_utils::local_config();
        assert!(local.node_url.contains("localhost"));
    }
}