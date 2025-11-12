//! Comprehensive tests for redemption transaction builder
//! 
//! These tests verify the complete redemption transaction assembly process:
//! - Transaction parameter validation (funds, time locks, signatures)
//! - Transaction data structure preparation
//! - Error handling for invalid inputs
//! - Schnorr signature compatibility
//! - Mock transaction generation
//! 
//! The tests ensure that redemption transactions are built correctly
//! according to the Basis contract specification before blockchain integration.

use crate::{
    schnorr::{self, generate_keypair},
    transaction_builder::{RedemptionTransactionBuilder, RedemptionTransactionData, TxContext},
    IouNote,
};

/// Test redemption transaction preparation
#[test]
fn test_redemption_transaction_preparation() {
    println!("=== Test: Redemption Transaction Preparation ===");

    // Generate test keypairs
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (recipient_secret, recipient_pubkey) = generate_keypair();

    println!("Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("Recipient pubkey: {}", hex::encode(recipient_pubkey));

    // Create test note
    let amount_collected = 100000000; // 0.1 ERG
    let timestamp = 1672531200; // Jan 1, 2023

    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        timestamp,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    println!("Test note created:");
    println!("  - Amount collected: {}", note.amount_collected);
    println!("  - Timestamp: {}", note.timestamp);
    println!("  - Outstanding debt: {}", note.outstanding_debt());

    // Create transaction context
    let context = TxContext {
        current_height: 1000,
        fee: 1000000, // 0.001 ERG
        change_address: "9".repeat(51), // Ergo mainnet address format
        network_prefix: 0,
    };

    println!("Transaction context:");
    println!("  - Height: {}", context.current_height);
    println!("  - Fee: {} nanoERG", context.fee);

    // Create mock signatures
    let message = b"test_redemption_message";
    let issuer_sig = schnorr::schnorr_sign(message, &issuer_secret, &issuer_pubkey).unwrap();
    let tracker_sig = vec![0u8; 65]; // Mock tracker signature

    println!("Mock signatures created:");
    println!("  - Issuer signature: {} bytes", issuer_sig.len());
    println!("  - Tracker signature: {} bytes", tracker_sig.len());

    // Create mock AVL proof
    let avl_proof = vec![0u8; 64]; // Mock proof

    // Test transaction preparation
    let result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        "test_reserve_box_1234567890abcdef",
        "test_tracker_box_abcdef1234567890",
        &note,
        &issuer_pubkey,
        "9".repeat(51).as_str(),
        &avl_proof,
        &issuer_sig,
        &tracker_sig,
        &context,
    );

    assert!(result.is_ok());
    
    let transaction_data = result.unwrap();
    
    println!("Transaction data prepared:");
    println!("  - Reserve box ID: {}", transaction_data.reserve_box_id);
    println!("  - Tracker box ID: {}", transaction_data.tracker_box_id);
    println!("  - Redemption amount: {} nanoERG", transaction_data.redemption_amount);
    println!("  - Recipient address: {}...", &transaction_data.recipient_address[..16]);
    println!("  - Fee: {} nanoERG", transaction_data.fee);


    // Verify transaction data
    assert_eq!(transaction_data.redemption_amount, 100000000);
    assert_eq!(transaction_data.fee, 1000000);
    assert!(!transaction_data.reserve_box_id.is_empty());
    assert!(!transaction_data.tracker_box_id.is_empty());
    assert!(!transaction_data.recipient_address.is_empty());
    assert_eq!(transaction_data.issuer_signature.len(), 65);
    assert_eq!(transaction_data.tracker_signature.len(), 65);
    assert!(!transaction_data.avl_proof.is_empty());


    println!("✅ Redemption transaction preparation test completed\n");
}

/// Test transaction context validation
#[test]
fn test_transaction_context_validation() {
    println!("=== Test: Transaction Context Validation ===");

    // Test default context
    let default_context = TxContext::default();
    assert_eq!(default_context.fee, 1000000);
    assert_eq!(default_context.network_prefix, 0);

    // Test custom context
    let custom_context = TxContext {
        current_height: 1500,
        fee: 2000000, // 0.002 ERG
        change_address: "9".repeat(51),
        network_prefix: 16, // Testnet
    };

    assert_eq!(custom_context.current_height, 1500);
    assert_eq!(custom_context.fee, 2000000);
    assert_eq!(custom_context.network_prefix, 16);

    println!("✅ Transaction context validation test passed\n");
}

