use basis_store::ergo_scanner::{ServerState, ErgoBox, ScanBox};
use std::collections::HashMap;

// Test to verify that the 0x07 prefix is properly stripped from public keys in registers
#[tokio::test]
async fn test_prefix_stripping_in_register_parsing() {
    // Create a mock scan box with a public key that has the 0x07 prefix
    let mut registers = HashMap::new();
    // This is a 33-byte public key with 0x07 prefix (GroupElement format)
    let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";  
    registers.insert("R4".to_string(), prefixed_pubkey.to_string());
    registers.insert("R5".to_string(), "some_tracker_nft_id".to_string());
    
    let scan_box = ScanBox {
        box_id: "test_box_id".to_string(),
        value: 1000000000, // 1 ERG
        creation_height: 1000,
        additional_registers: registers,
        assets: vec![],
    };
    
    // Create a dummy server state (we won't actually use the node connection)
    let config = basis_store::ergo_scanner::NodeConfig::default();
    let _server_state = ServerState::new(config, "http://dummy-node.com".to_string());
    
    // Test the parse_reserve_box function directly
    let result = _server_state.parse_reserve_box(&scan_box);
    
    match result {
        Ok(reserve_info) => {
            // The owner_pubkey should have the 0x07 prefix stripped
            let expected_pubkey = &prefixed_pubkey[2..]; // Remove first 2 characters (07)
            
            assert_eq!(reserve_info.owner_pubkey, expected_pubkey);
            println!("SUCCESS: Prefix was correctly stripped. Original: {}, Stripped: {}", prefixed_pubkey, reserve_info.owner_pubkey);
        },
        Err(e) => {
            panic!("Failed to parse reserve box: {:?}", e);
        }
    }
}

// Test to verify that public keys without prefix are preserved as-is
#[tokio::test]
async fn test_unprefixed_keys_remain_unchanged() {
    // Create a mock scan box with a public key that doesn't have the 0x07 prefix
    let mut registers = HashMap::new();
    // This is a standard 33-byte compressed public key (02 prefix)
    let unprefixed_pubkey = "02c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";  
    registers.insert("R4".to_string(), unprefixed_pubkey.to_string());
    registers.insert("R5".to_string(), "some_tracker_nft_id".to_string());
    
    let scan_box = ScanBox {
        box_id: "test_box_id_2".to_string(),
        value: 1000000000, // 1 ERG
        creation_height: 1000,
        additional_registers: registers,
        assets: vec![],
    };
    
    // Create a dummy server state
    let config = basis_store::ergo_scanner::NodeConfig::default();
    let _server_state = ServerState::new(config, "http://dummy-node.com".to_string());
    
    // Test the parse_reserve_box function directly
    let result = _server_state.parse_reserve_box(&scan_box);
    
    match result {
        Ok(reserve_info) => {
            // The owner_pubkey should remain unchanged
            assert_eq!(reserve_info.owner_pubkey, unprefixed_pubkey);
            println!("SUCCESS: Unprefixed key remained unchanged: {}", reserve_info.owner_pubkey);
        },
        Err(e) => {
            panic!("Failed to parse reserve box: {:?}", e);
        }
    }
}

// Test the normalize_public_key function directly
#[test]
fn test_normalize_public_key_function() {
    // Test with 0x07 prefixed key
    let prefixed_key = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
    let normalized = basis_store::normalize_public_key(prefixed_key);
    let expected = "c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
    assert_eq!(normalized, expected);
    println!("SUCCESS: Prefixed key normalized correctly. Original: {}, Normalized: {}", prefixed_key, normalized);
    
    // Test with standard compressed public key (02 prefix)
    let standard_key = "02c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
    let normalized2 = basis_store::normalize_public_key(standard_key);
    assert_eq!(normalized2, standard_key); // Should remain unchanged
    println!("SUCCESS: Standard key remained unchanged. Original: {}, Normalized: {}", standard_key, normalized2);
    
    // Test with standard compressed public key (03 prefix)
    let standard_key2 = "03d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6";
    let normalized3 = basis_store::normalize_public_key(standard_key2);
    assert_eq!(normalized3, standard_key2); // Should remain unchanged
    println!("SUCCESS: Standard key remained unchanged. Original: {}, Normalized: {}", standard_key2, normalized3);
}