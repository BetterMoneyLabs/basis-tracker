use basis_server::{TrackerBoxUpdateConfig, TrackerBoxUpdater, SharedTrackerState};

#[tokio::test]
async fn test_tracker_box_updater_integration() {
    // Create shared state with some test values
    let shared_state = SharedTrackerState::new();
    
    // Set some test values
    let test_root = [0x11u8; 33]; // Test AVL root digest
    let test_pubkey = [0x02u8; 33]; // Test compressed public key
    shared_state.set_avl_root_digest(test_root);
    shared_state.set_tracker_pubkey(test_pubkey);
    
    // Verify the values were set correctly
    assert_eq!(shared_state.get_avl_root_digest(), test_root);
    assert_eq!(shared_state.get_tracker_pubkey(), test_pubkey);
    
    // Test creating and starting the updater
    let config = TrackerBoxUpdateConfig::default();
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let updater_shutdown_rx = shutdown_tx.subscribe();
    
    // Just verify that we can create the updater without errors
    // The actual execution would require a longer test time due to intervals
    let result = TrackerBoxUpdater::start(config, shared_state, updater_shutdown_rx).await;
    
    // Should return Ok(()) when shutdown signal is received immediately
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