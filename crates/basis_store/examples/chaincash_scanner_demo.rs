//! Example demonstrating the chaincash-rs style scanner for Basis tracker

use basis_store::ergo_scanner::{create_default_scanner, start_scanner, ScannerError};

#[tokio::main]
async fn main() -> Result<(), ScannerError> {
    println!("Starting Basis tracker scanner (chaincash-rs style)...");

    // Create a scanner with default configuration
    let state = create_default_scanner();

    // Start the scanner
    println!("Starting scanner...");
    start_scanner(state).await?;

    println!("Scanner started successfully!");

    // The scanner runs in background tasks, so we need to keep the main thread alive
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    println!("Demo completed");
    Ok(())
}
