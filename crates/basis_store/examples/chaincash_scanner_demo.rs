//! Example demonstrating the Ergo scanner for Basis tracker

use basis_store::ergo_scanner::ergo_scanner::create_ergo_scanner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Basis tracker Ergo scanner...");

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

    println!("\nDemo completed");

    // Cleanup scanner
    scanner.cleanup().await?;

    Ok(())
}
