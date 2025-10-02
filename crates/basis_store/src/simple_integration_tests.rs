//! Simple integration tests that work with mock scanner implementation

use crate::ergo_scanner::{NodeConfig, ReserveEvent, ScannerError, ServerState};

/// Simple integration test suite that works with both real and mock scanners
pub struct SimpleIntegrationTestSuite {
    scanner: ServerState,
}

impl SimpleIntegrationTestSuite {
    /// Create a new simple integration test suite
    pub fn new() -> Self {
        let config = NodeConfig::default();
        let scanner = ServerState::new(config);

        Self { scanner }
    }

    /// Test basic scanner functionality
    pub async fn test_basic_functionality(&mut self) -> Result<(), ScannerError> {
        println!("Testing basic scanner functionality...");

        // Test getting current height
        let height = self.scanner.get_current_height().await?;
        assert!(height > 0, "Height should be positive");

        // Test scanning blocks
        let events = self.scanner.scan_new_blocks().await?;
        assert!(events.len() <= 1, "Should return 0 or 1 mock events");

        // Test unspent boxes
        let boxes = self.scanner.get_unspent_reserve_boxes().await?;
        assert!(
            boxes.is_empty(),
            "Mock implementation should return empty boxes"
        );

        println!("✓ Basic scanner functionality test passed");
        Ok(())
    }

    /// Test event processing
    pub async fn test_event_processing(&mut self) -> Result<(), ScannerError> {
        println!("Testing event processing...");

        let events = self.scanner.scan_new_blocks().await?;

        // Process any events found
        for event in events {
            match event {
                ReserveEvent::ReserveCreated {
                    box_id,
                    owner_pubkey: _,
                    collateral_amount,
                    height,
                } => {
                    println!(
                        "Mock reserve created: {} with {} nanoERG at height {}",
                        box_id, collateral_amount, height
                    );
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
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
                        "Mock reserve topped up: {} with additional {} nanoERG at height {}",
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
                        "Mock reserve redeemed: {} with {} nanoERG at height {}",
                        box_id, redeemed_amount, height
                    );
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                    assert!(redeemed_amount > 0, "Redeemed amount should be positive");
                }
                ReserveEvent::ReserveSpent { box_id, height } => {
                    println!("Mock reserve spent: {} at height {}", box_id, height);
                    assert!(!box_id.is_empty(), "Box ID should not be empty");
                }
            }
        }

        println!("✓ Event processing test passed");
        Ok(())
    }

    /// Test scanner state management
    pub async fn test_scanner_state(&self) -> Result<(), ScannerError> {
        println!("Testing scanner state management...");

        assert!(self.scanner.is_active(), "Scanner should be active");

        let last_scanned = self.scanner.last_scanned_height();
        assert!(
            last_scanned >= 0,
            "Last scanned height should be non-negative"
        );

        println!("✓ Scanner state management test passed");
        Ok(())
    }

    /// Run all simple integration tests
    pub async fn run_all_tests(&mut self) -> Result<(), ScannerError> {
        println!("Running simple integration tests...");

        self.test_basic_functionality().await?;
        self.test_event_processing().await?;
        self.test_scanner_state().await?;

        println!("✓ All simple integration tests passed!");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_integration_suite() {
        let mut test_suite = SimpleIntegrationTestSuite::new();

        // These tests should pass with the mock implementation
        let result = test_suite.run_all_tests().await;
        assert!(result.is_ok(), "Simple integration tests should pass");
    }

    #[tokio::test]
    async fn test_scanner_creation() {
        let test_suite = SimpleIntegrationTestSuite::new();

        // Scanner should be created successfully
        assert!(test_suite.scanner.is_active());
    }
}
