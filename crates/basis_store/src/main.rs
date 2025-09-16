//! Manual test runner for Basis Store

use basis_store::tests::run_all_tests;

fn main() {
    println!("Basis Store Manual Test Runner");
    
    match run_all_tests() {
        Ok(()) => {
            println!("\n✅ All tests completed successfully!");
        }
        Err(e) => {
            println!("\n❌ Test failed: {}", e);
            std::process::exit(1);
        }
    }
}