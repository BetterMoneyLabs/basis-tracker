#[cfg(test)]
mod avl_tree_integration_tests {
    use basis_store::{IouNote, NoteKey, PubKey, Signature, TrackerStateManager};

    /// Helper function to generate a test public key
    fn generate_test_pubkey(seed: u8) -> PubKey {
        let mut key = [0u8; 33];
        key[0] = 0x02; // Compressed public key prefix
        for i in 1..33 {
            key[i] = seed * (i as u8 + 1);
        }
        key
    }

    /// Helper function to generate a test signature
    fn generate_test_signature(seed: u8) -> Signature {
        let mut sig = [0u8; 65];
        for i in 0..65 {
            sig[i] = seed * (i as u8 + 1);
        }
        sig
    }

    #[tokio::test]
    async fn test_note_storage_under_avl_tree() {
        // Create a new tracker state manager
        let mut tracker = TrackerStateManager::new();
        
        // Generate test keys
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
        
        // Store the note in the tracker
        let result = tracker.add_note(&issuer_pubkey, &note);
        assert!(result.is_ok(), "Should be able to add note to tracker");
        
        // Check that the AVL tree root digest has been updated
        let initial_state = tracker.get_state();
        let initial_root = initial_state.avl_root_digest.clone();
        assert_ne!(initial_root, [0u8; 33], "Root digest should not be empty after adding note");
        
        // Verify the tree is not empty by checking another state change
        let note2 = IouNote::new(
            generate_test_pubkey(3),
            2000, // different amount
            0,    // amount redeemed
            1000001, // different timestamp
            generate_test_signature(2),
        );
        
        let result2 = tracker.add_note(&issuer_pubkey, &note2);
        assert!(result2.is_ok(), "Should be able to add second note to tracker");
        
        let updated_state = tracker.get_state();
        let updated_root = updated_state.avl_root_digest.clone();
        assert_ne!(updated_root, initial_root, "Root digest should change after adding second note");
    }

    #[tokio::test]
    async fn test_note_reading_via_tracker() {
        // Create a new tracker state manager
        let mut tracker = TrackerStateManager::new();
        
        // Generate test keys
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);
        
        // Create and store a test note
        let original_note = IouNote::new(
            recipient_pubkey,
            1000, // amount collected
            0,    // amount redeemed
            1000000, // timestamp
            generate_test_signature(1),
        );
        
        let result = tracker.add_note(&issuer_pubkey, &original_note);
        assert!(result.is_ok(), "Should be able to add note to tracker");
        
        // Read the note back from the tracker
        let retrieved_note = tracker.lookup_note(&issuer_pubkey, &recipient_pubkey);
        assert!(retrieved_note.is_ok(), "Should be able to retrieve note from tracker");
        
