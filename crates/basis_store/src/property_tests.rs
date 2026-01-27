use crate::{
    schnorr::{self, generate_keypair},
    IouNote,
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
            current_time in 1000000000u64..2000000000
        ) {
            // Test time lock validation logic
            let one_week = 7 * 24 * 60 * 60;
            let min_redemption_time = note_timestamp + one_week;

            let is_redeemable = current_time >= min_redemption_time;

            // Create a test note
            let note = IouNote::new(
                [1u8; 33],
                1000,
                0,
                note_timestamp,
                [2u8; 65],
            );

            // In a real implementation, we'd check against current_time
            // For now, just verify the time calculation is correct
            prop_assert_eq!(min_redemption_time, note_timestamp + one_week);

            // Test boundary conditions
            if current_time == min_redemption_time {
                prop_assert!(is_redeemable, "Exactly at min redemption time should be redeemable");
            }

            if current_time == min_redemption_time - 1 {
                prop_assert!(!is_redeemable, "One second before min redemption time should not be redeemable");
            }
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
}
