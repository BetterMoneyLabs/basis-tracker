//! Example demonstrating real ErgoScanner functionality with actual Ergo node

use basis_store::ergo_scanner::{ErgoScanner, NodeConfig};

fn main() {
    println!("=== Real Ergo Scanner Demo ===\n");

    // Create node configuration for the test node
    let config = NodeConfig {
        url: "http://213.239.193.208:9052".to_string(),
        api_key: "".to_string(),
        timeout_secs: 30,
        start_height: Some(1000000), // Start from a reasonable height
        contract_template: None, // No specific contract template for demo
    };
    
    // Create a new scanner
    let mut scanner = ErgoScanner::new(config);

    println!("Scanner created with configuration:");
    println!("  Node URL: {}", scanner.config().url);
    println!("  API Key: {}", scanner.config().api_key);
    println!("  Timeout: {} seconds", scanner.config().timeout_secs);
    println!("  Start Height: {:?}", scanner.config().start_height);
    println!("  Contract Template: {:?}", scanner.config().contract_template);
    println!();

    // Start scanning
    println!("Starting scanner...");
    match scanner.start_scanning() {
        Ok(_) => {
            println!("✓ Scanner started successfully");
            println!("Scanner is active: {}", scanner.is_active());
            
            // Get current height
            match scanner.get_current_height() {
                Ok(height) => {
                    println!("✓ Current blockchain height: {}", height);
                    println!("✓ Last scanned height: {}", scanner.last_scanned_height());
                }
                Err(e) => {
                    println!("✗ Failed to get current height: {}", e);
                }
            }

            // Try to scan new blocks
            println!("\nScanning for new blocks...");
            match scanner.scan_new_blocks() {
                Ok(events) => {
                    println!("✓ Scan completed. Found {} events", events.len());
                    if !events.is_empty() {
                        for event in events {
                            println!("  - {:?}", event);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to scan blocks: {}", e);
                }
            }

            // Try to get unspent reserve boxes
            println!("\nGetting unspent reserve boxes...");
            match scanner.get_unspent_reserve_boxes() {
                Ok(boxes) => {
                    println!("✓ Found {} unspent boxes", boxes.len());
                    if !boxes.is_empty() {
                        for ergo_box in boxes.iter().take(3) { // Show first 3
                            println!("  - Box ID: {}, Value: {} nanoERG", ergo_box.box_id, ergo_box.value);
                        }
                        if boxes.len() > 3 {
                            println!("  ... and {} more boxes", boxes.len() - 3);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to get unspent boxes: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to start scanner: {}", e);
            println!("This is expected if the Ergo node is not accessible");
        }
    }
    
    println!();

    // Stop scanning
    println!("Stopping scanner...");
    match scanner.stop_scanning() {
        Ok(_) => {
            println!("✓ Scanner stopped successfully");
            println!("Scanner is active: {}", scanner.is_active());
        }
        Err(e) => {
            println!("✗ Failed to stop scanner: {}", e);
        }
    }
    
    println!();
    println!("=== Demo Complete ===");
}