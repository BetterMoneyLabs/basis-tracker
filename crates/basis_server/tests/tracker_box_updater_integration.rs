#[cfg(test)]
mod integration_tests {
    use basis_server::{SharedTrackerState, TrackerBoxUpdateConfig, TrackerBoxUpdater};

    #[tokio::test]
    async fn test_tracker_box_updater_integration() {
        // Create shared state with some test values
        let shared_state = SharedTrackerState::new();

        // Set some test values
        let test_root = [0x11u8; 33]; // Test AVL root digest (33 bytes)
        let test_pubkey = [0x02u8; 33]; // Test compressed public key (33 bytes)
        shared_state.set_avl_root_digest(test_root);
        shared_state.set_tracker_pubkey(test_pubkey);

        // Verify the values were set correctly
        assert_eq!(shared_state.get_avl_root_digest(), test_root);
        assert_eq!(shared_state.get_tracker_pubkey(), test_pubkey);

        // Test creating and starting the updater
        let config = TrackerBoxUpdateConfig::default();

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        // Start the updater in a background task
        let updater_handle = tokio::spawn(async move {
            TrackerBoxUpdater::start(config, shared_state, shutdown_rx, None).await
        });

        // Give it a moment to start, then send shutdown
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = shutdown_tx.send(());

        // Wait for the updater to finish with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), updater_handle).await;

        // Should complete without error
        assert!(result.is_ok(), "Updater should complete within timeout");
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok(), "Updater task should succeed");
        let updater_result = inner_result.unwrap();
        assert!(updater_result.is_ok(), "Updater should return Ok");
    }

    #[tokio::test]
    async fn test_tracker_box_updates_avl_digest() {
        use basis_store::{IouNote, TrackerStateManager};
        use secp256k1::{Secp256k1, SecretKey};

        // Create shared state
        let shared_state = SharedTrackerState::new();

        // Create a test tracker and add a note to update the AVL tree
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Generate a valid keypair for testing
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let issuer_pubkey_obj = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = issuer_pubkey_obj.serialize();

        // Create a test recipient pubkey
        let recipient_pubkey = [0x03u8; 33]; // Valid compressed public key

        // Create a properly signed test note
        let note = IouNote::create_and_sign(
            recipient_pubkey,
            1000,       // amount collected
            1234567890, // timestamp
            &secret_key.secret_bytes(),
        )
        .expect("Should be able to create a valid signed note");

        // Add the note to the tracker
        let result = tracker.add_note(&issuer_pubkey, &note);
        assert!(
            result.is_ok(),
            "Adding note to tracker should succeed: {:?}",
            result.err()
        );

        // Get the new AVL root digest after the update
        let new_root = tracker.get_state().avl_root_digest;

        // Update the shared state to match
        shared_state.set_avl_root_digest(new_root);

        // Verify that the shared state was updated
        assert_eq!(shared_state.get_avl_root_digest(), new_root);
        assert_ne!(shared_state.get_avl_root_digest(), [0u8; 33]); // Should not be all zeros
    }

    #[tokio::test]
    async fn test_shared_tracker_state_nft_id() {
        let shared_state = SharedTrackerState::new();

        // Initially no NFT ID
        assert!(shared_state.get_tracker_nft_id().is_none());

        // Set NFT ID
        shared_state.set_tracker_nft_id("test_nft_123".to_string());
        assert_eq!(
            shared_state.get_tracker_nft_id(),
            Some("test_nft_123".to_string())
        );
    }

    #[tokio::test]
    async fn test_shared_tracker_state_box_id() {
        let shared_state = SharedTrackerState::new();

        // Initially no box ID
        assert!(shared_state.get_tracker_box_id().is_none());

        // Set box ID
        shared_state.set_tracker_box_id("box_123".to_string());
        assert_eq!(
            shared_state.get_tracker_box_id(),
            Some("box_123".to_string())
        );
    }

    #[tokio::test]
    async fn test_transaction_confirmation_check_not_found() {
        // Test that check_transaction_confirmation handles a non-existent tx gracefully
        // Using a local/mock URL that will definitely timeout/fail
        let config = TrackerBoxUpdateConfig {
            node_url: "http://localhost:99999".to_string(), // Invalid port - will fail fast
            ..Default::default()
        };

        // Check a non-existent transaction ID (64 hex chars)
        let fake_tx_id = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = TrackerBoxUpdater::check_transaction_confirmation(&config, fake_tx_id).await;

        // Should return an error since the node is unreachable
        assert!(result.is_err(), "Should error when node is unreachable");
    }
}

/// Tests for `confirmation_depth` — the gate that decides whether a pending tracker-box
/// update may promote notes to `Confirmed`.
///
/// This function previously had no test coverage at all, despite being the only thing standing
/// between a mempool-accepted transaction and confirmed on-chain state (see
/// specs/PRODUCTION_READINESS_AUDIT.md). All heights are inclusive, so a transaction included in
/// the current tip has depth 1.
mod confirmation_depth_tests {
    use basis_server::confirmation_depth;

    #[test]
    fn test_depth_is_one_at_the_tip() {
        assert_eq!(
            confirmation_depth(1000, 1000),
            1,
            "a transaction in the current tip must have depth 1, not 0"
        );
    }

    #[test]
    fn test_depth_increments_with_each_new_block() {
        assert_eq!(confirmation_depth(1001, 1000), 2);
        assert_eq!(confirmation_depth(1002, 1000), 3);
        assert_eq!(confirmation_depth(1010, 1000), 11);
    }

    #[test]
    fn test_zero_inclusion_height_means_unknown() {
        // Height 0 is the sentinel for "we do not know the inclusion height", so depth must be
        // 0 — never a computed value that could accidentally satisfy a min_depth threshold.
        assert_eq!(confirmation_depth(1000, 0), 0);
        assert_eq!(confirmation_depth(0, 0), 0);
        assert_eq!(confirmation_depth(u64::MAX, 0), 0);
    }

    #[test]
    fn test_default_min_depth_of_two_is_not_met_at_tip() {
        // This is the actual production default (config.rs::default_min_confirmation_depth).
        const MIN_DEPTH: u64 = 2;
        let depth_at_tip = confirmation_depth(1000, 1000);
        assert!(
            depth_at_tip < MIN_DEPTH,
            "a freshly included tx must NOT satisfy min_depth=2, got depth {}",
            depth_at_tip
        );
        assert!(
            confirmation_depth(1001, 1000) >= MIN_DEPTH,
            "one confirmation later the tx must satisfy min_depth=2"
        );
    }

    #[test]
    fn test_inclusion_height_above_tip_yields_one() {
        // Characterisation test for the saturating_sub edge case: if the reported inclusion
        // height is somehow ahead of the tip, `saturating_sub` clamps to 0 and the `+ 1`
        // makes the result 1. Under min_depth=2 this still blocks promotion, but under a
        // min_depth of 1 it would pass. Pinned so the behaviour is visible if the formula
        // ever changes.
        assert_eq!(confirmation_depth(1000, 1005), 1);
    }

    #[test]
    fn test_far_future_inclusion_height_saturates() {
        assert_eq!(
            confirmation_depth(1, u64::MAX),
            1,
            "saturating_sub must not panic or wrap on an absurd inclusion height"
        );
    }
}
