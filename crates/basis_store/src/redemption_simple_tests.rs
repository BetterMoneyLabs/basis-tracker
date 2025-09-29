//! Simple redemption tests that avoid AVL tree complexity

use crate::{
    schnorr::{self, generate_keypair},
    IouNote, PubKey, RedemptionManager, RedemptionRequest, TrackerStateManager,
};

/// Test 1: Basic redemption validation without AVL tree
#[test]
fn test_basic_redemption_validation() {
    println!("=== Test 1: Basic Redemption Validation ===");

    // Generate test keypairs
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();

    println!("Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("Recipient pubkey: {}", hex::encode(recipient_pubkey));

    // Create a simple note without adding to tracker
    let amount_collected = 1000;
    let timestamp = 1672531200; // Old timestamp

    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        timestamp,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    // Verify the note signature
    let signature_valid = note.verify_signature(&issuer_pubkey).is_ok();
    assert!(signature_valid, "Note signature should be valid");

    // Test outstanding debt calculation
    assert_eq!(note.outstanding_debt(), amount_collected);
    assert!(!note.is_fully_redeemed());

    println!("✅ Basic redemption validation test passed\n");
}

/// Test 2: Time lock validation
#[test]
fn test_time_lock_validation() {
    println!("=== Test 2: Time Lock Validation ===");

    // Generate test keypairs
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();

    // Create a note with recent timestamp
    let amount_collected = 1000;
    let recent_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let recent_note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        recent_timestamp,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    // Create a note with old timestamp
    let old_timestamp = 1672531200; // Jan 1, 2023
    let old_note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        old_timestamp,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    // Calculate minimum redemption time (1 week)
    let one_week = 7 * 24 * 60 * 60;
    let min_redemption_time_recent = recent_timestamp + one_week;
    let min_redemption_time_old = old_timestamp + one_week;

    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Recent note should not be redeemable yet
    let recent_redeemable = current_time >= min_redemption_time_recent;
    assert!(
        !recent_redeemable,
        "Recent note should not be redeemable yet"
    );

    // Old note should be redeemable
    let old_redeemable = current_time >= min_redemption_time_old;
    assert!(old_redeemable, "Old note should be redeemable");

    println!("Recent note redeemable: {}", recent_redeemable);
    println!("Old note redeemable: {}", old_redeemable);
    println!("✅ Time lock validation test passed\n");
}

/// Test 3: Signature verification
#[test]
fn test_signature_verification() {
    println!("=== Test 3: Signature Verification ===");

    // Generate test keypairs
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    let (wrong_issuer_secret, wrong_issuer_pubkey) = generate_keypair();

    // Create a valid note
    let amount_collected = 1000;
    let timestamp = 1672531200;

    let valid_note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        timestamp,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    // Create a note with wrong issuer
    let wrong_note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        timestamp,
        &wrong_issuer_secret.secret_bytes(),
    )
    .unwrap();

    // Verify valid signature
    let valid_signature = valid_note.verify_signature(&issuer_pubkey).is_ok();
    assert!(valid_signature, "Valid signature should verify");

    // Wrong issuer should fail
    let wrong_signature = valid_note.verify_signature(&wrong_issuer_pubkey).is_ok();
    assert!(!wrong_signature, "Wrong issuer should fail verification");

    // Wrong note should fail with correct issuer
    let wrong_note_valid = wrong_note.verify_signature(&issuer_pubkey).is_ok();
    assert!(
        !wrong_note_valid,
        "Wrong note should fail with correct issuer"
    );

    println!("Valid signature: {}", valid_signature);
    println!("Wrong issuer: {}", wrong_signature);
    println!("Wrong note: {}", wrong_note_valid);
    println!("✅ Signature verification test passed\n");
}

/// Test 4: Redemption request structure
#[test]
fn test_redemption_request_structure() {
    println!("=== Test 4: Redemption Request Structure ===");

    // Generate test keypairs
    let (_, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();

    // Create redemption request
    let redemption_request = RedemptionRequest {
        issuer_pubkey: hex::encode(issuer_pubkey),
        recipient_pubkey: hex::encode(recipient_pubkey),
        amount: 1000,
        timestamp: 1672531200,
        reserve_box_id: "test_reserve_box_1".to_string(),
        recipient_address: "test_recipient_address".to_string(),
    };

    // Verify request structure
    assert!(!redemption_request.issuer_pubkey.is_empty());
    assert!(!redemption_request.recipient_pubkey.is_empty());
    assert!(redemption_request.amount > 0);
    assert!(!redemption_request.reserve_box_id.is_empty());
    assert!(!redemption_request.recipient_address.is_empty());

    println!("Issuer pubkey: {}", &redemption_request.issuer_pubkey[..16]);
    println!(
        "Recipient pubkey: {}",
        &redemption_request.recipient_pubkey[..16]
    );
    println!("Amount: {}", redemption_request.amount);
    println!("Reserve box ID: {}", redemption_request.reserve_box_id);
    println!("✅ Redemption request structure test passed\n");
}

/// Test 5: Simulated blockchain data validation
#[test]
fn test_simulated_blockchain_data() {
    println!("=== Test 5: Simulated Blockchain Data ===");

    // Generate test keypairs
    let (_, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();

    // Test Schnorr signature generation
    let (issuer_secret, _) = generate_keypair();
    let (tracker_secret, _) = generate_keypair();

    let message = format!(
        "{}{}{}",
        hex::encode(issuer_pubkey),
        hex::encode(recipient_pubkey),
        1000
    )
    .into_bytes();

    // Generate signatures
    use secp256k1::SecretKey;

    let issuer_secret_key = SecretKey::from_slice(&issuer_secret.secret_bytes()).unwrap();
    let tracker_secret_key = SecretKey::from_slice(&tracker_secret.secret_bytes()).unwrap();

    // Generate public keys
    let secp = secp256k1::Secp256k1::new();
    let issuer_pubkey_bytes =
        secp256k1::PublicKey::from_secret_key(&secp, &issuer_secret_key).serialize();
    let tracker_pubkey_bytes =
        secp256k1::PublicKey::from_secret_key(&secp, &tracker_secret_key).serialize();

    let issuer_sig =
        schnorr::schnorr_sign(&message, &issuer_secret_key, &issuer_pubkey_bytes).unwrap();
    let tracker_sig =
        schnorr::schnorr_sign(&message, &tracker_secret_key, &tracker_pubkey_bytes).unwrap();

    // Verify signatures
    let issuer_valid = schnorr::schnorr_verify(&issuer_sig, &message, &issuer_pubkey_bytes).is_ok();
    let tracker_valid =
        schnorr::schnorr_verify(&tracker_sig, &message, &tracker_pubkey_bytes).is_ok();

    assert!(issuer_valid, "Issuer signature should be valid");
    assert!(tracker_valid, "Tracker signature should be valid");

    println!("Issuer signature valid: {}", issuer_valid);
    println!("Tracker signature valid: {}", tracker_valid);
    println!("✅ Simulated blockchain data test passed\n");
}
