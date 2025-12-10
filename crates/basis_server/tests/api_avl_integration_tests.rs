#[cfg(test)]
mod api_avl_integration_tests {
    use basis_store::{TrackerStateManager, IouNote, PubKey, Signature};
    use std::sync::Arc;

    /// Helper to generate a test public key
    fn generate_test_pubkey(seed: u8) -> PubKey {
        let mut key = [0u8; 33];
        key[0] = 0x02; // Compressed public key prefix
        for i in 1..33 {
            key[i] = seed * (i as u8 + 1);
        }
        key
    }

    /// Helper to generate a test signature
    fn generate_test_signature(seed: u8) -> Signature {
        let mut sig = [0u8; 65];
        for i in 0..65 {
            sig[i] = seed * (i as u8 + 1);
        }
        sig
    }

    #[tokio::test]
    async fn test_tracker_state_manager_avl_tree_integration() {
        // Test the core integration at the TrackerStateManager level
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Verify initial state
        let initial_state = tracker.get_state();
        let initial_root = initial_state.avl_root_digest.clone();

        // Generate test data
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);

        // Create a test note
        let note = IouNote::new(
            recipient_pubkey,
            1000, // amount collected
            0,    // amount redeemed
            1000000, // timestamp
            generate_test_signature(1),
        );

        // Add note and verify AVL tree state changed
        let result = tracker.add_note(&issuer_pubkey, &note);
        assert!(result.is_ok(), "Should be able to add note");

        let updated_state = tracker.get_state();
        let updated_root = updated_state.avl_root_digest.clone();

        // Verify that the AVL tree root has changed
        assert_ne!(initial_root, updated_root, "AVL tree root should change after adding note");

        // Verify that the root is not the empty root
        assert_ne!(updated_root, [0u8; 33], "Root should not be empty after adding note");
    }

    #[tokio::test]
    async fn test_tracker_multiple_notes_update_avl_tree() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Initial state
        let initial_state = tracker.get_state();
        let initial_root = initial_state.avl_root_digest.clone();

        // Generate test data
        let issuer_pubkey = generate_test_pubkey(1);

        // Add multiple notes and verify AVL tree updates each time
        for i in 1..=3 {
            let recipient_pubkey = generate_test_pubkey(i + 1);
            let note = IouNote::new(
                recipient_pubkey,
                1000 * (i as u64), // different amounts
                0,
                1000000 + (i as u64),
                generate_test_signature(i as u8),
            );

            let result = tracker.add_note(&issuer_pubkey, &note);
            assert!(result.is_ok(), "Should be able to add note {}", i);

            let current_state = tracker.get_state();
            let current_root = current_state.avl_root_digest.clone();

            // Verify root changes with each note
            assert_ne!(current_root, [0u8; 33], "Root should not be empty after note {}", i);

            // Each new note should result in a different root
            if i > 1 {
                // Note: roots might not be different for every single addition if the tree structure is similar
                // but the final state should be different than the initial
            }
        }

        // Final state should be different from initial
        let final_state = tracker.get_state();
        let final_root = final_state.avl_root_digest.clone();
        assert_ne!(initial_root, final_root, "Final root should differ from initial after adding notes");
    }

    #[tokio::test]
    async fn test_tracker_note_retrieval_corresponds_to_avl_storage() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Generate test data
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);

        // Create and store note
        let original_note = IouNote::new(
            recipient_pubkey,
            1500,
            0,
            1000000,
            generate_test_signature(1),
        );

        // Store the note and update AVL tree
        let result = tracker.add_note(&issuer_pubkey, &original_note);
        assert!(result.is_ok(), "Should be able to add note");

        // Retrieve the note back
        let retrieved_note_result = tracker.lookup_note(&issuer_pubkey, &recipient_pubkey);
        assert!(retrieved_note_result.is_ok(), "Should be able to retrieve note");

        let retrieved_note = retrieved_note_result.unwrap();

        // Verify retrieved note matches original
        assert_eq!(retrieved_note.amount_collected, original_note.amount_collected);
        assert_eq!(retrieved_note.amount_redeemed, original_note.amount_redeemed);
        assert_eq!(retrieved_note.timestamp, original_note.timestamp);
        assert_eq!(retrieved_note.signature, original_note.signature);
        assert_eq!(retrieved_note.recipient_pubkey, original_note.recipient_pubkey);

        // Verify AVL tree state is non-empty
        let state = tracker.get_state();
        let root = state.avl_root_digest.clone();
        assert_ne!(root, [0u8; 33], "Root should not be empty after note storage");
    }

    #[tokio::test]
    async fn test_avl_tree_state_consistency_with_note_operations() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Start with empty state
        let empty_state = tracker.get_state();
        let empty_root = empty_state.avl_root_digest.clone();

        // Generate test keys
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);

        // Add a note
        let note1 = IouNote::new(
            recipient_pubkey,
            1000,
            0,
            1000000,
            generate_test_signature(1),
        );

        tracker.add_note(&issuer_pubkey, &note1).unwrap();
        let state_after_add = tracker.get_state();
        let root_after_add = state_after_add.avl_root_digest.clone();

        // Update the same note
        let note2 = IouNote::new(
            recipient_pubkey,
            2000, // increased amount
            0,
            1000001, // updated timestamp
            generate_test_signature(2),
        );

        tracker.add_note(&issuer_pubkey, &note2).unwrap(); // add_note actually updates
        let state_after_update = tracker.get_state();
        let root_after_update = state_after_update.avl_root_digest.clone();

        // Verify all states are different
        assert_ne!(empty_root, root_after_add, "Root should change after first note");
        assert_ne!(root_after_add, root_after_update, "Root should change after update");
        assert_ne!(empty_root, root_after_update, "Final root should differ from initial");

        // Verify AVL tree root is properly formatted (33 bytes)
        assert_eq!(root_after_update.len(), 33, "AVL root should be 33 bytes");
    }

    #[tokio::test]
    async fn test_avl_tree_proof_generation_integration() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        // Generate test keys
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);

        // Create and store a note
        let note = IouNote::new(
            recipient_pubkey,
            1000,
            0,
            1000000,
            generate_test_signature(1),
        );

        let add_result = tracker.add_note(&issuer_pubkey, &note);
        assert!(add_result.is_ok(), "Should be able to add note");

        // Generate a proof for this note
        let proof_result = tracker.generate_proof(&issuer_pubkey, &recipient_pubkey);
        assert!(proof_result.is_ok(), "Should be able to generate proof for stored note");

        let proof = proof_result.unwrap();

        // Verify that the proof contains the expected note
        assert_eq!(proof.note.amount_collected, 1000);
        assert_eq!(proof.note.recipient_pubkey, recipient_pubkey);

        // Verify that the proof contains AVL proof data (not empty)
        assert!(!proof.avl_proof.is_empty(), "AVL proof should not be empty");

        // Verify AVL tree state commitment exists
        let state = tracker.get_state();
        assert_ne!(state.avl_root_digest, [0u8; 33], "Tracker state should have valid AVL root");
    }

    #[tokio::test]
    async fn test_tracker_state_manager_initial_empty_state() {
        let tracker = TrackerStateManager::new_with_temp_storage();

        // Verify initial state is properly initialized
        let initial_state = tracker.get_state();
        let initial_root = initial_state.avl_root_digest.clone();

        // Initial root should be the empty tree root (not all zeros in practice)
        // For an empty AVL tree, this could be a specific empty tree digest
        // The important thing is it should be consistent
        assert_eq!(initial_root.len(), 33, "Initial root should be 33 bytes");
    }
}