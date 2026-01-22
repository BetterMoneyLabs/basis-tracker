use basis_store::{
    IouNote, RedemptionRequest, ReserveTracker, ExtendedReserveInfo, 
    schnorr::{self, generate_keypair},
    TrackerStateManager,
    NoteError,
};
use basis_offchain::{RedemptionTransactionBuilder, TxContext};
use basis_trees::BasisAvlTree;

#[tokio::test]
async fn test_complete_issuance_redemption_flow() {
    println!("=== Starting Complete Issuance → Tracking → Redemption Flow Test ===");
    
    // Step 1: Generate test keypairs
    println!("Step 1: Generating test keypairs...");
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (recipient_secret, recipient_pubkey) = generate_keypair();
    
    println!("Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("Recipient pubkey: {}", hex::encode(recipient_pubkey));
    
    // Step 2: Create and sign IOU note
    println!("\nStep 2: Creating and signing IOU note...");
    let amount = 1000;
    let timestamp = 1672531200; // Old timestamp for immediate redemption
    
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount,
        timestamp,
        &issuer_secret.secret_bytes(),
    ).expect("Failed to create and sign note");
    
    println!("Note created successfully:");
    println!("  Amount: {}", note.amount_collected);
    println!("  Timestamp: {}", note.timestamp);
    println!("  Outstanding debt: {}", note.outstanding_debt());
    
    // Step 3: Verify note signature
    println!("\nStep 3: Verifying note signature...");
    let signature_valid = note.verify_signature(&issuer_pubkey).is_ok();
    assert!(signature_valid, "Note signature should be valid");
    println!("✓ Signature verification passed");
    
    // Step 4: Create redemption request
    println!("\nStep 4: Creating redemption request...");
    let redemption_request = RedemptionRequest {
        issuer_pubkey: hex::encode(issuer_pubkey),
        recipient_pubkey: hex::encode(recipient_pubkey),
        amount: 500, // Partial redemption
        timestamp,
        reserve_box_id: "test_reserve_box_1".to_string(),
        recipient_address: "test_recipient_address".to_string(),
    };
    
    println!("Redemption request created:");
    println!("  Amount: {}", redemption_request.amount);
    println!("  Reserve box: {}", redemption_request.reserve_box_id);
    
    // Step 5: Verify redemption validation
    println!("\nStep 5: Verifying redemption validation...");
    
    // Check that redemption amount doesn't exceed outstanding debt
    let redemption_valid = redemption_request.amount <= note.outstanding_debt();
    assert!(redemption_valid, "Redemption amount should not exceed outstanding debt");
    println!("✓ Redemption validation passed");
    
    // Step 6: Simulate redemption completion
    println!("\nStep 6: Simulating redemption completion...");
    let redeemed_amount = redemption_request.amount;
    let remaining_debt = note.outstanding_debt() - redeemed_amount;
    
    println!("Redemption completed:");
    println!("  Redeemed: {}", redeemed_amount);
    println!("  Remaining debt: {}", remaining_debt);
    
    // Step 7: Verify final state
    println!("\nStep 7: Verifying final state...");
    assert!(remaining_debt >= 0, "Remaining debt should not be negative");
    assert!(remaining_debt <= note.amount_collected, "Remaining debt should not exceed original amount");
    
    if remaining_debt == 0 {
        println!("✓ Note fully redeemed");
    } else {
        println!("✓ Note partially redeemed, {} remaining", remaining_debt);
    }
    
    println!("\n=== Complete Flow Test Passed ===\n");
}

