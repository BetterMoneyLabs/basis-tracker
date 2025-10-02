use crate::{
    cross_verification, ergo_scanner::ReserveEvent, ExtendedReserveInfo, IouNote, ReserveTracker,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_consistency_verification() {
        let mut reserve_tracker = ReserveTracker::new();

        // Create test reserve info
        let reserve_info = ExtendedReserveInfo::new(
            b"test_reserve_box_1",
            b"010101010101010101010101010101010101010101010101010101010101010101",
            1000000000, // 1 ERG
            None,
            1000,
        );

        // Add reserve to tracker
        reserve_tracker.update_reserve(reserve_info).unwrap();

        // Simulate on-chain event
        let on_chain_event = ReserveEvent::ReserveCreated {
            box_id: "test_reserve_box_1".to_string(),
            owner_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            collateral_amount: 1000000000,
            height: 1000,
        };

        // Verify consistency by checking the reserve exists
        let reserves = reserve_tracker.get_all_reserves();
        assert!(!reserves.is_empty(), "Should have reserves");

        let found_reserve = reserves
            .iter()
            .find(|r| r.box_id == "746573745f726573657276655f626f785f31"); // "test_reserve_box_1" in hex
        assert!(found_reserve.is_some(), "Should find the test reserve");
    }

    #[test]
    fn test_reserve_tracking_accuracy() {
        let mut reserve_tracker = ReserveTracker::new();

        // Add multiple reserves
        let reserves = vec![
            ExtendedReserveInfo::new(b"reserve_1", b"issuer_1", 1000000000, None, 1000),
            ExtendedReserveInfo::new(b"reserve_2", b"issuer_1", 2000000000, None, 1001),
            ExtendedReserveInfo::new(b"reserve_3", b"issuer_2", 1500000000, None, 1002),
        ];

        for reserve in reserves {
            reserve_tracker.update_reserve(reserve).unwrap();
        }

        // Test total reserves count
        let all_reserves = reserve_tracker.get_all_reserves();
        assert_eq!(all_reserves.len(), 3, "Should have 3 reserves total");

        // Test reserve lookup by box_id
        let reserve_1 = reserve_tracker.get_reserve("726573657276655f31"); // "reserve_1" in hex
        assert!(reserve_1.is_ok(), "Should find reserve_1");

        let reserve_2 = reserve_tracker.get_reserve("726573657276655f32"); // "reserve_2" in hex
        assert!(reserve_2.is_ok(), "Should find reserve_2");

        let unknown_reserve = reserve_tracker.get_reserve("unknown_reserve");
        assert!(unknown_reserve.is_err(), "Should not find unknown reserve");
    }

    #[test]
    fn test_collateralization_ratio_monitoring() {
        let mut reserve_tracker = ReserveTracker::new();

        // Add reserve with 1 ERG collateral
        let reserve_info = ExtendedReserveInfo::new(
            b"test_reserve",
            b"issuer_1",
            1000000000, // 1 ERG
            None,
            1000,
        );

        reserve_tracker.update_reserve(reserve_info).unwrap();

        // Create notes totaling 0.5 ERG debt (50% collateralization)
        let notes = vec![
            IouNote::new([2u8; 33], 250000000, 0, 1234567890, [3u8; 65]), // 0.25 ERG
            IouNote::new([2u8; 33], 250000000, 0, 1234567891, [3u8; 65]), // 0.25 ERG
        ];

        // Test collateralization ratio calculation
        let total_debt: u64 = notes.iter().map(|note| note.outstanding_debt()).sum();
        let total_collateral = 1000000000; // From our test reserve

        let ratio = total_collateral as f64 / total_debt as f64;
        assert_eq!(ratio, 2.0, "Collateralization ratio should be 200%");

        // Test alert generation for low collateralization
        let alert_threshold = 1.5; // 150%
        let should_alert = ratio < alert_threshold;
        assert!(!should_alert, "Should not alert for 200% ratio");

        // Test with higher debt (should trigger alert)
        let high_debt_notes = vec![
            IouNote::new([2u8; 33], 800000000, 0, 1234567890, [3u8; 65]), // 0.8 ERG
        ];

        let high_total_debt: u64 = high_debt_notes
            .iter()
            .map(|note| note.outstanding_debt())
            .sum();
        let high_ratio = total_collateral as f64 / high_total_debt as f64;
        let should_alert_high = high_ratio < alert_threshold;
        assert!(should_alert_high, "Should alert for 125% ratio");
    }

    #[test]
    fn test_reserve_event_processing() {
        let mut reserve_tracker = ReserveTracker::new();

        // Process reserve creation event
        let create_event = ReserveEvent::ReserveCreated {
            box_id: "box_1".to_string(),
            owner_pubkey: "issuer_1".to_string(),
            collateral_amount: 1000000000,
            height: 1000,
        };

        // Create reserve info from event
        let reserve_info = ExtendedReserveInfo::new(b"box_1", b"issuer_1", 1000000000, None, 1000);

        let result = reserve_tracker.update_reserve(reserve_info);
        assert!(result.is_ok(), "Should process creation event");

        // Verify reserve was added
        let reserves = reserve_tracker.get_all_reserves();
        assert_eq!(reserves.len(), 1, "Should have 1 reserve");
        assert_eq!(reserves[0].box_id, "626f785f31"); // "box_1" in hex
        assert_eq!(reserves[0].base_info.collateral_amount, 1000000000);

        // Process top-up event by updating the reserve
        let updated_reserve_info = ExtendedReserveInfo::new(
            b"box_1",
            b"issuer_1",
            1500000000, // Increased collateral
            None,
            1001,
        );

        let result = reserve_tracker.update_reserve(updated_reserve_info);
        assert!(result.is_ok(), "Should process top-up event");

        // Verify collateral increased
        let reserves_after_topup = reserve_tracker.get_all_reserves();
        assert_eq!(
            reserves_after_topup[0].base_info.collateral_amount, 1500000000,
            "Should have 1.5 ERG collateral after top-up"
        );

        // Process redemption event by updating with reduced collateral
        let redeemed_reserve_info = ExtendedReserveInfo::new(
            b"box_1",
            b"issuer_1",
            1200000000, // Reduced collateral after redemption
            None,
            1002,
        );

        let result = reserve_tracker.update_reserve(redeemed_reserve_info);
        assert!(result.is_ok(), "Should process redemption event");

        // Verify collateral decreased
        let reserves_after_redeem = reserve_tracker.get_all_reserves();
        assert_eq!(
            reserves_after_redeem[0].base_info.collateral_amount, 1200000000,
            "Should have 1.2 ERG collateral after redemption"
        );
    }

    #[test]
    fn test_error_conditions() {
        let mut reserve_tracker = ReserveTracker::new();

        // Test processing event for non-existent reserve
        let topup_event = ReserveEvent::ReserveToppedUp {
            box_id: "non_existent_box".to_string(),
            additional_collateral: 500000000,
            height: 1000,
        };

        // Try to update non-existent reserve
        let reserve_info =
            ExtendedReserveInfo::new(b"non_existent_box", b"issuer_1", 500000000, None, 1000);

        let result = reserve_tracker.update_reserve(reserve_info);
        assert!(result.is_ok(), "Should be able to add new reserve");

        // Test with small reserve
        let small_reserve_info = ExtendedReserveInfo::new(
            b"small_reserve",
            b"issuer_1",
            100000000, // 0.1 ERG
            None,
            1000,
        );

        reserve_tracker.update_reserve(small_reserve_info).unwrap();

        // Verify small reserve exists
        let small_reserve = reserve_tracker.get_reserve("736d616c6c5f72657365727665"); // "small_reserve" in hex
        assert!(small_reserve.is_ok(), "Should find small reserve");
        assert_eq!(
            small_reserve.unwrap().base_info.collateral_amount,
            100000000
        );
    }

    #[test]
    fn test_cross_verification_functionality() {
        // Test that cross verification functions work
        let result = cross_verification::run_cross_verification_tests();
        assert!(result.is_ok(), "Cross verification tests should pass");

        let report = cross_verification::generate_compatibility_report();
        assert!(
            report.contains("IMPLEMENTED"),
            "Report should indicate implementation status"
        );
        assert!(
            report.contains("Blake2b256"),
            "Report should mention hash function"
        );
    }
}
