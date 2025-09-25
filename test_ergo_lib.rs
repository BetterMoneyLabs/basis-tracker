// Test to see if we can use ergo-lib
fn main() {
    println!("Testing ergo-lib availability...");
    
    // Try to use some ergo-lib types to see if it's available
    #[cfg(feature = "ergo-lib")]
    {
        use ergo_lib::chain;
        println!("ergo-lib is available!");
    }
    
    #[cfg(not(feature = "ergo-lib"))]
    {
        println!("ergo-lib is not available as a feature");
    }
}