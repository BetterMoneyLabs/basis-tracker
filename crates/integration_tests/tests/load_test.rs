use basis_store::{ExtendedReserveInfo, ReserveTracker};
use std::time::Instant;

#[test]
fn test_tracker_load_performance() {
    let tracker = ReserveTracker::new();
    let num_reserves = 10_000;
    
    println!("Generating {} reserves...", num_reserves);
    let start_gen = Instant::now();
    for i in 0..num_reserves {
        // Construct unique IDs
        let box_id = format!("{:064x}", i).into_bytes();
        let owner = format!("{:066x}", i).into_bytes(); // 33 bytes
        
        let reserve = ExtendedReserveInfo::new(
            &box_id,
            &owner, // Slice of vector
            1_000_000_000,
            None,
            1000,
            None,
            None,
        );
        tracker.update_reserve(reserve).expect("Failed to insert reserve");
    }
    println!("Generation took: {:?}", start_gen.elapsed());
    
    // Test Lookup Speed
    println!("Testing lookup speed...");
    let start_lookup = Instant::now();
    for i in (0..num_reserves).step_by(100) {
        let box_id = format!("{:064x}", i);
        let _ = tracker.get_reserve(&box_id).expect("Failed to lookup");
    }
    let duration_lookup = start_lookup.elapsed();
    println!("Lookup (100 random samples) took: {:?}", duration_lookup);
    
    // Assert acceptable performance (e.g., lookups should be sub-millisecond on average in memory)
    // 100 lookups should take way less than 1 second.
    assert!(duration_lookup.as_millis() < 1000, "Lookup too slow!");
    
    // Test Aggregation Speed (System Totals)
    let start_agg = Instant::now();
    let (total_collateral, total_debt) = tracker.get_system_totals();
    println!("Aggregation took: {:?}", start_agg.elapsed());
    
    assert_eq!(total_collateral, num_reserves as u64 * 1_000_000_000);
    assert_eq!(total_debt, 0);
}
