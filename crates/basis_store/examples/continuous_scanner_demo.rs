//! Example demonstrating continuous blockchain scanning with ErgoScanner

use basis_store::ergo_scanner::{ErgoScanner, NodeConfig};
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("=== Continuous Ergo Scanner Demo ===\n");

    // Create node configuration for the test node
    let config = NodeConfig {
        url: "http://213.239.193.208:9052".to_string(),
        api_key: "".to_string(),
        timeout_secs: 30,
        start_height: None, // Start from current height
        contract_template: None,
    };
    
    // Create a new scanner
    let mut scanner = ErgoScanner::new(config);

    println!("Starting continuous scanner...");
    
    // Start scanning
    if let Err(e) = scanner.start_scanning().await {
        println!("✗ Failed to start scanner: {}", e);
        println!("This is expected if the Ergo node is not accessible");
        return;
    }

    println!("✓ Scanner started successfully");
    
    // Get initial height
    let initial_height = match scanner.get_current_height().await {
        Ok(height) => height,
        Err(e) => {
            println!("✗ Failed to get initial height: {}", e);
            return;
        }
    };
    
    println!("Initial blockchain height: {}", initial_height);
    println!("Starting continuous monitoring...\n");

    // Continuous monitoring loop
    let mut iteration = 0;
    loop {
        iteration += 1;
        
        println!("=== Iteration {} ===", iteration);
        
        // Check current height
        match scanner.get_current_height().await {
            Ok(current_height) => {
                println!("Current height: {} (last scanned: {})", 
                        current_height, scanner.last_scanned_height());
                
                // Scan for new blocks
                match scanner.scan_new_blocks().await {
                    Ok(events) => {
                        if !events.is_empty() {
                            println!("✓ Found {} new events:", events.len());
                            for event in events {
                                println!("  - {:?}", event);
                            }
                        } else {
                            println!("✓ No new events found");
                        }
                    }
                    Err(e) => {
                        println!("✗ Scan error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Height check error: {}", e);
            }
        }
        
        println!();
        
        // Wait before next iteration
        println!("Waiting 30 seconds before next scan...\n");
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        // Break after a few iterations for demo purposes
        if iteration >= 3 {
            println!("Demo complete after {} iterations", iteration);
            break;
        }
    }
    
    // Stop scanning
    println!("Stopping scanner...");
    if let Err(e) = scanner.stop_scanning() {
        println!("✗ Failed to stop scanner: {}", e);
    } else {
        println!("✓ Scanner stopped successfully");
    }
    
    println!("\n=== Demo Complete ===");
}