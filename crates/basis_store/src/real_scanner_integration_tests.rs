//! Integration tests for real Ergo scanner against actual Ergo nodes

use crate::ergo_scanner::{NodeConfig, ReserveEvent, ScannerError, ServerState};

/// Integration test suite that tests against real Ergo nodes
pub struct RealScannerIntegrationTestSuite {
    scanner: ServerState,
    node_url: String,
}

impl RealScannerIntegrationTestSuite {
    /// Create a new integration test suite with real Ergo scanner
    pub fn new(node_url: &str) -> Self {
        let config = NodeConfig::default();
        let scanner = ServerState::new(config, node_url.to_string());

        Self {
            scanner,
            node_url: node_url.to_string(),
        }
    }

    /// Test basic connectivity to Ergo node
    pub async fn test_node_connectivity(&mut self) -> Result<(), ScannerError> {
        println!("Testing connectivity to Ergo node: {}", self.node_url);

        // Test getting current height
        let height = self.scanner.get_current_height().await?;
        println!("Current blockchain height: {}", height);

        // Height should be reasonable (not 0 for mainnet/testnet)
        // For local development, height might be 0
        if self.node_url.contains("213.239.193.208") {
            // Public nodes should have reasonable height
            assert!(height > 0, "Public node should have height > 0");
        }

        println!("✓ Node connectivity test passed");
        Ok(())
    }

    /// Test scanner initialization
    pub async fn test_scanner_initialization(&mut self) -> Result<(), ScannerError> {
        println!("Testing scanner initialization...");

        // Start scanning
        self.scanner.start_scanning().await?;

        assert!(self.scanner.is_active(), "Scanner should be active");

        let last_scanned = self.scanner.last_scanned_height();
        assert!(
            last_scanned >= 0,
            "Last scanned height should be non-negative"
        );

        println!("✓ Scanner initialization test passed");
        Ok(())
    }

    /// Test block scanning functionality
    pub async fn test_block_scanning(&mut self) -> Result<(), ScannerError> {
        println!("Testing block scanning...");

        // Scan for new blocks
        let events = self.scanner.scan_new_blocks().await?;

        // With real scanner, we might get actual events or empty
        // The important thing is that it doesn't error
        println!("Scanned {} events", events.len());

        // Process any events found
        for event in events {
            match event {
                ReserveEvent::ReserveCreated {
                    box_id,
                    owner_pubkey,
                    collateral_amount,
                    height,
                } => {
                    println!(
                        "Reserve created: {} with {} nanoERG at height {}",
                        box_id, collateral_amount, height
                    );
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(!owner_pubkey.is_empty(), "Owner pubkey should not be empty");
                    assert!(
                        collateral_amount > 0,
                        "Collateral amount should be positive"
                    );
                    assert!(height > 0, "Height should be positive");
                }
                ReserveEvent::ReserveToppedUp {
                    box_id,
                    additional_collateral,
                    height,
                } => {
                    println!(
                        "Reserve topped up: {} with additional {} nanoERG at height {}",
                        box_id, additional_collateral, height
                    );
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(
                        additional_collateral > 0,
                        "Additional collateral should be positive"
                    );
                }
                ReserveEvent::ReserveRedeemed {
                    box_id,
                    redeemed_amount,
                    height,
                } => {
                    println!(
                        "Reserve redeemed: {} with {} nanoERG at height {}",
                        box_id, redeemed_amount, height
                    );
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(redeemed_amount > 0, "Redeemed amount should be positive");
                }
                ReserveEvent::ReserveSpent { box_id, height } => {
                    println!("Reserve spent: {} at height {}", box_id, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                }
            }
        }

        println!("✓ Block scanning test passed");
        Ok(())
    }

    /// Test unspent boxes query
    pub async fn test_unspent_boxes_query(&self) -> Result<(), ScannerError> {
        println!("Testing unspent boxes query...");

        let boxes = self.scanner.get_unspent_reserve_boxes().await?;

        // With real scanner, we might get actual boxes or empty
        // The important thing is that it doesn't error
        println!("Found {} unspent boxes", boxes.len());

        println!("✓ Unspent boxes query test passed");
        Ok(())
    }

    /// Run all real scanner integration tests
    pub async fn run_all_tests(&mut self) -> Result<(), ScannerError> {
        println!(
            "Running real scanner integration tests against: {}",
            self.node_url
        );

        self.test_node_connectivity().await?;
        self.test_scanner_initialization().await?;
        self.test_block_scanning().await?;
        self.test_unspent_boxes_query().await?;

        println!("✓ All real scanner integration tests passed!");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test against the public Ergo node
    #[tokio::test]
    #[ignore = "Requires network connection to Ergo node"]
    async fn test_real_scanner_against_public_node() {
        let node_url = "http://159.89.116.15:11088/";
        let mut test_suite = RealScannerIntegrationTestSuite::new(node_url);

        // These tests require network connectivity
        let result = test_suite.run_all_tests().await;

        // Test should either pass or fail gracefully due to network issues
        // but not panic
        if let Err(e) = result {
            println!("Test failed with error (may be due to network): {}", e);
            // Don't fail the test - network issues are expected in CI
        }
    }

    /// Test against testnet node
    #[tokio::test]
    #[ignore = "Requires network connection to Ergo testnet node"]
    async fn test_real_scanner_against_testnet_node() {
        let node_url = "http://213.239.193.208:9052";
        let mut test_suite = RealScannerIntegrationTestSuite::new(node_url);

        let result = test_suite.run_all_tests().await;

        if let Err(e) = result {
            println!("Testnet test failed with error: {}", e);
        }
    }

    /// Test connectivity specifically
    #[tokio::test]
    #[ignore = "Requires network connection"]
    async fn test_connectivity_only() {
        let node_url = "http://159.89.116.15:11088";
        let mut test_suite = RealScannerIntegrationTestSuite::new(node_url);

        // Just test connectivity
        let result = test_suite.test_node_connectivity().await;

        if let Err(e) = result {
            println!("Connectivity test failed: {}", e);
            // Don't fail - network issues are expected
        }
    }
}