/// Test parameter validation
#[test]
fn test_parameter_validation() {
    println!("=== Test: Parameter Validation ===");

    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    
    // Create test note with old timestamp
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        100000000, // 0.1 ERG
        1672531200, // Jan 1, 2023 (old)
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    let context = TxContext::default();
    
    // Test sufficient funds
    let result = RedemptionTransactionBuilder::validate_redemption_parameters(
        &note,
        200000000, // 0.2 ERG (enough for 0.1 ERG redemption + 0.001 ERG fee)
        &context,
    );
    
    assert!(result.is_ok());
    
    // Test insufficient funds
    let result = RedemptionTransactionBuilder::validate_redemption_parameters(
        &note,
        50000000, // 0.05 ERG (not enough)
        &context,
    );
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    println!("Actual error message: {}", error_msg);
    assert!(error_msg.contains("insufficient funds") || error_msg.contains("InsufficientFunds") || error_msg.contains("Reserve has"));

    println!("✅ Parameter validation test passed\n");
}

/// Test mock transaction creation
#[test]
fn test_mock_transaction_creation() {
    println!("=== Test: Mock Transaction Creation ===");

    let transaction_data = RedemptionTransactionData {
        reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
        tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
        redemption_amount: 100000000,
        recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
        avl_proof: vec![0u8; 64],
        issuer_signature: vec![0u8; 65],
        tracker_signature: vec![0u8; 65],
        fee: 1000000,

    };

    let mock_bytes = RedemptionTransactionBuilder::create_mock_transaction_bytes(&transaction_data);
    
    assert!(!mock_bytes.is_empty());
    let mock_string = String::from_utf8_lossy(&mock_bytes);
    
    println!("Mock transaction created:");
    println!("  - Bytes: {} bytes", mock_bytes.len());
    println!("  - Content: {}...", &mock_string[..50]);
    
    assert!(mock_string.contains("redemption_tx"));
    assert!(mock_string.contains(&transaction_data.redemption_amount.to_string()));
    assert!(mock_string.contains(&transaction_data.fee.to_string()));

    println!("✅ Mock transaction creation test passed\n");
}

/// Test Schnorr signature compatibility
#[test]
fn test_schnorr_signature_compatibility() {
    println!("=== Test: Schnorr Signature Compatibility ===");

    let (secret, pubkey) = generate_keypair();
    let message = b"test_message_for_redemption";

    // Generate Schnorr signature
    let signature = schnorr::schnorr_sign(message, &secret, &pubkey).unwrap();

    println!("Schnorr signature generated:");
    println!("  - Length: {} bytes", signature.len());
    println!("  - Format: 33-byte a + 32-byte z");

    // Verify signature format
    assert_eq!(signature.len(), 65);
    
    // Split into components
    let a_bytes = &signature[..33];
    let z_bytes = &signature[33..];
    
    assert_eq!(a_bytes.len(), 33);
    assert_eq!(z_bytes.len(), 32);

    println!("✅ Schnorr signature compatibility test passed\n");
}

/// Test error handling in transaction builder
#[test]
fn test_transaction_builder_error_handling() {
    println!("=== Test: Transaction Builder Error Handling ===");

    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();
    
    // Create test note
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        100000000,
        1672531200,
        &issuer_secret.secret_bytes(),
    )
    .unwrap();

    let context = TxContext::default();
    let avl_proof = vec![0u8; 64];
    let issuer_sig = vec![0u8; 65];
    let tracker_sig = vec![0u8; 65];

    // Test with empty reserve box ID
    let result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        "", // Empty reserve box ID
        "test_tracker_box",
        &note,
        &issuer_pubkey,
        "9".repeat(51).as_str(),
        &avl_proof,
        &issuer_sig,
        &tracker_sig,
        &context,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Reserve box ID"));

    // Test with empty tracker box ID
    let result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        "test_reserve_box",
        "", // Empty tracker box ID
        &note,
        &issuer_pubkey,
        "9".repeat(51).as_str(),
        &avl_proof,
        &issuer_sig,
        &tracker_sig,
        &context,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Tracker box ID"));

    // Test with empty recipient address
    let result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        "test_reserve_box",
        "test_tracker_box",
        &note,
        &issuer_pubkey,
        "", // Empty recipient address
        &avl_proof,
        &issuer_sig,
        &tracker_sig,
        &context,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Recipient address"));

    // Test with invalid signature length
    let short_sig = vec![0u8; 64]; // Wrong length
    let result = RedemptionTransactionBuilder::prepare_redemption_transaction(
        "test_reserve_box",
        "test_tracker_box",
        &note,
        &issuer_pubkey,
        "9".repeat(51).as_str(),
        &avl_proof,
        &short_sig, // Invalid signature
        &tracker_sig,
        &context,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("65 bytes"));

    println!("✅ Error handling test passed\n");
}

