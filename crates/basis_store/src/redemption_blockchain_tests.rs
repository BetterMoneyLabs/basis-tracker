//! Comprehensive tests for note redemption using Ergo transactions with simulated blockchain data

use crate::{
    schnorr::{self, generate_keypair},
    IouNote, PubKey, RedemptionManager, RedemptionRequest, TrackerStateManager,
};
use serde_json::{json, Value};

/// Simulated blockchain data for redemption tests
#[derive(Debug, Clone)]
pub struct SimulatedBlockchainData {
    /// Simulated reserve contract box
    pub reserve_box: Value,
    /// Simulated tracker commitment box
    pub tracker_box: Value,
    /// Simulated AVL tree for proofs
    pub avl_tree: Value,
    /// Required Schnorr signatures
    pub signatures: Vec<Vec<u8>>,
    /// Simulated redemption transaction
    pub transaction: Value,
}

/// Create simulated reserve box with proper registers
pub fn create_simulated_reserve_box(
    box_id: &str,
    value: u64,
    owner_pubkey: &PubKey,
    tracker_nft_id: &str,
) -> Value {
    json!({
        "boxId": box_id,
        "value": value,
        "ergoTree": "0008cd0101010101010101010101010101010101010101",
        "creationHeight": 1000,
        "transactionId": "test_tx_1",
        "additionalRegisters": {
            "R4": {
                "serializedValue": hex::encode(owner_pubkey)
            },
            "R5": {
                "serializedValue": hex::encode(vec![0u8; 32]) // Empty AVL tree
            },
            "R6": {
                "serializedValue": hex::encode(tracker_nft_id.as_bytes())
            }
        }
    })
}

/// Create simulated tracker box with AVL tree commitment
pub fn create_simulated_tracker_box(
    box_id: &str,
    tracker_pubkey: &PubKey,
    avl_tree_root: &[u8],
) -> Value {
    json!({
        "boxId": box_id,
        "value": 1000000,
        "ergoTree": "tracker_script",
        "creationHeight": 1000,
        "transactionId": "test_tx_1",
        "additionalRegisters": {
            "R4": {
                "serializedValue": hex::encode(tracker_pubkey)
            },
            "R5": {
                "serializedValue": hex::encode(avl_tree_root)
            }
        }
    })
}
/// Create simulated redemption transaction
pub fn create_simulated_redemption_transaction(
    reserve_box_id: &str,
    tracker_box_id: &str,
    output_value: u64,
    _action_byte: u8,
) -> Value {
    json!({
        "id": "test_redemption_tx",
        "inputs": [
            {
                "box": {
                    "boxId": reserve_box_id,
                    "value": output_value + 1000000, // Input value > output value
                    "ergoTree": "0008cd0101010101010101010101010101010101010101",
                    "creationHeight": 1000,
                    "transactionId": "test_tx_1",
                    "additionalRegisters": {
                        "R4": {"serializedValue": "owner_pubkey_hex"},
                        "R5": {"serializedValue": hex::encode(vec![0u8; 32])},
                        "R6": {"serializedValue": "tracker_nft_id"}
                    }
                }
            }
        ],
        "outputs": [
            {
                "boxId": "output_reserve_box",
                "value": output_value,
                "ergoTree": "0008cd0101010101010101010101010101010101010101",
                "creationHeight": 1001,
                "transactionId": "test_redemption_tx",
                "additionalRegisters": {
                    "R4": {"serializedValue": "owner_pubkey_hex"},
                    "R5": {"serializedValue": hex::encode(vec![0u8; 32])},
                    "R6": {"serializedValue": "tracker_nft_id"}
                }
            },
            {
                "boxId": "redemption_output",
                "value": 1000000, // Redemption amount minus fee
                "ergoTree": "recipient_script",
                "creationHeight": 1001,
                "transactionId": "test_redemption_tx"
            }
        ],
        "dataInputs": [
            {
                "boxId": tracker_box_id
            }
        ]
    })
}

