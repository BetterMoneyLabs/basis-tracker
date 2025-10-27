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

    println!("Starting Ergo scanner...");

    // Start scanner (registers scan with node)
    if let Err(e) = scanner.start_scanning().await {
        println!("Failed to start scanner: {}", e);
        println!("Note: This demo requires a running Ergo node with scan API enabled");
        return Ok(());
    }

    println!("Scanner started successfully!");
    println!("Scan ID: {:?}", scanner.scan_id);
    println!("Last scanned height: {}", scanner.last_scanned_height());

    println!("\n=== Demo Complete ===");
    println!("Ergo Scanner Features:");
    println!("1. Uses /scan API for efficient box tracking");
    println!("2. Uses /blockchain API for optimized queries");
    println!("3. No block-by-block transaction scanning");
    println!("4. Scan persistence across restarts");
    println!("5. Bulk unspent box retrieval");

    // Cleanup scanner
    scanner.cleanup().await?;

    Ok(())
}