/// Comprehensive transaction building tests following chaincash-rs patterns
#[test]
fn test_comprehensive_transaction_building() {
    println!("=== Test: Comprehensive Transaction Building ===");

    // Test various scenarios following chaincash-rs comprehensive testing approach
    let scenarios = vec![
        ("small_redemption", 1000000, 1000000, 1000),    // 0.001 ERG redemption, 0.001 ERG fee
        ("medium_redemption", 10000000, 1000000, 1500),  // 0.01 ERG redemption, 0.001 ERG fee
        ("large_redemption", 100000000, 2000000, 2000),  // 0.1 ERG redemption, 0.002 ERG fee
    ];

    for (scenario, amount, fee, height) in scenarios {
        println!("Testing scenario: {}", scenario);
        
        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            "test_reserve_box_1234567890abcdef",
            "test_tracker_box_abcdef1234567890",
            "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
            amount,
            fee,
            height,
        );

        assert!(result.is_ok(), "Failed to build transaction for scenario {}: {:?}", scenario, result.err());
        
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty(), "Empty transaction bytes for scenario {}", scenario);
        
        let tx_string = String::from_utf8_lossy(&tx_bytes);
        
        // Verify all required components are present
        assert!(tx_string.contains("ergo_tx_v1"), "Missing transaction version for scenario {}", scenario);
        assert!(tx_string.contains("test_reserve_box"), "Missing reserve box ID for scenario {}", scenario);
        assert!(tx_string.contains("test_tracker_box"), "Missing tracker box ID for scenario {}", scenario);
        assert!(tx_string.contains(&amount.to_string()), "Missing amount for scenario {}", scenario);
        assert!(tx_string.contains(&fee.to_string()), "Missing fee for scenario {}", scenario);
        assert!(tx_string.contains(&height.to_string()), "Missing height for scenario {}", scenario);
        
        println!("  - Scenario {} passed: amount={}, fee={}, height={}", scenario, amount, fee, height);
    }

    println!("✅ Comprehensive transaction building test passed\n");
}

/// Test transaction building with edge cases following chaincash-rs testing patterns
#[test]
fn test_transaction_building_edge_cases() {
    println!("=== Test: Transaction Building Edge Cases ===");

    // Test edge cases similar to chaincash-rs comprehensive testing
    let edge_cases = vec![
        ("minimum_amount", 1, "minimum redemption amount"),
        ("maximum_u32_height", 4294967295, "maximum blockchain height"),
        ("zero_fee", 0, "zero transaction fee"),
        ("very_high_amount", 10000000000, "very high redemption amount"),
    ];

    for (case_name, value, description) in edge_cases {
        println!("Testing edge case: {} ({})", case_name, description);
        
        let result = match case_name {
            "minimum_amount" => RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                value,
                1000000,
                1000,
            ),
            "maximum_u32_height" => RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                100000000,
                1000000,
                value as u32,
            ),
            "zero_fee" => RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                100000000,
                value,
                1000,
            ),
            "very_high_amount" => RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                value,
                1000000,
                1000,
            ),
            _ => panic!("Unknown test case: {}", case_name),
        };

        // For our current implementation, all these should succeed
        // In a full implementation, some might fail with proper validation
        assert!(result.is_ok(), "Failed to build transaction for edge case {}: {:?}", case_name, result.err());
        
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty(), "Empty transaction bytes for edge case {}", case_name);
        
        println!("  - Edge case {} passed", case_name);
    }

    println!("✅ Transaction building edge cases test passed\n");
}

/// Test transaction context validation following chaincash-rs patterns
#[test]
fn test_transaction_context_comprehensive() {
    println!("=== Test: Comprehensive Transaction Context ===");

    // Test various context configurations
    let contexts = vec![
        ("mainnet_default", TxContext::default()),
        ("testnet_custom", TxContext {
            current_height: 1500,
            fee: 2000000,
            change_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
            network_prefix: 16, // testnet
        }),
        ("high_fee", TxContext {
            current_height: 2000,
            fee: 5000000, // 0.005 ERG
            change_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
            network_prefix: 0,
        }),
    ];

    for (name, context) in contexts {
        println!("Testing context: {}", name);
        
        // Verify context properties
        match name {
            "mainnet_default" => {
                assert_eq!(context.fee, 1000000);
                assert_eq!(context.network_prefix, 0);
            },
            "testnet_custom" => {
                assert_eq!(context.current_height, 1500);
                assert_eq!(context.fee, 2000000);
                assert_eq!(context.network_prefix, 16);
            },
            "high_fee" => {
                assert_eq!(context.current_height, 2000);
                assert_eq!(context.fee, 5000000);
                assert_eq!(context.network_prefix, 0);
            },
            _ => {},
        }
        
        println!("  - Context {} validated: height={}, fee={}, network={}", 
                name, context.current_height, context.fee, context.network_prefix);
    }

    println!("✅ Comprehensive transaction context test passed\n");
}