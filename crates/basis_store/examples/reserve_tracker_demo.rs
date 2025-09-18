//! Example demonstrating the ReserveTracker functionality

use basis_store::reserve_tracker::{ExtendedReserveInfo, ReserveTracker};

fn main() {
    println!("=== Basis Reserve Tracker Demo ===\n");

    // Create a new reserve tracker
    let tracker = ReserveTracker::new();

    // Create some test reserves
    let reserve1 = ExtendedReserveInfo::new(
        b"reserve_box_id_1_abcdefgh",
        b"owner_pubkey_1_1234567890123456",
        1000000000, // 1 ERG collateral
        Some(b"tracker_nft_1_1234567890"),
        1000,
    );

    let reserve2 = ExtendedReserveInfo::new(
        b"reserve_box_id_2_ijklmnop",
        b"owner_pubkey_2_1234567890123456", 
        2000000000, // 2 ERG collateral
        Some(b"tracker_nft_2_1234567890"),
        1001,
    );

    // Add reserves to tracker
    tracker.update_reserve(reserve1.clone()).unwrap();
    tracker.update_reserve(reserve2.clone()).unwrap();

    println!("Added 2 reserves to tracker:");
    println!("1. {} with {} nanoERG", reserve1.owner_pubkey, reserve1.base_info.collateral_amount);
    println!("2. {} with {} nanoERG", reserve2.owner_pubkey, reserve2.base_info.collateral_amount);
    println!();

    // Add some debt to the first reserve
    println!("Adding 0.5 ERG debt to reserve 1...");
    tracker.add_debt(&reserve1.box_id, 500000000).unwrap();

    let updated_reserve1 = tracker.get_reserve(&reserve1.box_id).unwrap();
    println!("Reserve 1 now has {} nanoERG debt", updated_reserve1.total_debt);
    println!("Collateralization ratio: {:.2}", updated_reserve1.collateralization_ratio());
    println!();

    // Try to add too much debt
    println!("Trying to add 0.6 ERG more debt to reserve 1...");
    match tracker.add_debt(&reserve1.box_id, 600000000) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("Expected error: {}", e),
    }
    println!();

    // Check system totals
    let (total_collateral, total_debt) = tracker.get_system_totals();
    println!("System totals:");
    println!("  Total collateral: {} nanoERG", total_collateral);
    println!("  Total debt: {} nanoERG", total_debt);
    println!("  System collateralization ratio: {:.2}", total_collateral as f64 / total_debt as f64);
    println!();

    // Get warning and critical reserves
    let warning_reserves = tracker.get_warning_reserves();
    let critical_reserves = tracker.get_critical_reserves();

    println!("Warning reserves (<= 125% collateralization): {}", warning_reserves.len());
    println!("Critical reserves (<= 100% collateralization): {}", critical_reserves.len());
    println!();

    // Demonstrate reserve lookup by owner
    println!("Looking up reserve by owner public key...");
    let found_reserve = tracker.get_reserve_by_owner(&reserve2.owner_pubkey).unwrap();
    println!("Found reserve: {} with {} nanoERG", found_reserve.owner_pubkey, found_reserve.base_info.collateral_amount);
    println!();

    println!("=== Demo Complete ===");
}