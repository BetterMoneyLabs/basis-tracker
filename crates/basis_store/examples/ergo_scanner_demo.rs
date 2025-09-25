//! Example demonstrating the ErgoScanner functionality

use basis_store::ergo_scanner::{ErgoScanner, NodeConfig};

fn main() {
    println!("=== Basis Ergo Scanner Demo ===\n");

    // Create node configuration
    let config = NodeConfig::default();
    
    // Create a new scanner
    let mut scanner = ErgoScanner::new(config);

    println!("Scanner created with configuration:");
    println!("  Node URL: {}", scanner.config().url);
    println!("  API Key: {}", scanner.config().api_key);
    println!("  Timeout: {} seconds", scanner.config().timeout_secs);
    println!();

    // Start scanning
    println!("Starting scanner...");
    scanner.start_scanning().unwrap();
    
    println!("Scanner is active: {}", scanner.is_active());
    println!("Current blockchain height: {}", scanner.get_current_height().unwrap());
    println!();

    // Demonstrate waiting for next block
    println!("Waiting for next block (simulated)...");
    scanner.wait_for_next_block().unwrap();
    println!("New block detected!");
    println!();

    // Stop scanning
    println!("Stopping scanner...");
    scanner.stop_scanning().unwrap();
    
    println!("Scanner is active: {}", scanner.is_active());
    println!();

    println!("=== Demo Complete ===");
}