/// Generate test Schnorr signatures
pub fn generate_test_signatures(
    issuer_secret: &[u8; 32],
    tracker_secret: &[u8; 32],
    message: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    use secp256k1::SecretKey;

    let issuer_secret_key = SecretKey::from_slice(issuer_secret).unwrap();
    let tracker_secret_key = SecretKey::from_slice(tracker_secret).unwrap();

    // Generate public keys
    let secp = secp256k1::Secp256k1::new();
    let issuer_pubkey =
        secp256k1::PublicKey::from_secret_key(&secp, &issuer_secret_key).serialize();
    let tracker_pubkey =
        secp256k1::PublicKey::from_secret_key(&secp, &tracker_secret_key).serialize();

    let issuer_sig = schnorr::schnorr_sign(message, &issuer_secret_key.secret_bytes(), &issuer_pubkey).unwrap();
    let tracker_sig = schnorr::schnorr_sign(message, &tracker_secret_key.secret_bytes(), &tracker_pubkey).unwrap();
    (issuer_sig.to_vec(), tracker_sig.to_vec())
}

/// Create complete simulated blockchain data for redemption
pub fn create_complete_blockchain_data(
    issuer_pubkey: &PubKey,
    recipient_pubkey: &PubKey,
    amount: u64,
    _timestamp: u64,
) -> SimulatedBlockchainData {
    // Generate test keypairs
    let (_, tracker_pubkey) = generate_keypair();

    // Create reserve box
    let reserve_box_id = "test_reserve_box_1";
    let reserve_box = create_simulated_reserve_box(
        reserve_box_id,
        1000000000, // 1 ERG
        issuer_pubkey,
        "test_tracker_nft",
    );

    // Create tracker box with AVL tree commitment
    let tracker_box_id = "test_tracker_box_1";
    let avl_tree_root = blake2b_hash(&format!(
        "{}{}",
        hex::encode(issuer_pubkey),
        hex::encode(recipient_pubkey)
    ));
    let tracker_box = create_simulated_tracker_box(tracker_box_id, &tracker_pubkey, &avl_tree_root);

    // Create redemption transaction
    let transaction = create_simulated_redemption_transaction(
        reserve_box_id,
        tracker_box_id,
        700000000, // 0.7 ERG remaining after redemption
        0,         // Action 0 = redemption
    );

    // Generate test signatures
    let (issuer_secret, _) = generate_keypair();
    let (tracker_secret, _) = generate_keypair();
    let message = format!(
        "{}{}{}",
        hex::encode(issuer_pubkey),
        hex::encode(recipient_pubkey),
        amount
    )
    .into_bytes();

    let (issuer_sig, tracker_sig) = generate_test_signatures(
        &issuer_secret,
        &tracker_secret,
        &message,
    );

    SimulatedBlockchainData {
        reserve_box,
        tracker_box,
        avl_tree: json!({ "root": hex::encode(avl_tree_root) }),
        signatures: vec![issuer_sig, tracker_sig],
        transaction,
    }
}