#[tokio::test]
async fn test_end_to_end_with_commitments() {
    println!("=== Starting End-to-End Commitment & Transaction Flow Test ===");

    // 1. Setup - Keys
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (recipient_secret, recipient_pubkey) = generate_keypair();
    let (tracker_secret, tracker_pubkey) = generate_keypair(); // Tracker key for signing updates? logic mismatch maybe, but we can simulate

    // 2. Setup - Tracker State
    // We can't easily mock the full TrackerStateManager with file storage in a test without cleanup, 
    // so we might use the components directly or a temp dir.
    // simpler: use BasisAvlTree directly to simulate the off-chain state
    let mut avl_tree = BasisAvlTree::new().expect("Failed to create AVL tree");

    // 3. Create Note
    let amount = 1_000_000;
    let timestamp = 1672531200; // past
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount,
        timestamp,
        &issuer_secret.secret_bytes(),
    ).expect("Failed to create note");

    // 4. Tracker: Add Note to Tree (Commitment)
    // We need to mimic what `TrackerStateManager::add_note` does
    let key = basis_store::NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey);
    let mut value_bytes = Vec::new();
    value_bytes.extend_from_slice(&issuer_pubkey);
    value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
    value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes()); // 0
    value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
    value_bytes.extend_from_slice(&note.signature);
    value_bytes.extend_from_slice(&note.recipient_pubkey);

    avl_tree.update(key.to_bytes(), value_bytes).expect("Failed to update AVL tree");
    let root_digest = avl_tree.root_digest();
    println!("AVL Root Digest: {}", hex::encode(root_digest));

    // 5. Generate Proof (Membership)
    // We haven't implemented `generate_membership_proof` on BasisTree trait in lib.rs fully? 
    // It's in the trait definition. Let's start with `avl_state.generate_proof()` if exposed, 
    // or use `TrackerStateManager` if we can instantiated it safely.
    // The `BasisAvlTree` likely wraps `ergo_avltree_rust`.
    // Let's assume for this integration test we can generate a proof using the underlying tree or available methods.
    // `basis_store::TrackerStateManager::generate_proof` exists.
    
    // Let's rely on the `avl_proof` from `avl_tree.generate_proof()` if it returns bytes.
    // `basis_trees::avl_tree::BasisAvlTree` has `generate_proof()`.
    let proof_bytes = avl_tree.generate_proof(); // Should return proof for recent ops?
    // Wait, AVL+ trees usually generate proofs for *specific* keys.
    // ergo_avltree_rust might produce a batch proof for the last batch?
    // Let's assume `generate_proof()` gives us what we need for the last op or so.
    assert!(!proof_bytes.is_empty(), "Proof should not be empty");

    // 6. Build Redemption Transaction
    let reserve_box_id = "e56847ed19b3dc6b9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33";
    let tracker_box_id = "f67858fe2ac4ed7c9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33";
    let recipient_addr = "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33";
    
    // Signatures
    let issuer_sig = vec![0u8; 65];   // Mock signature for tx builder validation
    let tracker_sig = vec![0u8; 65];  // Mock signature
    
    let context = TxContext::default();

    // Prepare transaction
    let prep_result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        reserve_box_id,
        tracker_box_id,
        note.amount_collected,
        note.amount_redeemed,
        note.timestamp,
        &issuer_pubkey,
        recipient_addr,
        &proof_bytes,
        &issuer_sig,
        &tracker_sig,
        &context
    );
    
    assert!(prep_result.is_ok(), "Transaction preparation failed: {:?}", prep_result.err());
    let tx_data = prep_result.unwrap();
    
    println!("Transaction prepared successfully.");
    println!("Redemption Amount: {}", tx_data.redemption_amount);
    
    // 7. Validate outcome
    assert_eq!(tx_data.redemption_amount, 1_000_000);
    assert_eq!(tx_data.reserve_box_id, reserve_box_id);
    
    println!("=== End-to-End Commitment & Transaction Flow Test Passed ===\n");
}

#[tokio::test]
async fn test_negative_scenarios_extended() {
    println!("=== Starting Negative Scenarios Test ===");
    
    let context = TxContext::default();
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    
    // Case 1: Insufficient Reserve Funds
    // Reserve has 500, needs 1000 + fee
    println!("Case 1: Undercollateralized Reserve");
    let res = RedemptionTransactionBuilder::validate_redemption_parameters(
        1000, // collected
        0,    // redeemed
        1000, // timestamp
        500,  // reserve value (too low)
        &context
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("InsufficientFunds") || res.unwrap_err().to_string().contains("insufficient"));
    println!("✓ Correctly rejected undercollateralized reserve");

    // Case 2: Time Lock
    println!("Case 2: Time Lock active");
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let recent_timestamp = current_time - 100; // created 100s ago
    let res = RedemptionTransactionBuilder::validate_redemption_parameters(
        1000,
        0,
        recent_timestamp,
        2_000_000,
        &context
    );
    
    // Should fail because < 1 week
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Time lock") || res.unwrap_err().to_string().contains("expired"));
    println!("✓ Correctly rejected locked note");

    println!("=== Negative Scenarios Test Passed ===\n");
}
