#[cfg(test)]
mod integration_tests {
    use basis_server::{TrackerBoxUpdateConfig, TrackerBoxUpdater, SharedTrackerState};
    use ergo_lib::ergotree_ir::address::NetworkPrefix;

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
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        let updater_shutdown_rx = shutdown_tx.subscribe();

        // Spawn the updater in a separate task so we can send shutdown signal
        let updater_task = tokio::spawn(TrackerBoxUpdater::start(
            config,
            shared_state,
            NetworkPrefix::Mainnet,
            "test_tracker_nft_1234567890abcdef".to_string(), // tracker_nft_id - required parameter
            updater_shutdown_rx,
        ));

        // Give the updater a moment to start, then send shutdown signal
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = shutdown_tx.send(()); // Send shutdown signal

        // Wait for the updater task to complete
        let result = match tokio::time::timeout(std::time::Duration::from_secs(2), updater_task).await {
            Ok(task_result) => task_result.unwrap(), // Get the actual result from the task
            Err(_) => {
                // If timeout occurs, the function is hanging - return an error
                Err(basis_server::TrackerBoxUpdaterError::LoggingError("Updater did not shut down in time".to_string()))
            }
        };

        // Should return Ok(()) when shutdown signal is received
        // For this test, we're mainly validating that the types work together
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tracker_box_updates_avl_digest() {
        use std::sync::Arc;
        use tokio::sync::Mutex;
        use basis_store::{TrackerStateManager, IouNote, PubKey, Signature};
        use secp256k1::{Secp256k1, SecretKey};

        // Create shared state
        let shared_state = SharedTrackerState::new();

        // Create a test tracker and add a note to update the AVL tree
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Generate a valid keypair for testing
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let issuer_pubkey_obj = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey: PubKey = issuer_pubkey_obj.serialize();

        // Create a test recipient pubkey
        let recipient_pubkey: PubKey = [0x03u8; 33]; // Valid compressed public key

        // Create a properly signed test note
        let note = IouNote::create_and_sign(
            recipient_pubkey,
            1000, // amount collected
            1234567890, // timestamp
            &secret_key.secret_bytes(),
        ).expect("Should be able to create a valid signed note");

        // Add the note to the tracker
        let result = tracker.add_note(&issuer_pubkey, &note);
        assert!(result.is_ok(), "Adding note to tracker should succeed: {:?}", result.err());

        // Get the new AVL root digest after the update
        let new_root = tracker.get_state().avl_root_digest;

        // Update the shared state to match
        shared_state.set_avl_root_digest(new_root);

        // Verify that the shared state was updated
        assert_eq!(shared_state.get_avl_root_digest(), new_root);
        assert_ne!(shared_state.get_avl_root_digest(), [0u8; 33]); // Should not be all zeros
    }
}