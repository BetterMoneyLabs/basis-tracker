//! Reserve collateralization and tracking tests
//! 
//! Tests for reserve creation, debt tracking, collateralization ratios,
//! and alert thresholds (80% warning, 100% critical)

use basis_store::{
    ExtendedReserveInfo, ReserveTracker,
};
use hex;

#[tokio::test]
async fn test_reserve_creation_and_tracking() {
    let tracker = ReserveTracker::new();
    
    // Create a reserve with 1,000,000 nanoERG collateral
    let box_id = b"test_reserve_box_1";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        Some(b"tracker_nft_001"),
        1000, // height
        None,
        None,
    );
    
    // Add reserve to tracker
    let result = tracker.update_reserve(reserve_info.clone());
    assert!(result.is_ok(), "Reserve should be added successfully");
    
    // Retrieve and verify
    let retrieved = tracker.get_reserve(&hex::encode(box_id));
    assert!(retrieved.is_ok(), "Should retrieve reserve");
    
    let reserve = retrieved.unwrap();
    assert_eq!(reserve.base_info.collateral_amount, collateral);
    assert_eq!(reserve.total_debt, 0);
    assert_eq!(reserve.collateralization_ratio(), f64::INFINITY);
}

#[tokio::test]
async fn test_debt_accumulation_and_collateralization() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_box_2";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    
    // Add debt incrementally
    let box_id_hex = hex::encode(box_id);
    
    // Add 100,000 debt (10% utilization)
    tracker.add_debt(&box_id_hex, 100_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert_eq!(reserve.total_debt, 100_000);
    assert!((reserve.collateralization_ratio() - 10.0).abs() < 0.01);
    
    // Add another 200,000 debt (30% total utilization)
    tracker.add_debt(&box_id_hex, 200_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert_eq!(reserve.total_debt, 300_000);
    assert!((reserve.collateralization_ratio() - 3.33).abs() < 0.01);
    
    // Verify can support additional debt
    let can_support = tracker.can_support_debt(&box_id_hex, 100_000).unwrap();
    assert!(can_support, "Should support 100k more debt at 30% utilization");
}

#[tokio::test]
async fn test_warning_threshold_80_percent() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_warning";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Add debt to 75% - should NOT trigger warning
    tracker.add_debt(&box_id_hex, 750_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(!reserve.is_warning_level(), "75% should not trigger warning");
    
    // Add debt to 80% - should trigger warning
    tracker.add_debt(&box_id_hex, 50_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(reserve.is_warning_level(), "80% should trigger warning");
    assert_eq!(reserve.total_debt, 800_000);
    
    // Check warning reserves list
    let warning_reserves = tracker.get_warning_reserves();
    assert_eq!(warning_reserves.len(), 1);
    assert_eq!(warning_reserves[0].total_debt, 800_000);
}

#[tokio::test]
async fn test_critical_threshold_100_percent() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_critical";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Add debt to 95% - should be warning but not critical
    tracker.add_debt(&box_id_hex, 950_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(reserve.is_warning_level(), "95% should be warning");
    assert!(!reserve.is_critical_level(), "95% should not be critical");
    
    // Add debt to exactly 100% - should be critical
    tracker.add_debt(&box_id_hex, 50_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(reserve.is_critical_level(), "100% should be critical");
    assert_eq!(reserve.total_debt, 1_000_000);
    assert_eq!(reserve.collateralization_ratio(), 1.0);
    
    // Check critical reserves list
    let critical_reserves = tracker.get_critical_reserves();
    assert_eq!(critical_reserves.len(), 1);
    assert_eq!(critical_reserves[0].total_debt, 1_000_000);
}

#[tokio::test]
async fn test_undercollateralized_reserve_rejection() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_undercollateralized";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Fill to 100%
    tracker.add_debt(&box_id_hex, 1_000_000).unwrap();
    
    // Try to add more debt - should be rejected
    let result = tracker.add_debt(&box_id_hex, 1);
    assert!(result.is_err(), "Should reject debt beyond collateral");
    
    // Verify can_support_debt returns false
    let can_support = tracker.can_support_debt(&box_id_hex, 1).unwrap();
    assert!(!can_support, "Should not support additional debt at 100%");
}

#[tokio::test]
async fn test_multiple_reserves_for_issuer() {
    let tracker = ReserveTracker::new();
    
    let owner_pubkey = [0x02u8; 33];
    
    // Create 3 reserves with different collateral amounts
    let reserves = vec![
        (b"reserve_1".as_ref(), 1_000_000u64),
        (b"reserve_2".as_ref(), 2_000_000u64),
        (b"reserve_3".as_ref(), 500_000u64),
    ];
    
    for (box_id, collateral) in &reserves {
        let reserve_info = ExtendedReserveInfo::new(
            box_id,
            &owner_pubkey,
            *collateral,
            None,
            1000,
            None,
            None,
        );
        tracker.update_reserve(reserve_info).unwrap();
    }
    
    // Verify all reserves exist
    let all_reserves = tracker.get_all_reserves();
    assert_eq!(all_reserves.len(), 3);
    
    // Add debt to each reserve
    tracker.add_debt(&hex::encode(reserves[0].0), 500_000).unwrap();
    tracker.add_debt(&hex::encode(reserves[1].0), 1_500_000).unwrap();
    tracker.add_debt(&hex::encode(reserves[2].0), 400_000).unwrap();
    
    // Calculate total system collateral and debt
    let (total_collateral, total_debt) = tracker.get_system_totals();
    assert_eq!(total_collateral, 3_500_000);
    assert_eq!(total_debt, 2_400_000);
}

#[tokio::test]
async fn test_reserve_top_up_increases_capacity() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_topup";
    let owner_pubkey = [0x02u8; 33];
    let initial_collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        initial_collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Add debt to 80%
    tracker.add_debt(&box_id_hex, 800_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(reserve.is_warning_level());
    
    // Top up with additional 500,000 collateral
    tracker.update_collateral(&box_id_hex, 1_500_000).unwrap();
    
    // Verify warning level cleared
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert_eq!(reserve.base_info.collateral_amount, 1_500_000); // Fixed field access
    assert_eq!(reserve.total_debt, 800_000);
    assert!(!reserve.is_warning_level(), "After top-up, should not be at warning level");
    assert!((reserve.collateralization_ratio() - 1.875).abs() < 0.01);
}

#[tokio::test]
async fn test_debt_paydown_reduces_utilization() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_paydown";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Add debt to critical level
    tracker.add_debt(&box_id_hex, 1_000_000).unwrap();
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert!(reserve.is_critical_level());
    
    // Pay down 300,000
    tracker.remove_debt(&box_id_hex, 300_000).unwrap();
    
    // Verify debt reduced and no longer critical
    let reserve = tracker.get_reserve(&box_id_hex).unwrap();
    assert_eq!(reserve.total_debt, 700_000);
    assert!(!reserve.is_critical_level(), "After paydown, should not be critical");
    assert!(!reserve.is_warning_level(), "After paydown to 70%, should not be warning");
}

#[tokio::test]
async fn test_reserve_removal() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_reserve_removal";
    let owner_pubkey = [0x02u8; 33];
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        1_000_000,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    let box_id_hex = hex::encode(box_id);
    
    // Verify exists
    assert!(tracker.get_reserve(&box_id_hex).is_ok());
    
    // Remove reserve
    let result = tracker.remove_reserve(&box_id_hex);
    assert!(result.is_ok(), "Should remove reserve successfully");
    
    // Verify doesn't exist
    let result = tracker.get_reserve(&box_id_hex);
    assert!(result.is_err(), "Reserve should not exist after removal");
}

