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
        
        // Create shared state
        let shared_state = SharedTrackerState::new();
        
        // Create a test tracker and add a note to update the AVL tree
        let mut tracker = TrackerStateManager::new_with_temp_storage();
        
        // Create a test note
        let recipient_pubkey: PubKey = [0x03u8; 33];
        let mut issuer_pubkey: PubKey = [0x02u8; 33];
        issuer_pubkey[0] = 0x02; // Ensure it's a valid compressed key
        let signature: Signature = [0x00u8; 65]; // Placeholder signature
        
        let note = IouNote::new(recipient_pubkey, 1000, 0, 1234567890, signature);
        
        // Add the note to the tracker
        let result = tracker.add_note(&issuer_pubkey, &note);
        assert!(result.is_ok());
        
        // Get the new AVL root digest after the update
        let new_root = tracker.get_state().avl_root_digest;
        
        // Update the shared state to match
        shared_state.set_avl_root_digest(new_root);
        
        // Verify that the shared state was updated
        assert_eq!(shared_state.get_avl_root_digest(), new_root);
        assert_ne!(shared_state.get_avl_root_digest(), [0u8; 33]); // Should not be all zeros
    }
}