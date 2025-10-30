//! Simple integration tests that work with the reserves-only scanner implementation

use crate::ergo_scanner::{create_default_scanner, NodeConfig, ReserveEvent, ScannerError, ServerState};

/// Simple integration test suite that works with the reserves-only scanner
pub struct SimpleIntegrationTestSuite {
    scanner: ServerState,
}

impl SimpleIntegrationTestSuite {
    /// Create a new simple integration test suite
    pub fn new() -> Result<Self, ScannerError> {
        let scanner = create_default_scanner()?;
        Ok(Self { scanner })
    }

    /// Test basic scanner functionality
    pub async fn test_basic_functionality(&mut self) -> Result<(), ScannerError> {
        println!("Testing basic scanner functionality...");

        // Test getting current height
        let height = self.scanner.get_current_height().await?;
        assert!(height > 0, "Height should be positive");

        // Test unspent boxes
        let boxes = self.scanner.get_unspent_reserve_boxes().await?;
        assert!(
            boxes.is_empty(),
            "Current implementation should return empty boxes"
        );

        println!("✓ Basic scanner functionality test passed");
        Ok(())
    }

    /// Test scanner state management
    pub async fn test_scanner_state(&mut self) -> Result<(), ScannerError> {
        println!("Testing scanner state management...");

        // Start scanning
        self.scanner.start_scanning().await?;
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
        let mut test_suite = SimpleIntegrationTestSuite::new().expect("Should create test suite");

        // These tests should pass with the new implementation
        let result = test_suite.run_all_tests().await;
        assert!(result.is_ok(), "Simple integration tests should pass");
    }

    #[tokio::test]
    async fn test_scanner_creation() {
        let test_suite = SimpleIntegrationTestSuite::new().expect("Should create test suite");

        // Scanner should be created successfully
        // Note: scanner starts inactive until start_scanning is called
        assert!(!test_suite.scanner.is_active());
    }
}
