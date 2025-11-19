//! Contract compilation utilities for Basis tracker

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Ergo-lib not available: {0}")]
    ErgoLibUnavailable(String),
}

/// Get the Basis reserve contract P2S address
pub fn get_basis_reserve_contract_p2s() -> Result<String, CompilerError> {
    // Return the compiled Basis reserve contract P2S address
    Ok("AtC4LmBhPrHQJkS4yxCS5pxFoxLvZ7Jhbp4ARvah8LzyXWzRYGXnd7szw6RQiS9npVUidW8nQK6EMHQRfPBFKP7LKxYDw4FVsLDpeArKQ8yk85iJDgDR3QRdVwqSXtQkYVDDsKJA8NXh8caZYBLSdhqAvsejn3bTE2RzLYWdt2xsuB9BF9GJm8GjBwH6WGcBQaJtzPTe4rKzgFqT1nFyHJsAiT6EWv3dPivf519CA6oKBm9deAfe8xqvSRjbBL147E2bJE5MtCu5TmDp3Vv4YQV3AXuQawYemvQxZxQCzyEBCTcYpegZjJaNSpYYBRRFUevjKmvyyBHgwSnLqKHk1BN2gpAh4d2EXxRoXbSLALXoSjHQ3kDUtpvjiRpFh1BvC8YxY5vmTWzhtvpzt6evHcvT7Gqp6FvcHuwKw3m4AxsUVdhgHEuXiXK6qTjKDtdf7X5HjNChLLvKhuwvyjzswweopJnARkqzy2UKwdMQr9VYtJ5qwxngqd9CfJaP3yVjnSLF7jQPThFUvSW7TUijPnmzTHHVH6sPArDhTV7tsqxQifPrUC".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::address::AddressEncoder;
    use ergo_lib::ergotree_ir::address::NetworkPrefix;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    #[test]
    fn test_contract_compilation_placeholder() {
        // Test that we can get the Basis reserve contract P2S
        let p2s = get_basis_reserve_contract_p2s().unwrap();
        assert!(!p2s.is_empty());
        // The P2S should be a valid P2S address
        assert!(p2s.len() > 50);
    }

    #[test]
    fn test_sigma_serialized_bytes_matches_expected() {
        // Test that the sigma_serialized_bytes for the address "AtC4..." returns the expected bytes
        let p2s_address = get_basis_reserve_contract_p2s().unwrap();
        
        // Parse the address to get the ErgoTree
        let address_encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
        let address = address_encoder.parse_address_from_str(&p2s_address)
            .expect("Failed to parse P2S address");
        let ergo_tree = address.script()
            .expect("Failed to get script from address");
        
        // Serialize the ErgoTree to bytes (this gives us the raw ErgoTree bytes)
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes();
        let ergo_tree_hex = hex::encode(&ergo_tree_bytes);
        
        // The expected bytes from the scanner include the ByteArrayConstant wrapper
        // This is what the Ergo node expects when registering scans
        let _expected_bytes_hex = "0e97031994031004140414040004000442040004420400044205000400048090e4c004044204020580a8d6b9070100d805d6017ee4e3000204d6029d72017300d603e4c6a70407d604b2a59e7201730100d605ededed93c27204c2a793db63087204db6308a793e4c672040407720393e4c67204060ee4c6a7060e959372027302d80dd606db07027203d607e4e30107d608cbb37206db07027207d609e4e30405d60a7a7209d60bdb6a01ddd60ce4e3020ed60db4720c73037304d60ee4e30305d60fb3b372087a720e720ad610e4e3060ed611b4721073057306d612e4c6b2db6501fe7307000407ea02d1ededed720593e4dc640ce4c6a705640283013c0e0e86027208720ae4e3050ee4c672040564939f720b7bb4720c7308b1720ca0ee720d9f72037bcbb3b3720d720f7206ed9099c1a7c17204720eeced91720973099199db6807b2db6502fe730a0072097e730b05939f720b7bb47210730cb17210a0ee72119f72127bcbb3b37211720fdb07027212cd720795937202730dd1eded72059299c17204c1a7730e93e4c672040564e4c6a70564d1730f";
        
        // For now, we'll verify that we can parse the address and get the ErgoTree
        // The actual byte serialization with ByteArrayConstant wrapper would require
        // additional serialization logic that matches the Scala implementation
        assert!(!ergo_tree_hex.is_empty(), "ErgoTree bytes should not be empty");
        assert_eq!(ergo_tree_hex, "1994031004140414040004000442040004420400044205000400048090e4c004044204020580a8d6b9070100d805d6017ee4e3000204d6029d72017300d603e4c6a70407d604b2a59e7201730100d605ededed93c27204c2a793db63087204db6308a793e4c672040407720393e4c67204060ee4c6a7060e959372027302d80dd606db07027203d607e4e30107d608cbb37206db07027207d609e4e30405d60a7a7209d60bdb6a01ddd60ce4e3020ed60db4720c73037304d60ee4e30305d60fb3b372087a720e720ad610e4e3060ed611b4721073057306d612e4c6b2db6501fe7307000407ea02d1ededed720593e4dc640ce4c6a705640283013c0e0e86027208720ae4e3050ee4c672040564939f720b7bb4720c7308b1720ca0ee720d9f72037bcbb3b3720d720f7206ed9099c1a7c17204720eeced91720973099199db6807b2db6502fe730a0072097e730b05939f720b7bb47210730cb17210a0ee72119f72127bcbb3b37211720fdb07027212cd720795937202730dd1eded72059299c17204c1a7730e93e4c672040564e4c6a70564d1730f",
            "ErgoTree bytes don't match expected raw bytes");
        
        // Note: The full ByteArrayConstant serialization would require:
        // ByteArrayConstant(ErgoTreeSerializer.DefaultSerializer.serializeErgoTree(script))
        // followed by ValueSerializer.serialize(v)
        // This matches the Scala implementation pattern you mentioned
    }

    #[test]
    fn test_reserve_scan_contains_expected_bytearrayconstant_bytes() {
        // Test that the reserve scan contains exactly the expected ByteArrayConstant-wrapped bytes
        // This matches the Scala pattern: ByteArrayConstant(ErgoTreeSerializer.DefaultSerializer.serializeErgoTree(script))
        let p2s_address = get_basis_reserve_contract_p2s().unwrap();
        
        // Parse the address to get the ErgoTree
        let address_encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
        let address = address_encoder.parse_address_from_str(&p2s_address)
            .expect("Failed to parse P2S address");
        let ergo_tree = address.script()
            .expect("Failed to get script from address");
        
        // Get the raw ErgoTree bytes
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes();
        
        // Create a ByteArrayConstant with the ErgoTree bytes
        // In Rust ergo-lib, this would be equivalent to Constant::from(ergo_tree_bytes)
        let byte_array_constant = ergo_lib::ergotree_ir::mir::constant::Constant::from(ergo_tree_bytes);
        
        // Serialize the ByteArrayConstant to bytes
        // This matches the Scala pattern: ValueSerializer.serialize(ByteArrayConstant(...))
        let serialized_bytes = byte_array_constant.sigma_serialize_bytes();
        let serialized_hex = hex::encode(&serialized_bytes);
        
        // The expected ByteArrayConstant-wrapped bytes that the Ergo node expects for scan registration
        let expected_bytes_hex = "0e97031994031004140414040004000442040004420400044205000400048090e4c004044204020580a8d6b9070100d805d6017ee4e3000204d6029d72017300d603e4c6a70407d604b2a59e7201730100d605ededed93c27204c2a793db63087204db6308a793e4c672040407720393e4c67204060ee4c6a7060e959372027302d80dd606db07027203d607e4e30107d608cbb37206db07027207d609e4e30405d60a7a7209d60bdb6a01ddd60ce4e3020ed60db4720c73037304d60ee4e30305d60fb3b372087a720e720ad610e4e3060ed611b4721073057306d612e4c6b2db6501fe7307000407ea02d1ededed720593e4dc640ce4c6a705640283013c0e0e86027208720ae4e3050ee4c672040564939f720b7bb4720c7308b1720ca0ee720d9f72037bcbb3b3720d720f7206ed9099c1a7c17204720eeced91720973099199db6807b2db6502fe730a0072097e730b05939f720b7bb47210730cb17210a0ee72119f72127bcbb3b37211720fdb07027212cd720795937202730dd1eded72059299c17204c1a7730e93e4c672040564e4c6a70564d1730f";
        
        // Verify the reserve scan contains exactly the expected ByteArrayConstant-wrapped bytes
        assert_eq!(
            serialized_hex, expected_bytes_hex,
            "Reserve scan ByteArrayConstant bytes do not match expected bytes.\nGot: {}\nExpected: {}",
            serialized_hex, expected_bytes_hex
        );
        
        // Also verify that this is what would be sent to the Ergo node for scan registration
        println!("Reserve scan registration would use bytes: {}", serialized_hex);
    }
}
