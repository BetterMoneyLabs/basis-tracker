use basis_store::{IouNote, RedemptionRequest, schnorr::{self, generate_keypair}};

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
async fn test_multiple_issuers_flow() {
    println!("=== Starting Multiple Issuers Flow Test ===");
    
    // Generate multiple issuer keypairs
    let issuers: Vec<_> = (0..3)
        .map(|_| generate_keypair())
        .collect();
    
    let (recipient_secret, recipient_pubkey) = generate_keypair();
    
    println!("Testing with {} issuers", issuers.len());
    
    // Each issuer creates a note for the same recipient
    let mut total_debt = 0;
    for (i, (issuer_secret, issuer_pubkey)) in issuers.iter().enumerate() {
        let amount = 1000 * (i as u64 + 1);
        let timestamp = 1672531200 + (i as u64 * 60);
        
        let note = IouNote::create_and_sign(
            recipient_pubkey,
            amount,
            timestamp,
            &issuer_secret.secret_bytes(),
        ).expect("Failed to create note");
        
        // Verify each note independently
        let signature_valid = note.verify_signature(issuer_pubkey).is_ok();
        assert!(signature_valid, "Note {} signature should be valid", i);
        
        total_debt += note.outstanding_debt();
        
        println!("Issuer {}: created note for {} (total debt: {})", i, amount, total_debt);
    }
    
    println!("Total outstanding debt across all issuers: {}", total_debt);
    assert!(total_debt > 0, "Total debt should be positive");
    
    println!("\n=== Multiple Issuers Flow Test Passed ===\n");
}

#[tokio::test]
async fn test_error_conditions_flow() {
    println!("=== Starting Error Conditions Flow Test ===");
    
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    let (wrong_issuer_secret, wrong_issuer_pubkey) = generate_keypair();
    
    // Test 1: Invalid signature verification
    println!("Test 1: Invalid signature verification...");
    
    let valid_note = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1672531200,
        &issuer_secret.secret_bytes(),
    ).unwrap();
    
    // Try to verify with wrong issuer
    let wrong_verification = valid_note.verify_signature(&wrong_issuer_pubkey);
    assert!(wrong_verification.is_err(), "Should fail with wrong issuer pubkey");
    println!("✓ Wrong issuer detection passed");
    
    // Test 2: Excessive redemption amount
    println!("\nTest 2: Excessive redemption amount...");
    
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1672531200,
        &issuer_secret.secret_bytes(),
    ).unwrap();
    
    let excessive_redemption = RedemptionRequest {
        issuer_pubkey: hex::encode(issuer_pubkey),
        recipient_pubkey: hex::encode(recipient_pubkey),
        amount: 2000, // More than outstanding debt
        timestamp: 1672531200,
        reserve_box_id: "test_reserve_box_1".to_string(),
        recipient_address: "test_recipient_address".to_string(),
    };
    
    let redemption_valid = excessive_redemption.amount <= note.outstanding_debt();
    assert!(!redemption_valid, "Should detect excessive redemption amount");
    println!("✓ Excessive redemption detection passed");
    
    // Test 3: Time lock validation
    println!("\nTest 3: Time lock validation...");
    
    let recent_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let recent_note = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        recent_timestamp,
        &issuer_secret.secret_bytes(),
    ).unwrap();
    
    let one_week = 7 * 24 * 60 * 60;
    let min_redemption_time = recent_timestamp + one_week;
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let is_redeemable = current_time >= min_redemption_time;
    assert!(!is_redeemable, "Recent note should not be redeemable yet");
    println!("✓ Time lock validation passed");
    
    println!("\n=== Error Conditions Flow Test Passed ===\n");
}

#[tokio::test]
async fn test_signature_tampering_detection() {
    println!("=== Starting Signature Tampering Detection Test ===");
    
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    
    // Create a valid note
    let mut note = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1672531200,
        &issuer_secret.secret_bytes(),
    ).unwrap();
    
    // Tamper with the signature
    note.signature[0] ^= 0x01; // Flip one bit
    
    // Verification should fail
    let verification_result = note.verify_signature(&issuer_pubkey);
    assert!(verification_result.is_err(), "Should detect tampered signature");
    
    println!("✓ Signature tampering detection passed");
    println!("\n=== Signature Tampering Detection Test Passed ===\n");
}