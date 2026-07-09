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