#[tokio::test]
async fn test_reserve_by_owner_lookup() {
    let tracker = ReserveTracker::new();
    
    let owner_pubkey = [0x03u8; 33];
    let box_id = b"test_reserve_owner_lookup";
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        1_000_000,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    
    // Lookup by owner pubkey
    let result = tracker.get_reserve_by_owner(&hex::encode(owner_pubkey));
    assert!(result.is_ok(), "Should find reserve by owner");
    
    let reserve = result.unwrap();
    assert_eq!(reserve.owner_pubkey, hex::encode(owner_pubkey));
    assert_eq!(reserve.base_info.collateral_amount, 1_000_000);
}

#[tokio::test]
async fn test_sufficient_collateralization_check() {
    let tracker = ReserveTracker::new();
    
    let box_id = b"test_sufficient_collateral";
    let owner_pubkey = [0x02u8; 33];
    let collateral = 1_000_000u64;
    
    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        None,
        None,
    );
    
    tracker.update_reserve(reserve_info.clone()).unwrap();
    
    // Test is_sufficiently_collateralized at various debt levels
    assert!(reserve_info.is_sufficiently_collateralized(500_000), "50% should be sufficient");
    assert!(reserve_info.is_sufficiently_collateralized(800_000), "80% should be sufficient");
    assert!(!reserve_info.is_sufficiently_collateralized(1_000_001), "100%+ should not be sufficient");
}
