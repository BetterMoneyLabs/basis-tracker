//! Example demonstrating the ErgoScanner functionality

use basis_store::ergo_scanner::{ErgoScanner, NodeConfig};

#[tokio::main]
async fn main() {
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
    scanner.start_scanning().await.unwrap();
    
    println!("Scanner is active: {}", scanner.is_active());
    println!("Current blockchain height: {}", scanner.get_current_height().await.unwrap());
    println!();

    // Demonstrate waiting for next block (with timeout)
    println!("Waiting for next block (with 5s timeout)...");
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        scanner.wait_for_next_block()
    ).await {
        Ok(Ok(_)) => println!("New block detected!"),
        Ok(Err(e)) => println!("Error waiting for block: {}", e),
        Err(_) => println!("Timeout waiting for block (this is expected)"),
    }
    println!();

    // Stop scanning
    println!("Stopping scanner...");
    scanner.stop_scanning().unwrap();
    
    println!("Scanner is active: {}", scanner.is_active());
    println!();

    println!("=== Demo Complete ===");
}