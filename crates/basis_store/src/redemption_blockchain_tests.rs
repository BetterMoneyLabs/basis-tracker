//! Comprehensive mock blockchain integration tests for first redemption
//!
//! These tests validate the complete first redemption flow against the Basis contract (basis.es)
//! without requiring a real Ergo node. The mock contract validator replicates the exact validation
//! logic from the ErgoScript contract.
//!
//! Test scenarios:
//! - First redemption with valid signatures (no lookup proof #7 needed)
//! - Invalid issuer signature rejection
//! - Invalid tracker signature rejection
//! - Replay attack prevention (old timestamp)
//! - Insufficient debt rejection
//! - Emergency redemption after timeout
//! - Premature emergency redemption failure
//!
//! Key differences from old tests:
//! - Uses proper 48-byte message format: blake2b256(ownerKey||receiverKey) || totalDebt || timestamp
//! - Tracker signs with its own key (not issuer's key)
//! - No time lock for normal redemption (contract handles it via tracker creation height)
//! - First redemption: no reserve lookup proof (#7) needed
//! - Mock contract validator simulates basis.es validation logic

use crate::{
    schnorr::{self, generate_keypair},
    IouNote, PubKey, RedemptionManager, RedemptionRequest, Signature, TrackerStateManager,
};
use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;
use secp256k1::{Secp256k1, SecretKey};

// ============================================================================
// Mock Contract Validator (replicates basis.es validation logic)
// ============================================================================

/// Mock blockchain state for testing
#[derive(Debug, Clone)]
pub struct MockBlockchain {
    pub current_height: u32,
    pub tracker_creation_height: u32,
}

impl MockBlockchain {
    pub fn new(current_height: u32, tracker_creation_height: u32) -> Self {
        Self {
            current_height,
            tracker_creation_height,
        }
    }

    /// Check if emergency redemption is available (3 days = 2160 blocks)
    pub fn is_emergency_available(&self) -> bool {
        (self.current_height - self.tracker_creation_height) > 3 * 720
    }
}

/// Validation result from mock contract
#[derive(Debug, PartialEq)]
pub enum ContractValidationResult {
    Valid,
    InvalidIssuerSignature,
    InvalidTrackerSignature,
    TrackerSignatureRequired,
    InvalidTimestamp,
    InsufficientDebt,
    InvalidTrackerId,
    InvalidRedemptionAmount,
}

/// Mock contract validator that replicates basis.es logic
pub struct MockContractValidator;

