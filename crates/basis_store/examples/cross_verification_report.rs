//! Example: Generate cross-verification report for Schnorr signature compatibility

use basis_store::cross_verification;

fn main() {
    println!("Generating Basis Schnorr Signature Compatibility Report...\n");
    
    // Generate the compatibility report
    let report = cross_verification::generate_compatibility_report();
    println!("{}", report);
    
    // Run cross-verification tests
    println!("Running cross-verification tests...");
    match cross_verification::run_cross_verification_tests() {
        Ok(()) => println!("✅ All tests passed! The implementation is compatible with basis.es"),
        Err(e) => println!("❌ Cross-verification failed: {}", e),
    }
    
    // Export test vectors
    println!("\nExporting test vectors for cross-language testing...");
    match cross_verification::export_test_vectors_to_file("basis_schnorr_test_vectors.json") {
        Ok(()) => println!("✅ Test vectors exported to basis_schnorr_test_vectors.json"),
        Err(e) => println!("❌ Failed to export test vectors: {}", e),
    }
}