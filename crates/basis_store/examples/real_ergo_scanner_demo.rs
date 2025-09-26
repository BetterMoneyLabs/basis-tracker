//! Example demonstrating real Ergo node integration

use basis_store::ergo_scanner::{ErgoScanner, NodeConfig};

#[tokio::main]
async fn main() {
    println!("=== Real Ergo Scanner Demo ===\n");

    // Create node configuration for a real Ergo node
    let config = NodeConfig {
        url: "http://213.239.193.208:9052".to_string(), // Public test node
        api_key: "".to_string(), // No API key needed for public node
        timeout_secs: 30,
        start_height: None,
        contract_template: None,
    };
    
    // Create a new scanner
    let mut scanner = ErgoScanner::new(config);

    println!("Scanner created with configuration:");
    println!("  Node URL: {}", scanner.config().url);
    println!("  API Key: {}", scanner.config().api_key);
    println!("  Timeout: {} seconds", scanner.config().timeout_secs);
    println!();

    // Start scanning
    println!("Starting scanner...");
    match scanner.start_scanning().await {
        Ok(_) => {
            println!("✓ Scanner started successfully");
            println!("Scanner is active: {}", scanner.is_active());
            
            // Get current blockchain height
            match scanner.get_current_height().await {
                Ok(height) => {
                    println!("✓ Current blockchain height: {}", height);
                    println!("✓ Last scanned height: {}", scanner.last_scanned_height());
                }
                Err(e) => {
                    println!("✗ Failed to get current height: {}", e);
                }
            }
            
            // Try to get unspent boxes (will be empty without contract template)
            match scanner.get_unspent_reserve_boxes().await {
                Ok(boxes) => {
                    println!("✓ Found {} unspent reserve boxes", boxes.len());
                    for (i, ergo_box) in boxes.iter().enumerate() {
                        println!("  Box {}: {} ({} nanoERG)", i + 1, &ergo_box.box_id[..16], ergo_box.value);
                    }
                }
                Err(e) => {
                    println!("✗ Failed to get unspent boxes: {}", e);
                }
            }
            
            // Scan for new blocks
            println!("\nScanning for new blocks...");
            match scanner.scan_new_blocks().await {
                Ok(events) => {
                    println!("✓ Found {} reserve events", events.len());
                    for event in events {
                        println!("  Event: {:?}", event);
                    }
                }
                Err(e) => {
                    println!("✗ Failed to scan blocks: {}", e);
                }
            }
            
            // Wait for next block (with timeout)
            println!("\nWaiting for next block (with 5s timeout)...");
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                scanner.wait_for_next_block()
            ).await {
                Ok(Ok(_)) => println!("✓ New block detected!"),
                Ok(Err(e)) => println!("✗ Error waiting for block: {}", e),
                Err(_) => println!("⚠ Timeout waiting for next block (this is expected)"),
            }
        }
        Err(e) => {
            println!("✗ Failed to start scanner: {}", e);
            println!("This might be due to network connectivity or node availability.");
        }
    }

    // Stop scanning
    println!("\nStopping scanner...");
    scanner.stop_scanning().unwrap();
    
    println!("Scanner is active: {}", scanner.is_active());
    println!();

    println!("=== Demo Complete ===");
}