//! Example demonstrating complete integration between ReserveTracker and ErgoScanner

use basis_store::{
    ergo_scanner::{ErgoScanner, NodeConfig, ReserveEvent},
    reserve_tracker::{ExtendedReserveInfo, ReserveTracker},
};

fn main() {
    println!("=== Basis Full Integration Demo ===\n");

    // Create reserve tracker
    let reserve_tracker = ReserveTracker::new();

    // Create Ergo scanner
    let node_config = NodeConfig::default();
    let mut ergo_scanner = ErgoScanner::new(node_config);

    println!("1. Starting Ergo scanner...");
    ergo_scanner.start_scanning().unwrap();
    println!("   Scanner active: {}", ergo_scanner.is_active());
    println!();

    println!("2. Processing mock blockchain events...");
    let events = ergo_scanner.scan_new_blocks().unwrap();

    for event in events {
        match event {
            ReserveEvent::ReserveCreated {
                box_id,
                owner_pubkey,
                collateral_amount,
                height,
            } => {
                println!("   📦 New reserve created:");
                println!("      Box ID: {}", box_id);
                println!("      Owner: {}", owner_pubkey);
                println!("      Collateral: {} nanoERG", collateral_amount);
                println!("      Height: {}", height);

                // Add to reserve tracker
                let reserve_info = ExtendedReserveInfo::new(
                    box_id.as_bytes(),
                    owner_pubkey.as_bytes(),
                    collateral_amount,
                    None, // No tracker NFT
                    height,
                );

                reserve_tracker.update_reserve(reserve_info).unwrap();
                println!("      ✅ Added to reserve tracker");
            }
            ReserveEvent::ReserveToppedUp {
                box_id,
                additional_collateral,
                height,
            } => {
                println!("   💰 Reserve topped up:");
                println!("      Box ID: {}", box_id);
                println!("      Additional: {} nanoERG", additional_collateral);
                println!("      Height: {}", height);

                // Update collateral in tracker
                if let Ok(mut reserve) = reserve_tracker.get_reserve(&box_id) {
                    let new_collateral =
                        reserve.base_info.collateral_amount + additional_collateral;
                    reserve.base_info.collateral_amount = new_collateral;
                    reserve_tracker.update_reserve(reserve).unwrap();
                    println!("      ✅ Collateral updated");
                }
            }
            _ => println!("   Other event: {:?}", event),
        }
        println!();
    }

    println!("3. Current reserve tracker state:");
    let all_reserves = reserve_tracker.get_all_reserves();
    println!("   Total reserves: {}", all_reserves.len());

    for reserve in &all_reserves {
        println!("   ───────────────────────────");
        println!("   Reserve: {}", reserve.box_id);
        println!("   Owner: {}", reserve.owner_pubkey);
        println!(
            "   Collateral: {} nanoERG",
            reserve.base_info.collateral_amount
        );
        println!("   Debt: {} nanoERG", reserve.total_debt);
        println!("   Ratio: {:.2}", reserve.collateralization_ratio());

        if reserve.is_warning_level() {
            println!("   ⚠️  WARNING: Low collateralization");
        }
        if reserve.is_critical_level() {
            println!("   🚨 CRITICAL: Very low collateralization");
        }
    }
    println!();

    println!("4. Adding some debt to test risk monitoring...");
    if let Some(reserve) = all_reserves.first() {
        let box_id = &reserve.box_id;

        // Add debt to test warning system
        match reserve_tracker.add_debt(box_id, 800000000) {
            // 0.8 ERG debt
            Ok(_) => println!("   ✅ Added 0.8 ERG debt to reserve"),
            Err(e) => println!("   ❌ Failed to add debt: {}", e),
        }

        // Check updated state
        let updated = reserve_tracker.get_reserve(box_id).unwrap();
        println!(
            "   New collateralization ratio: {:.2}",
            updated.collateralization_ratio()
        );

        if updated.is_warning_level() {
            println!("   ⚠️  Reserve is now at warning level");
        }
    }
    println!();

    println!("5. System-wide statistics:");
    let (total_collateral, total_debt) = reserve_tracker.get_system_totals();
    println!("   Total collateral: {} nanoERG", total_collateral);
    println!("   Total debt: {} nanoERG", total_debt);
    println!(
        "   System ratio: {:.2}",
        total_collateral as f64 / (total_debt as f64).max(1.0)
    );
    println!();

    println!("6. Stopping scanner...");
    ergo_scanner.stop_scanning().unwrap();
    println!("   Scanner active: {}", ergo_scanner.is_active());
    println!();

    println!("=== Demo Complete ===");
}
