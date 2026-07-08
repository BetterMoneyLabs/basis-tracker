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
    Ok("4ZhBzJfNoUL9Bp993NzJcdUr6CNfuwvwNMgHC2JPHs8ane1jjE3K7gzUQVBNQfJccoLbB2P8xMsa9qZNFgRwgrWs6WGEa38gwF1BDkGwMLh6RJUez5Ge6toZzu7tZo5qYtqUinmckb5q9hcVo6Cpn3w2gcuwCd2sKmRohedxxbpP7vnrQmCNQveB22RN5ZVv8VGJaDUEC3ADCSRjzr5ZzJNBmVbAw2k5sTmoXGm7qJ1YT9gzmAPi97ptJJQXqNJoi1W6coMFwg34Dc21K9TMkKQexnXxon21XrbyWL6fzLGbYBRBiVpiRTeMah9Tc33yN93NVTjHWKvBcxSYiJU7eJy6aiwAHhqxYPtZNhwE196qUEYHX5gnN1xB4CpZA2W2HDuEZREpDPV4xy6g2qucW2fyhgDpscHMxrbaGfRq1zkrvML54z2Da9jpkM6nmZx2KB29HTh1do6L3rrLxnvg5cgANzfYuaWPFEoo6j2ZqjPzLDeSSVhPbkMnw6HhQp2qtzayqWVgCKGRzMFuh8BkpmkFCPKjhUwX6Dgv6DpkuHbJRM7k9YSvPCHRQTSeDJa4B5wuyXMsfFMkAnjR4oaLbSBU2QCgKBLFbGvrRKgAJG9eTSc31x6EtqKFoLN2urEWGsEh1F6cxDh2Ma3izwFLyHAgCcUurRXndm5gy3U4GpKdaJiWtwfhcZspwtJ72gWUBEzuPdcqjEyBc95jVtubHeN95QcZLJkJM88c6m1DPXaTBSfDpL8s3sBySa7".to_string())
}

/// Get the Basis reserve contract ErgoTree hex (for reserve output in redemption transactions)
pub fn get_basis_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    // This is the raw ErgoTree hex for the P2S contract
    // Can be obtained by parsing the P2S address and serializing the script
    Ok("10180414041404000400041004200500040004420400040004000410050004420500040004420442010104e0210402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d813d606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be4e30405d60ce3070ed60de6720cd60e7a720bd60f95720d7cb4e4dc640ae4c6a70564027209e4720c730473057306d61099c1a7c17203d611db6a01ddd612e4e3020ed613b4721273077308d614b3b372097a720a720ed615e4e3060ed616b17215d6179172167309d618e4c672060407ea02d1edededededed7205938cb2db63087206730a0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a91720b95720d7cb4e4dc640ae4c6a70564027209e4720c730b730c730d93e4dc640ce4c6a705640283013c0e0e86027209b3720e7a9a720f7210e4e3050ee4c672030564939f72117bb47212730eb17212a0ee72139f72047bcbb3b3721372147207eded917210730f90721099720a720f957217957217d801d619b4721573107311939f72117bb4721573127216a0ee72199f72187bcbb3b372197214db0702721873139199a38cc77206017314cd7208959372027315d1eded720593e4c672030564e4c6a705649299c17203c1a77316d17317".to_string())
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
        let address = address_encoder
            .parse_address_from_str(&p2s_address)
            .expect("Failed to parse P2S address");
        let ergo_tree = address.script().expect("Failed to get script from address");

        // Serialize the ErgoTree to bytes (this gives us the raw ErgoTree bytes)
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes();
        let ergo_tree_hex = hex::encode(&ergo_tree_bytes);

        // For now, we'll verify that we can parse the address and get the ErgoTree
        // The actual byte serialization with ByteArrayConstant wrapper would require
        // additional serialization logic that matches the Scala implementation
        assert!(
            !ergo_tree_hex.is_empty(),
            "ErgoTree bytes should not be empty"
        );
        // Updated expected ErgoTree bytes for current P2S address
        assert_eq!(ergo_tree_hex, "10180414041404000400041004200500040004420400040004000410050004420500040004420442010104e0210402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d813d606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be4e30405d60ce3070ed60de6720cd60e7a720bd60f95720d7cb4e4dc640ae4c6a70564027209e4720c730473057306d61099c1a7c17203d611db6a01ddd612e4e3020ed613b4721273077308d614b3b372097a720a720ed615e4e3060ed616b17215d6179172167309d618e4c672060407ea02d1edededededed7205938cb2db63087206730a0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a91720b95720d7cb4e4dc640ae4c6a70564027209e4720c730b730c730d93e4dc640ce4c6a705640283013c0e0e86027209b3720e7a9a720f7210e4e3050ee4c672030564939f72117bb47212730eb17212a0ee72139f72047bcbb3b3721372147207eded917210730f90721099720a720f957217957217d801d619b4721573107311939f72117bb4721573127216a0ee72199f72187bcbb3b372197214db0702721873139199a38cc77206017314cd7208959372027315d1eded720593e4c672030564e4c6a705649299c17203c1a77316d17317",
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
        let address = address_encoder
            .parse_address_from_str(&p2s_address)
            .expect("Failed to parse P2S address");
        let ergo_tree = address.script().expect("Failed to get script from address");

        // Get the raw ErgoTree bytes
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes();

        // Create a ByteArrayConstant with the ErgoTree bytes
        // In Rust ergo-lib, this would be equivalent to Constant::from(ergo_tree_bytes)
        let byte_array_constant =
            ergo_lib::ergotree_ir::mir::constant::Constant::from(ergo_tree_bytes);

        // Serialize the ByteArrayConstant to bytes
        // This matches the Scala pattern: ValueSerializer.serialize(ByteArrayConstant(...))
        let serialized_bytes = byte_array_constant.sigma_serialize_bytes();
        let serialized_hex = hex::encode(&serialized_bytes);

        // The expected ByteArrayConstant-wrapped bytes that the Ergo node expects for scan registration
        // Updated for current P2S address: starts with 0eaa04 (ByteArrayConstant prefix with length)
        let expected_bytes_hex = "0eaa0410180414041404000400041004200500040004420400040004000410050004420500040004420442010104e0210402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d813d606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be4e30405d60ce3070ed60de6720cd60e7a720bd60f95720d7cb4e4dc640ae4c6a70564027209e4720c730473057306d61099c1a7c17203d611db6a01ddd612e4e3020ed613b4721273077308d614b3b372097a720a720ed615e4e3060ed616b17215d6179172167309d618e4c672060407ea02d1edededededed7205938cb2db63087206730a0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a91720b95720d7cb4e4dc640ae4c6a70564027209e4720c730b730c730d93e4dc640ce4c6a705640283013c0e0e86027209b3720e7a9a720f7210e4e3050ee4c672030564939f72117bb47212730eb17212a0ee72139f72047bcbb3b3721372147207eded917210730f90721099720a720f957217957217d801d619b4721573107311939f72117bb4721573127216a0ee72199f72187bcbb3b372197214db0702721873139199a38cc77206017314cd7208959372027315d1eded720593e4c672030564e4c6a705649299c17203c1a77316d17317";

        // Verify the reserve scan contains exactly the expected ByteArrayConstant-wrapped bytes
        assert_eq!(
            serialized_hex, expected_bytes_hex,
            "Reserve scan ByteArrayConstant bytes do not match expected bytes.\nGot: {}\nExpected: {}",
            serialized_hex, expected_bytes_hex
        );

        // Also verify that this is what would be sent to the Ergo node for scan registration
        println!(
            "Reserve scan registration would use bytes: {}",
            serialized_hex
        );
    }
}