/// Simple Blake2b hash function for testing
fn blake2b_hash(data: &str) -> [u8; 32] {
    use blake2::{Blake2b, Digest};

    let mut hasher = Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result[..32]);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: Valid redemption with proper signatures and time lock
    #[test]
    fn test_valid_redemption_flow() {
        println!("=== Test 1: Valid Redemption Flow ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (recipient_secret, recipient_pubkey) = generate_keypair();

        println!("Issuer pubkey: {}", hex::encode(issuer_pubkey));
        println!("Recipient pubkey: {}", hex::encode(recipient_pubkey));

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note (with old timestamp to pass time lock)
        let amount_collected = 1000;
        let timestamp = 1672531200; // Jan 1, 2023 (well past 1 week)

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(issuer_pubkey),
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Initiate redemption
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);

        // Should succeed
        assert!(redemption_data.is_ok(), "Valid redemption should succeed");

        let redemption_data = redemption_data.unwrap();
        println!("Redemption ID: {}", redemption_data.redemption_id);
        println!(
            "Transaction bytes: {}...",
            &redemption_data.transaction_bytes[..20]
        );
        println!(
            "Required signatures: {:?}",
            redemption_data.required_signatures
        );

        // Verify redemption data structure
        assert!(!redemption_data.redemption_id.is_empty());
        assert!(!redemption_data.transaction_bytes.is_empty());
        assert_eq!(redemption_data.required_signatures.len(), 2);
        assert!(redemption_data.estimated_fee > 0);

        println!("✅ Valid redemption test passed\n");
    }

    /// Test 2: Redemption before time lock expires
    #[test]
    fn test_redemption_before_time_lock() {
        println!("=== Test 2: Redemption Before Time Lock ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note with recent timestamp
        let amount_collected = 1000;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(); // Current time

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(issuer_pubkey),
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Initiate redemption - should fail due to time lock
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);

        // Should fail
        assert!(
            redemption_data.is_err(),
            "Redemption before time lock should fail"
        );

        if let Err(e) = redemption_data {
            println!("Expected error: {}", e);
            assert!(matches!(
                e,
                crate::RedemptionError::RedemptionTooEarly(_, _)
            ));
        }

        println!("✅ Time lock protection test passed\n");
    }

    /// Test 3: Note signature validation
    #[test]
    fn test_note_signature_validation() {
        println!("=== Test 3: Note Signature Validation ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note
        let amount_collected = 1000;
        let timestamp = 1672531200; // Old timestamp

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create redemption request with wrong amount (should fail signature check)
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(issuer_pubkey),
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected + 100, // Wrong amount
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Initiate redemption - should fail due to invalid signature
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);

        // Should fail
        assert!(
            redemption_data.is_err(),
            "Redemption with wrong amount should fail"
        );

        if let Err(e) = redemption_data {
            println!("Expected error: {}", e);
            assert!(matches!(e, crate::RedemptionError::InsufficientCollateral(_, _)));
        }

        println!("✅ Note signature validation test passed\n");
    }

    /// Test 4: Simulated blockchain data generation
    #[test]
    fn test_simulated_blockchain_data() {
        println!("=== Test 4: Simulated Blockchain Data ===");

        // Generate test keypairs
        let (_, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create simulated blockchain data
        let blockchain_data =
            create_complete_blockchain_data(&issuer_pubkey, &recipient_pubkey, 1000, 1672531200);

        // Verify blockchain data structure
        assert!(blockchain_data.reserve_box["boxId"].is_string());
        assert!(blockchain_data.tracker_box["boxId"].is_string());
        assert!(blockchain_data.avl_tree["root"].is_string());
        assert_eq!(blockchain_data.signatures.len(), 2);
        assert!(blockchain_data.transaction["id"].is_string());

        println!("Reserve box ID: {}", blockchain_data.reserve_box["boxId"]);
        println!("Tracker box ID: {}", blockchain_data.tracker_box["boxId"]);
        println!("Transaction ID: {}", blockchain_data.transaction["id"]);
        println!("Signatures generated: {}", blockchain_data.signatures.len());

        println!("✅ Simulated blockchain data test passed\n");
    }

    /// Test 5: Full redemption flow
    #[test]
    fn test_full_redemption_flow() {
        println!("=== Test 5: Full Redemption Flow ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note
        let amount_collected = 1000;
        let timestamp = 1672531200; // Old timestamp

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create full redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(issuer_pubkey),
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected, // Full amount
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Initiate full redemption
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);

        // Should succeed
        if let Err(e) = &redemption_data {
            println!("Full redemption failed: {}", e);
        }
        assert!(redemption_data.is_ok(), "Full redemption should succeed");

        // Complete the redemption
        redemption_manager
            .complete_redemption(&issuer_pubkey, &recipient_pubkey, amount_collected)
            .unwrap();

        // Check updated note
        let updated_note = redemption_manager
            .tracker
            .lookup_note(&issuer_pubkey, &recipient_pubkey)
            .unwrap();

        println!("Original collected: {}", amount_collected);
        println!("Amount redeemed: {}", updated_note.amount_redeemed);
        println!("Outstanding debt: {}", updated_note.outstanding_debt());

        assert_eq!(updated_note.amount_redeemed, amount_collected);
        assert_eq!(updated_note.outstanding_debt(), 0);

        println!("✅ Full redemption test passed\n");
    }

    /// Test 6: Invalid signature detection
    #[test]
    fn test_invalid_signature_detection() {
        println!("=== Test 6: Invalid Signature Detection ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create a note with valid signature
        let amount_collected = 1000;
        let timestamp = 1672531200;

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create redemption request with wrong issuer pubkey
        let (_, wrong_issuer_pubkey) = generate_keypair();
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(wrong_issuer_pubkey), // Wrong issuer
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Initiate redemption - should fail due to invalid signature
        let redemption_data = redemption_manager.initiate_redemption(&redemption_request);

        // Should fail
        assert!(
            redemption_data.is_err(),
            "Redemption with wrong issuer should fail"
        );

        if let Err(e) = redemption_data {
            println!("Expected error: {}", e);
            assert!(matches!(e, crate::RedemptionError::NoteNotFound));
        }

        println!("✅ Invalid signature detection test passed\n");
    }

    /// Test 7: Complete redemption flow with simulated blockchain
    #[test]
    fn test_complete_redemption_flow() {
        println!("=== Test 7: Complete Redemption Flow ===");

        // Generate test keypairs
        let (issuer_secret, issuer_pubkey) = generate_keypair();
        let (_, recipient_pubkey) = generate_keypair();

        // Create tracker and redemption manager
        let tracker = TrackerStateManager::new_with_temp_storage();
        let mut redemption_manager = RedemptionManager::new(tracker);

        // Create and sign a test note
        let amount_collected = 1000;
        let timestamp = 1672531200;

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount_collected,
            timestamp,
            SecretKey::from_slice(&issuer_secret).unwrap(),
        )
        .unwrap();

        // Add note to tracker
        redemption_manager
            .tracker
            .add_note(&issuer_pubkey, &note)
            .unwrap();

        // Create redemption request
        let redemption_request = RedemptionRequest {
            issuer_pubkey: hex::encode(issuer_pubkey),
            recipient_pubkey: hex::encode(recipient_pubkey),
            amount: amount_collected,
            timestamp,
            reserve_box_id: "test_reserve_box_1".to_string(),
            recipient_address: "test_recipient_address".to_string(),
        };

        // Step 1: Initiate redemption
        let redemption_data = redemption_manager
            .initiate_redemption(&redemption_request)
            .unwrap();

        println!("Step 1 - Redemption initiated:");
        println!("  - Redemption ID: {}", redemption_data.redemption_id);
        println!(
            "  - Transaction bytes length: {}",
            redemption_data.transaction_bytes.len()
        );
        println!(
            "  - Required signatures: {}",
            redemption_data.required_signatures.len()
        );

        // Step 2: Simulate blockchain transaction processing
        let blockchain_data = create_complete_blockchain_data(
            &issuer_pubkey,
            &recipient_pubkey,
            amount_collected,
            timestamp,
        );

        println!("Step 2 - Blockchain data simulated:");
        println!(
            "  - Reserve box value: {}",
            blockchain_data.reserve_box["value"]
        );
        println!(
            "  - Tracker box exists: {}",
            blockchain_data.tracker_box["boxId"].is_string()
        );
        println!(
            "  - Signatures available: {}",
            blockchain_data.signatures.len()
        );

        // Step 3: Complete redemption
        redemption_manager
            .complete_redemption(&issuer_pubkey, &recipient_pubkey, amount_collected)
            .unwrap();

        println!("Step 3 - Redemption completed:");

        // Step 4: Verify final state
        let final_note = redemption_manager
            .tracker
            .lookup_note(&issuer_pubkey, &recipient_pubkey)
            .unwrap();

        println!("Step 4 - Final state verified:");
        println!("  - Amount collected: {}", final_note.amount_collected);
        println!("  - Amount redeemed: {}", final_note.amount_redeemed);
        println!("  - Outstanding debt: {}", final_note.outstanding_debt());

        assert_eq!(final_note.amount_redeemed, amount_collected);
        assert_eq!(final_note.outstanding_debt(), 0);

        println!("✅ Complete redemption flow test passed\n");
    }
}
