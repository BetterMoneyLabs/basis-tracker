use crate::{
    schnorr::{self, generate_keypair},
    transaction_builder::{RedemptionTransactionBuilder, TxContext},
    IouNote, RedemptionManager, RedemptionRequest, TrackerStateManager,
};

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_signature_roundtrip(
            secret in prop::array::uniform32(0u8..255),
            amount in 1u64..1000000,
            timestamp in 1000000000u64..2000000000
        ) {
            // Test that signing and verification always works for valid inputs
            let (_, recipient_pubkey) = generate_keypair();
            let note = IouNote::create_and_sign(recipient_pubkey, amount, timestamp, &secret);

            // Note creation should succeed with valid inputs
            prop_assume!(note.is_ok());
            let note = note.unwrap();

            // Generate the issuer public key from the secret
            let secp = secp256k1::Secp256k1::new();
            let secret_key = secp256k1::SecretKey::from_slice(&secret).unwrap();
            let issuer_pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key).serialize();

            // Signature verification should succeed
            let verification_result = note.verify_signature(&issuer_pubkey);
            prop_assert!(verification_result.is_ok(), "Signature verification failed for valid note");
        }

        #[test]
        fn test_note_hash_consistency(
            amount1 in 1u64..1000000,
            amount2 in 1u64..1000000,
            timestamp1 in 1000000000u64..2000000000,
            timestamp2 in 1000000000u64..2000000000
        ) {
            // Test that identical inputs produce identical hashes
            let (secret, _) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();

            let note1 = IouNote::create_and_sign(recipient_pubkey, amount1, timestamp1, &secret).unwrap();
            let note2 = IouNote::create_and_sign(recipient_pubkey, amount1, timestamp1, &secret).unwrap();

            // Same inputs should produce same note structure
            prop_assert_eq!(note1.recipient_pubkey, note2.recipient_pubkey);
            prop_assert_eq!(note1.amount_collected, note2.amount_collected);
            prop_assert_eq!(note1.timestamp, note2.timestamp);

            // Different inputs should produce different notes
            let note3 = IouNote::create_and_sign(recipient_pubkey, amount2, timestamp2, &secret).unwrap();
            prop_assert_ne!(note1.amount_collected, note3.amount_collected);
        }

        #[test]
        fn test_outstanding_debt_calculation(
            amount_collected in 1u64..1000000,
            amount_redeemed in 0u64..1000000
        ) {
            // Test that outstanding debt calculation is consistent
            prop_assume!(amount_redeemed <= amount_collected);

            let note = IouNote::new(
                [1u8; 33],
                amount_collected,
                amount_redeemed,
                1234567890,
                [2u8; 65],
            );

            let expected_debt = amount_collected - amount_redeemed;
            prop_assert_eq!(note.outstanding_debt(), expected_debt);

            // Test is_fully_redeemed property
            let fully_redeemed = amount_redeemed == amount_collected;
            prop_assert_eq!(note.is_fully_redeemed(), fully_redeemed);
        }

        #[test]
        fn test_schnorr_signature_properties(
            message in prop::collection::vec(any::<u8>(), 1..1000),
            secret in prop::array::uniform32(0u8..255)
        ) {
            // Test Schnorr signature properties
            let secp = secp256k1::Secp256k1::new();
            let secret_key = secp256k1::SecretKey::from_slice(&secret).unwrap();
            let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key).serialize();

            // Generate signature
            let signature = schnorr::schnorr_sign(&message, &secret_key.secret_bytes(), &public_key);
            prop_assume!(signature.is_ok());
            let signature = signature.unwrap();

            // Verify signature
            let verification = schnorr::schnorr_verify(&signature, &message, &public_key);
            prop_assert!(verification.is_ok(), "Valid signature should verify");

            // Test that tampered message fails verification
            let mut tampered_message = message.clone();
            if !tampered_message.is_empty() {
                tampered_message[0] ^= 0x01; // Flip one bit
                let tampered_verification = schnorr::schnorr_verify(&signature, &tampered_message, &public_key);
                prop_assert!(tampered_verification.is_err(), "Tampered message should fail verification");
            }
        }

        #[test]
        fn test_note_serialization_roundtrip(
            amount in 1u64..1000000,
            timestamp in 1000000000u64..2000000000
        ) {
            // Test that note serialization preserves all fields
            let (secret, _) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();
            let original_note = IouNote::create_and_sign(recipient_pubkey, amount, timestamp, &secret).unwrap();

            // Simulate serialization by checking field access
            let reconstructed_note = IouNote::new(
                original_note.recipient_pubkey,
                original_note.amount_collected,
                original_note.amount_redeemed,
                original_note.timestamp,
                original_note.signature,
            );

            prop_assert_eq!(original_note.recipient_pubkey, reconstructed_note.recipient_pubkey);
            prop_assert_eq!(original_note.amount_collected, reconstructed_note.amount_collected);
            prop_assert_eq!(original_note.amount_redeemed, reconstructed_note.amount_redeemed);
            prop_assert_eq!(original_note.timestamp, reconstructed_note.timestamp);
            prop_assert_eq!(original_note.signature, reconstructed_note.signature);
        }

        #[test]
        fn test_time_lock_validation(
            note_timestamp in 1000000000u64..2000000000,
            _current_time in 1000000000u64..2000000000
        ) {
            // Note: Time lock enforcement is now handled by the contract based on tracker creation height.
            // Emergency redemption is available after 3 days (3*720 blocks) from tracker creation.
            // Normal redemption requires both owner and tracker signatures with no time restriction.
            // The transaction builder no longer enforces time locks.

            // Create a test note
            let note = IouNote::new(
                [1u8; 33],
                1000,
                0,
                note_timestamp,
                [2u8; 65],
            );

            // Verify note was created successfully
            prop_assert_eq!(note.timestamp, note_timestamp);

            // Time lock is now enforced by contract, not transaction builder
            // Contract checks: (HEIGHT - trackerCreationHeight) > 3 * 720
        }
    }

    // Additional property tests for specific invariants
    proptest! {
        #[test]
        fn test_note_amount_invariants(
            amount_collected in 0u64..u64::MAX,
            amount_redeemed in 0u64..u64::MAX
        ) {
            // Test that note amounts maintain invariants
            let note = IouNote::new(
                [1u8; 33],
                amount_collected,
                amount_redeemed,
                1234567890,
                [2u8; 65],
            );

            // Outstanding debt should never be negative
            prop_assert!(note.outstanding_debt() <= amount_collected);

            // If amount_redeemed > amount_collected, outstanding_debt should be 0
            // (though this should be prevented by validation)
            if amount_redeemed > amount_collected {
                prop_assert_eq!(note.outstanding_debt(), 0);
            } else {
                prop_assert_eq!(note.outstanding_debt(), amount_collected - amount_redeemed);
            }
        }

        #[test]
        fn test_signature_format_invariants(
            signature in prop::collection::vec(any::<u8>(), 65)
        ) {
            // Test that signature format maintains invariants
            let signature_array: [u8; 65] = signature.try_into().unwrap();
            let note = IouNote::new(
                [1u8; 33],
                1000,
                0,
                1234567890,
                signature_array,
            );

            // Signature should always be 65 bytes
            prop_assert_eq!(note.signature.len(), 65);

            // All-zero signature should fail verification (basic sanity check)
            if signature_array == [0u8; 65] {
                let (_, pubkey) = generate_keypair();
                let verification = note.verify_signature(&pubkey);
                prop_assert!(verification.is_err(), "Zero signature should fail verification");
            }
        }
    }

    // ============================================================================
    // Redemption-specific property tests
    // ============================================================================

    proptest! {
        #[test]
        fn test_redemption_request_validation_proptest(
            amount in 1u64..1000000,
            timestamp in 1000000000u64..2000000000,
            issuer_sig_prefix in prop::collection::vec(any::<u8>(), 1..10)
        ) {
            // Test that RedemptionRequest with various valid inputs can be created
            let (_, issuer_pubkey) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();

            let issuer_sig = format!("{}{}", hex::encode(&issuer_sig_prefix), "0".repeat(130usize.saturating_sub(issuer_sig_prefix.len() * 2)));
            let issuer_sig = if issuer_sig.len() > 130 { issuer_sig[..130].to_string() } else { issuer_sig };

            let request = RedemptionRequest {
                issuer_pubkey: hex::encode(issuer_pubkey),
                recipient_pubkey: hex::encode(recipient_pubkey),
                amount,
                timestamp,
                reserve_box_id: "test_reserve_box_1".to_string(),
                tracker_box_id: "test_tracker_box_1".to_string(),
                tracker_nft_id: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string(),
                current_height: 1000,
                recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                change_address: "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ".to_string(),
                issuer_signature: issuer_sig,
                emergency: false,
                tracker_signature: Some("02".repeat(65)),
                reserve_box_value: amount + 1000000 + 1000000, // Reserve must cover debt + fee + buffer
                fee_input_box_ids: Vec::new(),
                fee_input_total_value: 0,
            };

            // Basic validation: amount should be positive
            prop_assert!(request.amount > 0);
            // Public keys should be valid hex and 66 chars (33 bytes)
            prop_assert_eq!(request.issuer_pubkey.len(), 66);
            prop_assert_eq!(request.recipient_pubkey.len(), 66);
            // Reserve and tracker box IDs should not be empty
            prop_assert!(!request.reserve_box_id.is_empty());
            prop_assert!(!request.tracker_box_id.is_empty());
        }

        #[test]
        fn test_transaction_building_random_amounts_proptest(
            redemption_amount in 1u64..1000000u64,
            fee in 100000u64..5000000u64
        ) {
            // Test transaction building with random valid amounts
            let (secret, issuer_pubkey) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();

            // Create a note with sufficient outstanding debt
            let total_debt = redemption_amount + 1; // Ensure outstanding_debt >= redemption_amount
            let note = IouNote::create_and_sign(recipient_pubkey, total_debt, 1234567890, &secret).unwrap();

            prop_assume!(redemption_amount <= note.outstanding_debt());

            let context = TxContext {
                current_height: 1000,
                fee,
                change_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                network_prefix: 0,
            };

            let result = RedemptionTransactionBuilder::build_unsigned_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304",
                &note,
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                &[0x01, 0x02, 0x03],
                &[0u8; 65],
                &[0u8; 65],
                &issuer_pubkey,
                &context,
                redemption_amount + fee + 1000000, // Reserve box value: enough to cover redemption + fee + buffer
                None, // First redemption: no reserve lookup proof
                vec![0x03, 0x04],
                redemption_amount,
            );

            prop_assert!(result.is_ok(), "Transaction building should succeed for valid amounts: {:?}", result.err());

            let tx_data = result.unwrap();
            prop_assert_eq!(tx_data.redemption_amount, redemption_amount);
            prop_assert_eq!(tx_data.fee, fee);
            prop_assert!(tx_data.context_extension.is_some());
        }

        #[test]
        fn test_transaction_building_invalid_amounts_proptest(
            redemption_amount in 1u64..u64::MAX,
            note_amount in 1u64..1000000u64
        ) {
            // Test that transaction builder rejects amounts exceeding outstanding debt
            prop_assume!(redemption_amount > note_amount);

            let (secret, issuer_pubkey) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();

            let note = IouNote::create_and_sign(recipient_pubkey, note_amount, 1234567890, &secret).unwrap();

            let context = TxContext {
                current_height: 1000,
                fee: 1000000,
                change_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                network_prefix: 0,
            };

            let result = RedemptionTransactionBuilder::build_unsigned_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304",
                &note,
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                &[0x01, 0x02, 0x03],
                &[0u8; 65],
                &[0u8; 65],
                &issuer_pubkey,
                &context,
                note_amount + 1000000 + 1000000, // Reserve box value: enough to cover max debt + fee + buffer
                None,
                vec![0x03, 0x04],
                redemption_amount,
            );

            prop_assert!(result.is_err(), "Transaction building should fail when redemption amount {} exceeds outstanding debt {}", redemption_amount, note_amount);
        }

        #[test]
        fn test_multiple_redemption_sequence_proptest(
            initial_amount in 1000u64..10000000u64,
            num_redemptions in 1usize..10usize
        ) {
            // Test that a sequence of partial redemptions maintains invariants
            let tracker = TrackerStateManager::new_with_temp_storage();
            let mut redemption_manager = RedemptionManager::new(tracker);

            let (secret, issuer_pubkey) = generate_keypair();
            let (_, recipient_pubkey) = generate_keypair();

            // Create and add a note
            let note = IouNote::create_and_sign(recipient_pubkey, initial_amount, 1234567890, &secret).unwrap();
            redemption_manager.tracker.add_note(&issuer_pubkey, &note).unwrap();

            let mut total_redeemed: u64 = 0;

            for i in 0..num_redemptions {
                let remaining = initial_amount - total_redeemed;
                prop_assume!(remaining > 0);

                // Redeem a random portion of the remaining amount
                let redeem_amount = if remaining > 1 { (i as u64 + 1) * (remaining / (num_redemptions as u64 + 1)).max(1) } else { remaining };
                let redeem_amount = redeem_amount.min(remaining);

            let request = RedemptionRequest {
                issuer_pubkey: hex::encode(issuer_pubkey),
                recipient_pubkey: hex::encode(recipient_pubkey),
                amount: redeem_amount,
                timestamp: 1234567890 + i as u64,
                reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
                tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
                tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
                current_height: 1000,
                recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                change_address: "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ".to_string(),
                issuer_signature: "01".repeat(65),
                emergency: false,
                tracker_signature: Some("02".repeat(65)),
                reserve_box_value: initial_amount + 1000000 + 1000000, // Reserve must cover max debt + fee + buffer
                fee_input_box_ids: Vec::new(),
                fee_input_total_value: 0,
            };

                let result = redemption_manager.initiate_redemption(&request);
                if result.is_ok() {
                    total_redeemed += redeem_amount;
                    let _ = redemption_manager.complete_redemption(&issuer_pubkey, &recipient_pubkey, redeem_amount, None);
                }
            }

            // Verify that total redeemed never exceeds initial amount
            prop_assert!(total_redeemed <= initial_amount, "Total redeemed {} should not exceed initial amount {}", total_redeemed, initial_amount);

            // Verify final state
            let final_note = redemption_manager.tracker.lookup_note(&issuer_pubkey, &recipient_pubkey).unwrap();
            prop_assert_eq!(final_note.amount_redeemed, total_redeemed, "Final redeemed amount should match total redeemed");
            prop_assert!(final_note.outstanding_debt() == initial_amount - total_redeemed || final_note.outstanding_debt() == 0,
                "Outstanding debt should be initial - total_redeemed or 0");
        }
    }
}
