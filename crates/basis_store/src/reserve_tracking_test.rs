//! Test for verifying reserve tracking functionality in the scanner

#[cfg(test)]
mod tests {
    use crate::ergo_scanner::{NodeConfig, ScanBox, ServerState};
    use crate::persistence::{ReserveStorage, ScannerMetadataStorage};
    use crate::{ExtendedReserveInfo, ReserveTracker};
    use tempfile::TempDir;

    /// Test that verifies the reserve tracking functionality
    /// This test simulates the process of scanning reserve boxes and updating the tracker
    #[tokio::test]
    async fn test_reserve_tracking_functionality() {
        // Create a temporary directory for test storage
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().join("scanner_metadata");

        // Create scanner metadata storage
        let metadata_storage = ScannerMetadataStorage::open(&storage_path)
            .expect("Failed to create scanner metadata storage");

        // Create a test configuration
        let config = NodeConfig {
            start_height: Some(0),
            reserve_contract_p2s: Some("test_reserve_contract_p2s".to_string()),
            node_url: "http://test-node:9053".to_string(),
            scan_name: Some("Test Reserve Scanner".to_string()),
            api_key: None,
        };

        // Create reserve storage
        let reserve_storage_path = temp_dir.path().join("reserves");
        let reserve_storage =
            ReserveStorage::open(&reserve_storage_path).expect("Failed to create reserve storage");

        // Create server state
        let state = ServerState::new(config).expect("Failed to create server state");

        // Create test scan boxes that simulate reserve boxes from the blockchain
        let test_scan_boxes = vec![
            ScanBox {
                box_id: "box1".to_string(), // Use plain text box_id
                value: 1000000000,          // 1 ERG
                ergo_tree: "test_ergo_tree_1".to_string(),
                creation_height: 1000,
                transaction_id: "tx1".to_string(),
                assets: vec![], // Empty assets for reserve boxes
                additional_registers: {
                    let mut registers = std::collections::HashMap::new();
                    // Use a valid hex-encoded compressed public key (33 bytes = 66 hex chars)
                    registers.insert("R4".to_string(), "026d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cb".to_string());
                    registers.insert("R6".to_string(), "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string()); // 32-byte tracker NFT ID (64 hex chars)
                    registers
                },
            },
            ScanBox {
                box_id: "box2".to_string(), // Use plain text box_id
                value: 2000000000,          // 2 ERG
                ergo_tree: "test_ergo_tree_2".to_string(),
                creation_height: 1001,
                transaction_id: "tx2".to_string(),
                assets: vec![], // Empty assets for reserve boxes
                additional_registers: {
                    let mut registers = std::collections::HashMap::new();
                    // Use a valid hex-encoded compressed public key (33 bytes = 66 hex chars)
                    registers.insert("R4".to_string(), "037d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cc".to_string());
                    registers.insert("R6".to_string(), "2bf34e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f30415".to_string()); // 32-byte tracker NFT ID (64 hex chars)
                    registers
                },
            },
        ];

        // Test parsing individual reserve boxes
        println!("Testing reserve box parsing...");

        for scan_box in &test_scan_boxes {
            match state.parse_reserve_box(scan_box) {
                Ok(reserve_info) => {
                    println!("Successfully parsed reserve box: {}", scan_box.box_id);
                    println!("  - Owner pubkey: {}", reserve_info.owner_pubkey);
                    println!(
                        "  - Collateral: {}",
                        reserve_info.base_info.collateral_amount
                    );
                    println!("  - Tracker NFT: {:?}", reserve_info.base_info.tracker_nft_id);

                    // Verify the parsed data matches expected values
                    assert_eq!(reserve_info.base_info.collateral_amount, scan_box.value);
                    assert_eq!(reserve_info.box_id, hex::encode(scan_box.box_id.as_bytes()));

                    // Check owner pubkey extraction
                    let expected_owner_pubkey = scan_box
                        .additional_registers
                        .get("R4")
                        .expect("R4 register should be present");
                    assert_eq!(
                        reserve_info.owner_pubkey,
                        *expected_owner_pubkey  // Already hex-encoded
                    );

                    // Check tracker NFT extraction (if present) - now comes from R6 register according to spec
                    if let Some(expected_tracker_nft) = scan_box.additional_registers.get("R6") {
                        assert_eq!(
                            reserve_info.base_info.tracker_nft_id,
                            *expected_tracker_nft  // Compare directly since we're using raw hex strings
                        );
                    } else {
                        // If no R6 register, the tracker_nft_id should be empty (handled by the parsing logic)
                    }
                }
                Err(e) => panic!("Failed to parse reserve box {}: {}", scan_box.box_id, e),
            }
        }

        // Test updating the reserve tracker
        println!("\nTesting reserve tracker updates...");

        let mut current_box_ids = Vec::new();
        for scan_box in &test_scan_boxes {
            match state.parse_reserve_box(scan_box) {
                Ok(reserve_info) => {
                    current_box_ids.push(reserve_info.box_id.clone());
                    state
                        .reserve_tracker
                        .update_reserve(reserve_info)
                        .expect("Failed to update reserve in tracker");
                    println!("Updated reserve in tracker: {}", scan_box.box_id);
                }
                Err(e) => panic!("Failed to parse reserve box for tracker update: {}", e),
            }
        }

        // Verify reserves are correctly tracked
        let all_reserves = state.reserve_tracker.get_all_reserves();
        println!("\nAll tracked reserves:");
        for reserve in &all_reserves {
            println!(
                "  - Reserve {}: collateral={}, owner={}",
                reserve.box_id, reserve.base_info.collateral_amount, reserve.owner_pubkey
            );
        }

        // Verify we have the expected number of reserves
        assert_eq!(
            all_reserves.len(),
            2,
            "Should have exactly 2 reserves tracked"
        );

        // Verify individual reserve details - use hex-encoded box IDs for lookups
        let reserve1 = state
            .reserve_tracker
            .get_reserve("626f7831")
            .expect("Reserve box1 should exist");
        assert_eq!(reserve1.base_info.collateral_amount, 1000000000);
        assert_eq!(
            reserve1.owner_pubkey,
            "026d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cb"
        );
        // Expected tracker NFT ID is now the 32-byte hex string we put in the R6 register
        let expected_tracker_nft_hex = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304"; // 32-byte tracker NFT ID
        assert_eq!(
            reserve1.base_info.tracker_nft_id,
            expected_tracker_nft_hex
        );

        let reserve2 = state
            .reserve_tracker
            .get_reserve("626f7832")
            .expect("Reserve box2 should exist");
        assert_eq!(reserve2.base_info.collateral_amount, 2000000000);
        assert_eq!(
            reserve2.owner_pubkey,
            "037d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cc"
        );
        // Since tracker_nft_id is now a required field in base_info, we can't have it as None anymore
        // The test should check that it's not empty or has the expected value
        assert!(!reserve2.base_info.tracker_nft_id.is_empty());

        // Test reserve removal (simulating spent reserves)
        println!("\nTesting reserve removal (spent reserves)...");

        // Remove box1 from current scan (simulating it being spent)
        let spent_box_id = "626f7831".to_string();
        state
            .reserve_tracker
            .remove_reserve(&spent_box_id)
            .expect("Failed to remove spent reserve");

        let remaining_reserves = state.reserve_tracker.get_all_reserves();
        assert_eq!(
            remaining_reserves.len(),
            1,
            "Should have 1 reserve after removal"
        );
        assert!(
            state.reserve_tracker.get_reserve("626f7831").is_err(),
            "box1 should be removed"
        );
        assert!(
            state.reserve_tracker.get_reserve("626f7832").is_ok(),
            "box2 should still exist"
        );

        println!("Reserve tracking test completed successfully!");
    }

    /// Test collateralization ratio calculations
    #[test]
    fn test_collateralization_ratios() {
        let tracker = ReserveTracker::new();

        // Create test reserves with different collateral amounts
        let reserve_info = ExtendedReserveInfo::new(
            b"test_box_1",
            b"owner_1",
            1000000000, // 1 ERG
            Some(b"nft_1"),
            1000,
        );

        tracker.update_reserve(reserve_info).unwrap();

        // Test adding debt
        tracker.add_debt("746573745f626f785f31", 500000000).unwrap(); // 0.5 ERG debt

        let reserve = tracker.get_reserve("746573745f626f785f31").unwrap();
        assert_eq!(reserve.total_debt, 500000000);
        assert_eq!(reserve.collateralization_ratio(), 2.0); // 100% / 50% = 2.0

        // Test warning level (80% utilization)
        assert!(!reserve.is_warning_level()); // 50% utilization, not at warning

        // Test critical level
        assert!(!reserve.is_critical_level()); // 50% utilization, not critical

        // Test sufficient collateral check
        assert!(reserve.is_sufficiently_collateralized(500000000)); // Can add another 0.5 ERG
        assert!(!reserve.is_sufficiently_collateralized(600000000)); // Cannot add 0.6 ERG more

        println!("Collateralization ratio tests completed successfully!");
    }

    /// Test reserve tracking with multiple operations
    #[tokio::test]
    async fn test_reserve_tracking_comprehensive() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().join("scanner_metadata");

        let metadata_storage = ScannerMetadataStorage::open(&storage_path)
            .expect("Failed to create scanner metadata storage");

        let config = NodeConfig {
            start_height: Some(0),
            reserve_contract_p2s: Some("test_reserve_contract_p2s".to_string()),
            node_url: "http://test:9053".to_string(),
            scan_name: Some("Test Scanner".to_string()),
            api_key: None,
        };

        // Create reserve storage for the second test
        let reserve_storage_path = temp_dir.path().join("reserves_comprehensive");
        let reserve_storage =
            ReserveStorage::open(&reserve_storage_path).expect("Failed to create reserve storage");

        let state = ServerState::new(config).expect("Failed to create server state");

        // Test multiple reserve operations
        let reserves = vec![
            ("reserve_a", 500000000, "owner_a", Some("nft_a")),
            ("reserve_b", 1000000000, "owner_b", None),
            ("reserve_c", 1500000000, "owner_c", Some("nft_c")),
        ];

        for (box_id, collateral, owner, nft) in reserves {
            let scan_box = ScanBox {
                box_id: box_id.to_string(),
                value: collateral,
                ergo_tree: "test_tree".to_string(),
                creation_height: 1000,
                transaction_id: "test_tx".to_string(),
                assets: vec![], // Empty assets for reserve boxes
                additional_registers: {
                    let mut registers = std::collections::HashMap::new();
                    // Use valid hex-encoded compressed public keys for each owner
                    let owner_key = match owner {
                        "owner_a" => "026d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cb",
                        "owner_b" => "037d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cc",
                        "owner_c" => "028d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cd",
                        _ => "026d5e27e6b7d3def910b39a3e0559500b728b025a9a85c66542e4f3e061e8a8cb", // default
                    };
                    registers.insert("R4".to_string(), owner_key.to_string());
                    // Always include R6 register with a 32-byte tracker NFT ID
                    let tracker_nft_id = match nft {
                        Some(nft_val) => {
                            // Generate a 32-byte hex string based on the nft value
                            let nft_bytes = hex::encode(nft_val.as_bytes());
                            // Pad or truncate to ensure exactly 64 hex chars (32 bytes)
                            format!("{:0<64}", nft_bytes.get(..64).unwrap_or(&nft_bytes))
                        },
                        None => "0000000000000000000000000000000000000000000000000000000000000000".to_string(), // 32 zero bytes
                    };
                    registers.insert("R6".to_string(), tracker_nft_id);
                    registers
                },
            };

            let reserve_info = state.parse_reserve_box(&scan_box).unwrap();
            state.reserve_tracker.update_reserve(reserve_info).unwrap();
        }

        // Verify all reserves are tracked
        let all_reserves = state.reserve_tracker.get_all_reserves();
        assert_eq!(all_reserves.len(), 3);

        // Test system totals
        let (total_collateral, total_debt) = state.reserve_tracker.get_system_totals();
        assert_eq!(total_collateral, 3000000000); // 3 ERG total
        assert_eq!(total_debt, 0); // No debt initially

        // Test adding debt to one reserve
        state
            .reserve_tracker
            .add_debt("726573657276655f61", 200000000)
            .unwrap();

        let (total_collateral_after, total_debt_after) = state.reserve_tracker.get_system_totals();
        assert_eq!(total_collateral_after, 3000000000);
        assert_eq!(total_debt_after, 200000000);

        println!("Comprehensive reserve tracking test completed successfully!");
    }
}