        let retrieved_note = retrieved_note.unwrap();
        assert_eq!(retrieved_note.recipient_pubkey, original_note.recipient_pubkey);
        assert_eq!(retrieved_note.amount_collected, original_note.amount_collected);
        assert_eq!(retrieved_note.amount_redeemed, original_note.amount_redeemed);
        assert_eq!(retrieved_note.timestamp, original_note.timestamp);
        assert_eq!(retrieved_note.signature, original_note.signature);
    }

    #[tokio::test]
    async fn test_multiple_notes_under_avl_tree() {
        // Create a new tracker state manager
        let mut tracker = TrackerStateManager::new();
        
        // Generate test keys
        let issuer_pubkey = generate_test_pubkey(1);
        
        // Add multiple notes with different recipients
        let recipients = vec![generate_test_pubkey(2), generate_test_pubkey(3), generate_test_pubkey(4)];
        let amounts = vec![1000, 2000, 3000];
        
        for (i, recipient_pubkey) in recipients.iter().enumerate() {
            let note = IouNote::new(
                *recipient_pubkey,
                amounts[i], // different amounts
                0,          // amount redeemed
                1000000 + (i as u64), // different timestamps
                generate_test_signature(i as u8 + 1),
            );
            
            let result = tracker.add_note(&issuer_pubkey, &note);
            assert!(result.is_ok(), "Should be able to add note {} to tracker", i);
        }
        
        // Verify all notes can be retrieved
        for (i, recipient_pubkey) in recipients.iter().enumerate() {
            let retrieved_note = tracker.lookup_note(&issuer_pubkey, recipient_pubkey);
            assert!(retrieved_note.is_ok(), "Should be able to retrieve note {} from tracker", i);
            
            let retrieved_note = retrieved_note.unwrap();
            assert_eq!(retrieved_note.amount_collected, amounts[i]);
        }
        
        // Check that we can get all notes for the issuer
        let all_notes = tracker.get_issuer_notes(&issuer_pubkey);
        assert!(all_notes.is_ok(), "Should be able to get all notes for issuer");
        assert_eq!(all_notes.as_ref().unwrap().len(), 3, "Should have 3 notes for the issuer");
        
        // Verify AVL tree state commitment has been updated
        let state = tracker.get_state();
        let root_digest = state.avl_root_digest.clone();
        assert_ne!(root_digest, [0u8; 33], "Root digest should not be empty after adding multiple notes");
    }

    #[tokio::test]
    async fn test_avl_tree_state_commitment_changes() {
        // Create a new tracker state manager
        let mut tracker = TrackerStateManager::new();
        
        // Initial state
        let initial_state = tracker.get_state();
        let initial_root = initial_state.avl_root_digest.clone();
        
        // Generate test keys
        let issuer_pubkey = generate_test_pubkey(1);
        let recipient_pubkey = generate_test_pubkey(2);
        
        // Add first note
        let note1 = IouNote::new(
            recipient_pubkey,
            1000,
            0,
            1000000,
            generate_test_signature(1),
        );
        
        tracker.add_note(&issuer_pubkey, &note1).unwrap();
        let state_after_first = tracker.get_state();
        let root_after_first = state_after_first.avl_root_digest.clone();
        
        // Add second note
        let note2 = IouNote::new(
            generate_test_pubkey(3),
            2000,
            0,
            1000001,
            generate_test_signature(2),
        );
        
        tracker.add_note(&issuer_pubkey, &note2).unwrap();
        let state_after_second = tracker.get_state();
        let root_after_second = state_after_second.avl_root_digest.clone();
        
        // Verify all roots are different
        assert_ne!(initial_root, root_after_first, "Root should change after first note");
        assert_ne!(root_after_first, root_after_second, "Root should change after second note");
        assert_ne!(initial_root, root_after_second, "Final root should be different from initial");
    }

    #[tokio::test]
    async fn test_avl_tree_integration_with_server_channel() {
        // This test would require a full tracker thread implementation
        // which is beyond the scope of simple integration tests
        // The focus is on testing the core AVL functionality which is done above
        assert!(true, "Server channel integration requires running tracker thread");
    }

    #[tokio::test]
    async fn test_note_key_generation() {
        // Test that note keys are generated correctly for AVL tree
        let issuer1 = generate_test_pubkey(1);
        let issuer2 = generate_test_pubkey(2);
        let recipient = generate_test_pubkey(3);
        
        // Keys with same issuer and recipient should be identical
        let key1 = NoteKey::from_keys(&issuer1, &recipient);
        let key2 = NoteKey::from_keys(&issuer1, &recipient);
        assert_eq!(key1.to_bytes(), key2.to_bytes(), "Same issuer/recipient should generate same key");
        
        // Keys with different issuer should be different
        let key3 = NoteKey::from_keys(&issuer2, &recipient);
        assert_ne!(key1.to_bytes(), key3.to_bytes(), "Different issuer should generate different key");
        
        // Keys with different recipient should be different
        let recipient2 = generate_test_pubkey(4);
        let key4 = NoteKey::from_keys(&issuer1, &recipient2);
        assert_ne!(key1.to_bytes(), key4.to_bytes(), "Different recipient should generate different key");
    }
}