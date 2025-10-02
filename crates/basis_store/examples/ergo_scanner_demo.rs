//! Demo of Ergo scanner

use basis_store::ergo_scanner::ergo_scanner::create_ergo_scanner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    println!("=== Ergo Scanner Demo ===");

    // Example contract template hash (replace with actual Basis reserve contract hash)
    let contract_template_hash = "example_contract_template_hash_here";
    
    // Example node URL (replace with actual Ergo node URL)
    let node_url = "http://localhost:9053";
    
    // Create Ergo scanner
    let mut scanner = create_ergo_scanner(node_url, "basis_reserves", contract_template_hash);
    
    println!("Initializing Ergo scanner...");
    
    // Initialize scanner (registers scan with node)
    if let Err(e) = scanner.initialize().await {
        println!("Failed to initialize scanner: {}", e);
        println!("Note: This demo requires a running Ergo node with scan API enabled");
        return Ok(());
    }
    
    println!("Scanner initialized successfully!");
    println!("Scan ID: {:?}", scanner.scan_id);
    
    // Perform initial scan
    println!("\nPerforming initial scan...");
    match scanner.scan_new_events().await {
        Ok(events) => {
            println!("Found {} events:", events.len());
            for event in events {
                println!("  - {:?}", event);
            }
        }
        Err(e) => {
            println!("Scan failed: {}", e);
        }
    }
    
    // Get unspent boxes
    println!("\nGetting unspent reserve boxes...");
    match scanner.get_unspent_reserve_boxes().await {
        Ok(boxes) => {
            println!("Found {} unspent boxes:", boxes.len());
            for box_ in boxes.iter().take(5) { // Show first 5 boxes
                println!("  - Box ID: {}, Value: {} nanoERG", box_.box_id, box_.value);
            }
            if boxes.len() > 5 {
                println!("  ... and {} more", boxes.len() - 5);
            }
        }
        Err(e) => {
            println!("Failed to get unspent boxes: {}", e);
        }
    }
    
    println!("\n=== Demo Complete ===");
    println!("Ergo Scanner Features:");
    println!("1. Uses /scan API for efficient box tracking");
    println!("2. Uses /blockchain API for optimized queries");
    println!("3. No block-by-block transaction scanning");
    println!("4. Scan persistence across restarts");
    println!("5. Bulk unspent box retrieval");
    
    Ok(())
}