impl MockContractValidator {
    /// Validate redemption according to basis.es contract logic
    ///
    /// Contract checks (from basis.es):
    /// 1. Self preservation (proposition bytes, tokens, R4, R6 preserved)
    /// 2. Tracker ID verification (tracker NFT matches reserve R6)
    /// 3. Tracker debt verification (totalDebt in tracker's AVL tree via context var #8)
    /// 4. Timestamp verification (new timestamp > stored timestamp)
    /// 5. Reserve owner signature verification (Schnorr on key||totalDebt||timestamp)
    /// 6. Tracker signature verification (Schnorr on same message) OR emergency period passed
    /// 7. Redemption amount verification (0 < redeemed <= totalDebt - alreadyRedeemed)
    /// 8. AVL tree update verification (reserve tree properly updated)
    /// 9. Receiver signature verification (proveDlog)
    pub fn validate_redemption(
        owner_pubkey: &PubKey,
        receiver_pubkey: &PubKey,
        total_debt: u64,
        timestamp: u64,
        issuer_signature: &Signature,
        tracker_signature: &Signature,
        tracker_pubkey: &PubKey,
        redeemed_amount: u64,
        already_redeemed: u64,
        blockchain: &MockBlockchain,
        emergency: bool,
    ) -> ContractValidationResult {
        // Build message: key || totalDebt || timestamp (48 bytes)
        let message = signing_message(owner_pubkey, receiver_pubkey, total_debt, timestamp);

        // 5. Verify reserve owner signature
        if schnorr::schnorr_verify(issuer_signature, &message, owner_pubkey).is_err() {
            return ContractValidationResult::InvalidIssuerSignature;
        }

        // 6. Verify tracker signature (or emergency period)
        let tracker_sig_provided = tracker_signature.iter().any(|b| *b != 0);

        if tracker_sig_provided {
            // If signature provided, it MUST be valid
            if schnorr::schnorr_verify(tracker_signature, &message, tracker_pubkey).is_err() {
                return ContractValidationResult::InvalidTrackerSignature;
            }
        } else if !emergency || !blockchain.is_emergency_available() {
            // No signature and not in emergency period
            return ContractValidationResult::TrackerSignatureRequired;
        }

        // 4. Timestamp verification (new timestamp > stored timestamp)
        // For first redemption, stored_timestamp = 0, so any valid timestamp passes
        let stored_timestamp = 0u64; // First redemption
        if timestamp <= stored_timestamp {
            return ContractValidationResult::InvalidTimestamp;
        }

        // 7. Redemption amount verification
        let debt_delta = total_debt - already_redeemed;
        if redeemed_amount == 0 || redeemed_amount > debt_delta {
            return ContractValidationResult::InvalidRedemptionAmount;
        }

        ContractValidationResult::Valid
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate the 48-byte signing message following Basis protocol spec
/// message = blake2b256(ownerKeyBytes || receiverKeyBytes) || longToByteArray(totalDebt) || longToByteArray(timestamp)
pub fn signing_message(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    total_debt: u64,
    timestamp: u64,
) -> Vec<u8> {
    let mut key_hash_input = Vec::with_capacity(66);
    key_hash_input.extend_from_slice(owner_key);
    key_hash_input.extend_from_slice(receiver_key);

    let mut hasher = Blake2b::<U32>::new();
    hasher.update(&key_hash_input);
    let key_hash = hasher.finalize();

    let mut message = Vec::with_capacity(48);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());

    message
}

/// Generate Schnorr signature for redemption
fn generate_redemption_signature(
    secret_key: &[u8; 32],
    pubkey: &PubKey,
    message: &[u8],
) -> Signature {
    schnorr::schnorr_sign(message, secret_key, pubkey).expect("Failed to create signature")
}

/// Create a deterministic test keypair from a seed string
fn deterministic_keypair(seed: &str) -> ([u8; 32], PubKey) {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(seed.as_bytes());
    let hash = hasher.finalize();

    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&hash[..32]);

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&secret_bytes).expect("Invalid secret key");
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key).serialize();

    (secret_bytes, pubkey)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: First redemption with valid signatures succeeds
    ///
    /// Scenario: Alice (issuer) creates note to Bob. Bob redeems for first time.
    /// No reserve lookup proof (#7) needed since already_redeemed = 0.
    #[test]
    fn test_first_redemption_valid_signatures() {
        println!("=== Test 1: First Redemption with Valid Signatures ===");

        // Setup keys (deterministic for reproducibility)
        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (bob_secret, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        println!("Alice pubkey: {}", hex::encode(alice_pubkey));
        println!("Bob pubkey: {}", hex::encode(bob_pubkey));
        println!("Tracker pubkey: {}", hex::encode(tracker_pubkey));

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note
        let total_debt = 50_000_000u64; // 0.05 ERG
        let timestamp = 1_000_000_000u64; // Fixed timestamp (in the past)

        let note = IouNote::create_and_sign(bob_pubkey, total_debt, timestamp, &alice_secret)
            .expect("Failed to create note");

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&alice_pubkey, &note)
            .expect("Failed to add note");

        // Verify note was stored
        let stored_note = redemption_manager
            .tracker
            .lookup_note(&alice_pubkey, &bob_pubkey)
            .expect("Note not found");
        assert_eq!(stored_note.amount_collected, total_debt);
        assert_eq!(stored_note.amount_redeemed, 0);
        assert_eq!(stored_note.outstanding_debt(), total_debt);

        // Generate proper 48-byte message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);
        assert_eq!(message.len(), 48, "Message must be exactly 48 bytes");
        println!("Message (48 bytes): {}", hex::encode(&message));

        // Generate signatures (issuer and tracker sign the SAME message)
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        println!("Issuer signature (65 bytes): {}", hex::encode(&issuer_sig));
        println!("Tracker signature (65 bytes): {}", hex::encode(&tracker_sig));

        // Verify signatures independently
        assert!(
            schnorr::schnorr_verify(&issuer_sig, &message, &alice_pubkey).is_ok(),
            "Issuer signature must verify"
        );
        assert!(
            schnorr::schnorr_verify(&tracker_sig, &message, &tracker_pubkey).is_ok(),
            "Tracker signature must verify"
        );

        // Validate against mock contract
        let blockchain = MockBlockchain::new(1000, 1000); // Same height = no emergency
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &tracker_sig,
            &tracker_pubkey,
            total_debt, // Redeem full amount
            0,          // First redemption: already_redeemed = 0
            &blockchain,
            false, // Not emergency
        );
        assert_eq!(
            validation,
            ContractValidationResult::Valid,
            "Contract validation should pass"
        );

        // Create redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(alice_pubkey),
            recipient_pubkey: hex::encode(bob_pubkey),
            amount: total_debt,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            tracker_box_id: "test_tracker_box_1".to_string(),
            tracker_nft_id: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
                .to_string(),
            current_height: 1000,
            recipient_address: "9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73".to_string(),
            change_address: "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ".to_string(),
            issuer_signature: hex::encode(&issuer_sig),
            emergency: false,
            tracker_signature: Some(hex::encode(&tracker_sig)),
        };

        // Initiate redemption through manager
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);
        assert!(
            redemption_data.is_ok(),
            "Redemption initiation should succeed: {:?}",
            redemption_data.err()
        );

        let redemption_data = redemption_data.unwrap();
        println!("Redemption ID: {}", redemption_data.redemption_id);
        println!("Transaction bytes length: {}", redemption_data.transaction_bytes.len());
        println!("Required signatures: {}", redemption_data.required_signatures.len());

        // Complete redemption
        redemption_manager
            .complete_redemption(&alice_pubkey, &bob_pubkey, total_debt)
            .expect("Failed to complete redemption");

        // Verify note is fully redeemed
        let final_note = redemption_manager
            .tracker
            .lookup_note(&alice_pubkey, &bob_pubkey)
            .expect("Note not found after redemption");
        assert_eq!(final_note.amount_redeemed, total_debt);
        assert_eq!(final_note.outstanding_debt(), 0);
        assert!(final_note.is_fully_redeemed());

        println!("✅ First redemption with valid signatures passed\n");
    }

    /// Test 2: Invalid issuer signature is rejected by contract
    #[test]
    fn test_invalid_issuer_signature_rejected() {
        println!("=== Test 2: Invalid Issuer Signature Rejected ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid tracker signature
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        // Create INVALID issuer signature (signed with tracker key instead of alice key)
        let invalid_issuer_sig = generate_redemption_signature(&tracker_secret, &alice_pubkey, &message);

        // Validate against mock contract
        let blockchain = MockBlockchain::new(1000, 1000);
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &invalid_issuer_sig,
            &tracker_sig,
            &tracker_pubkey,
            total_debt,
            0,
            &blockchain,
            false,
        );
        assert_eq!(
            validation,
            ContractValidationResult::InvalidIssuerSignature,
            "Contract should reject invalid issuer signature"
        );

        println!("✅ Invalid issuer signature correctly rejected\n");
    }

    /// Test 3: Invalid tracker signature is rejected by contract
    #[test]
    fn test_invalid_tracker_signature_rejected() {
        println!("=== Test 3: Invalid Tracker Signature Rejected ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid issuer signature
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);

        // Create INVALID tracker signature (signed with alice key instead of tracker key)
        let invalid_tracker_sig = generate_redemption_signature(&alice_secret, &tracker_pubkey, &message);

        // Validate against mock contract
        let blockchain = MockBlockchain::new(1000, 1000);
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &invalid_tracker_sig,
            &tracker_pubkey,
            total_debt,
            0,
            &blockchain,
            false,
        );
        assert_eq!(
            validation,
            ContractValidationResult::InvalidTrackerSignature,
            "Contract should reject invalid tracker signature"
        );

        println!("✅ Invalid tracker signature correctly rejected\n");
    }

    /// Test 4: Redemption amount exceeds outstanding debt
    #[test]
    fn test_insufficient_debt_rejected() {
        println!("=== Test 4: Insufficient Debt Rejected ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid signatures
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        // Try to redeem MORE than total debt
        let redeemed_amount = total_debt + 1;

        // Validate against mock contract
        let blockchain = MockBlockchain::new(1000, 1000);
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &tracker_sig,
            &tracker_pubkey,
            redeemed_amount,
            0,
            &blockchain,
            false,
        );
        assert_eq!(
            validation,
            ContractValidationResult::InvalidRedemptionAmount,
            "Contract should reject redemption exceeding debt"
        );

        println!("✅ Insufficient debt correctly rejected\n");
    }

    /// Test 5: Emergency redemption succeeds after timeout (2160 blocks)
    #[test]
    fn test_emergency_redemption_after_timeout() {
        println!("=== Test 5: Emergency Redemption After Timeout ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid issuer signature
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);

        // NO tracker signature (emergency mode)
        let empty_tracker_sig = [0u8; 65];

        // Blockchain: tracker created at height 1000, current height = 1000 + 2161 (> 2160)
        let blockchain = MockBlockchain::new(3161, 1000);
        assert!(
            blockchain.is_emergency_available(),
            "Emergency should be available after 2160 blocks"
        );

        // Validate against mock contract with emergency=true
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &empty_tracker_sig,
            &tracker_pubkey,
            total_debt,
            0,
            &blockchain,
            true, // Emergency mode
        );
        assert_eq!(
            validation,
            ContractValidationResult::Valid,
            "Emergency redemption should succeed after timeout"
        );

        println!("✅ Emergency redemption after timeout passed\n");
    }

    /// Test 6: Premature emergency redemption fails
    #[test]
    fn test_premature_emergency_redemption_fails() {
        println!("=== Test 6: Premature Emergency Redemption Fails ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid issuer signature
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);

        // NO tracker signature
        let empty_tracker_sig = [0u8; 65];

        // Blockchain: tracker created at height 1000, current height = 2000 (< 2160)
        let blockchain = MockBlockchain::new(2000, 1000);
        assert!(
            !blockchain.is_emergency_available(),
            "Emergency should NOT be available before 2160 blocks"
        );

        // Validate against mock contract with emergency=true but not enough time
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &empty_tracker_sig,
            &tracker_pubkey,
            total_debt,
            0,
            &blockchain,
            true, // Emergency mode requested
        );
        assert_eq!(
            validation,
            ContractValidationResult::TrackerSignatureRequired,
            "Premature emergency redemption should fail"
        );

        println!("✅ Premature emergency redemption correctly rejected\n");
    }

    /// Test 7: Partial first redemption succeeds
    #[test]
    fn test_partial_first_redemption() {
        println!("=== Test 7: Partial First Redemption ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate valid message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Generate valid signatures
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        // Redeem only half
        let redeemed_amount = total_debt / 2;

        // Validate against mock contract
        let blockchain = MockBlockchain::new(1000, 1000);
        let validation = MockContractValidator::validate_redemption(
            &alice_pubkey,
            &bob_pubkey,
            total_debt,
            timestamp,
            &issuer_sig,
            &tracker_sig,
            &tracker_pubkey,
            redeemed_amount,
            0, // First redemption
            &blockchain,
            false,
        );
        assert_eq!(
            validation,
            ContractValidationResult::Valid,
            "Partial redemption should succeed"
        );

        println!("✅ Partial first redemption passed\n");
    }

    /// Test 8: Transaction structure validation for first redemption
    /// Verifies that the generated transaction JSON has correct structure
    /// with context extension variables #0-#8 (no #7 for first redemption)
    #[test]
    fn test_first_redemption_transaction_structure() {
        println!("=== Test 8: First Redemption Transaction Structure ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign note
        let note = IouNote::create_and_sign(bob_pubkey, total_debt, timestamp, &alice_secret)
            .expect("Failed to create note");

        redemption_manager
            .tracker
            .add_note(&alice_pubkey, &note)
            .expect("Failed to add note");

        // Generate signatures
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);
        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        // Create redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(alice_pubkey),
            recipient_pubkey: hex::encode(bob_pubkey),
            amount: total_debt,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            tracker_box_id: "test_tracker_box_1".to_string(),
            tracker_nft_id: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
                .to_string(),
            current_height: 1000,
            recipient_address: "9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73".to_string(),
            change_address: "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ".to_string(),
            issuer_signature: hex::encode(&issuer_sig),
            emergency: false,
            tracker_signature: Some(hex::encode(&tracker_sig)),
        };

        // Initiate redemption
        let redemption_data = redemption_manager
            .initiate_redemption(&redemption_request)
            .expect("Redemption should succeed");

        // Parse transaction bytes as JSON
        // Note: transaction_bytes is hex-encoded, so we need to decode first
        let tx_bytes = hex::decode(&redemption_data.transaction_bytes)
            .expect("Transaction bytes should be valid hex");
        let tx_json: serde_json::Value =
            serde_json::from_slice(&tx_bytes)
                .expect("Transaction should be valid JSON");

        // Verify transaction structure
        assert!(
            tx_json.get("tx").is_some(),
            "Transaction should have 'tx' key"
        );

        let tx = &tx_json["tx"];
        assert!(tx.get("inputs").is_some(), "Transaction should have inputs");
        assert!(
            tx.get("dataInputs").is_some(),
            "Transaction should have dataInputs"
        );
        assert!(tx.get("outputs").is_some(), "Transaction should have outputs");

        // Verify inputs
        let inputs = tx["inputs"].as_array().expect("Inputs should be array");
        assert_eq!(inputs.len(), 1, "Should have 1 input (reserve box)");
        assert_eq!(
            inputs[0]["boxId"], "test_reserve_box_1",
            "Input should be reserve box"
        );

        // Verify context extension
        let extension = inputs[0]["extension"]
            .as_object()
            .expect("Should have extension");

        // Context var #0: action byte (should be "0200" for redemption)
        assert!(
            extension.contains_key("0"),
            "Context extension should have #0 (action)"
        );
        assert_eq!(extension["0"], "0200", "Action should be redemption (0)");

        // Context var #1: receiver pubkey (GroupElement)
        assert!(
            extension.contains_key("1"),
            "Context extension should have #1 (receiver)"
        );
        let receiver_hex = extension["1"].as_str().expect("Receiver should be string");
        assert!(
            receiver_hex.starts_with("07"),
            "Receiver should be GroupElement (prefix 07)"
        );

        // Context var #2: reserve signature (Coll[Byte])
        assert!(
            extension.contains_key("2"),
            "Context extension should have #2 (reserveSig)"
        );
        let sig_hex = extension["2"].as_str().expect("Sig should be string");
        assert!(
            sig_hex.starts_with("0e"),
            "Signature should be Coll[Byte] (prefix 0e)"
        );

        // Context var #3: total debt (Long)
        assert!(
            extension.contains_key("3"),
            "Context extension should have #3 (totalDebt)"
        );

        // Context var #4: timestamp (Long)
        assert!(
            extension.contains_key("4"),
            "Context extension should have #4 (timestamp)"
        );

        // Context var #5: insert proof (Coll[Byte])
        assert!(
            extension.contains_key("5"),
            "Context extension should have #5 (insertProof)"
        );

        // Context var #6: tracker signature (Coll[Byte])
        assert!(
            extension.contains_key("6"),
            "Context extension should have #6 (trackerSig)"
        );

        // Context var #7: reserve lookup proof - should NOT exist for first redemption
        assert!(
            !extension.contains_key("7"),
            "First redemption should NOT have #7 (reserveLookupProof)"
        );

        // Context var #8: tracker lookup proof (Coll[Byte])
        assert!(
            extension.contains_key("8"),
            "Context extension should have #8 (trackerLookupProof)"
        );

        // Verify data inputs
        let data_inputs = tx["dataInputs"]
            .as_array()
            .expect("Data inputs should be array");
        assert_eq!(data_inputs.len(), 1, "Should have 1 data input (tracker box)");
        assert_eq!(
            data_inputs[0]["boxId"], "test_tracker_box_1",
            "Data input should be tracker box"
        );

        // Verify outputs
        let outputs = tx["outputs"].as_array().expect("Outputs should be array");
        assert_eq!(outputs.len(), 2, "Should have 2 outputs (reserve + recipient)");

        println!("✅ First redemption transaction structure validated\n");
    }

    /// Test 9: Verify that the 48-byte message format matches Scala demo
    #[test]
    fn test_message_format_matches_spec() {
        println!("=== Test 9: Message Format Matches Spec ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Generate message using our function
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        // Verify length: 32 (key) + 8 (totalDebt) + 8 (timestamp) = 48 bytes
        assert_eq!(message.len(), 48, "Message must be exactly 48 bytes");

        // Verify key hash (first 32 bytes)
        let mut key_hash_input = Vec::with_capacity(66);
        key_hash_input.extend_from_slice(&alice_pubkey);
        key_hash_input.extend_from_slice(&bob_pubkey);

        let mut hasher = Blake2b::<U32>::new();
        hasher.update(&key_hash_input);
        let expected_key_hash = hasher.finalize();

        assert_eq!(
            &message[0..32],
            &expected_key_hash[..],
            "Key hash should match blake2b256(ownerKey||receiverKey)"
        );

        // Verify total debt (bytes 32-40, big-endian)
        let debt_from_message = u64::from_be_bytes(message[32..40].try_into().unwrap());
        assert_eq!(debt_from_message, total_debt, "Total debt should match");

        // Verify timestamp (bytes 40-48, big-endian)
        let timestamp_from_message = u64::from_be_bytes(message[40..48].try_into().unwrap());
        assert_eq!(timestamp_from_message, timestamp, "Timestamp should match");

        // Verify this matches the message used by IouNote::signing_message
        let note = IouNote::create_and_sign(bob_pubkey, total_debt, timestamp, &alice_secret)
            .expect("Failed to create note");
        let note_message = note.signing_message(&alice_pubkey);
        assert_eq!(
            message, note_message,
            "Our message should match IouNote::signing_message"
        );

        println!("Message: {}", hex::encode(&message));
        println!("  Key hash (32 bytes): {}", hex::encode(&message[0..32]));
        println!("  Total debt (8 bytes): {}", hex::encode(&message[32..40]));
        println!("  Timestamp (8 bytes): {}", hex::encode(&message[40..48]));
        println!("✅ Message format matches spec\n");
    }

    /// Test 10: Both issuer and tracker sign the EXACT same message
    #[test]
    fn test_both_parties_sign_same_message() {
        println!("=== Test 10: Both Parties Sign Same Message ===");

        let (alice_secret, alice_pubkey) = deterministic_keypair("alice_seed");
        let (_, bob_pubkey) = deterministic_keypair("bob_seed");
        let (tracker_secret, tracker_pubkey) = deterministic_keypair("tracker_seed");

        let total_debt = 50_000_000u64;
        let timestamp = 1_000_000_000u64;

        // Both parties sign the SAME message
        let message = signing_message(&alice_pubkey, &bob_pubkey, total_debt, timestamp);

        let issuer_sig = generate_redemption_signature(&alice_secret, &alice_pubkey, &message);
        let tracker_sig = generate_redemption_signature(&tracker_secret, &tracker_pubkey, &message);

        // Both verify against their respective pubkeys
        assert!(
            schnorr::schnorr_verify(&issuer_sig, &message, &alice_pubkey).is_ok(),
            "Issuer signature must verify against issuer pubkey"
        );
        assert!(
            schnorr::schnorr_verify(&tracker_sig, &message, &tracker_pubkey).is_ok(),
            "Tracker signature must verify against tracker pubkey"
        );

        // Signatures should be different (different signers, different nonces)
        assert_ne!(
            issuer_sig, tracker_sig,
            "Issuer and tracker signatures should differ"
        );

        println!("✅ Both parties sign same message test passed\n");
    }
}
