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
    Ok("RtQxdWJ9axeb5Ltahqosnhj45BE26xuDK4YWddVj5p59t9RjKPEkkHCYEiyxwRFMJcEHwVd9syFod8ReQo1Zaz9eNTZ5JwDEN5hkLd67sVr2sNQ6R46TSfausAc9D3q7et1apYaXnqV9PkpHPMCA1zMCEsmmADj62XRGq4Cw2VwpuKKCAdreTgmLzdFWHGVGQMsPDFFBkRibsPFMzXkytdy2mPs2zCtm15uyDpd3jDLBy95BtUFXU2DdaYa1xMZE9UXju4R4MhWH8vqWda5BgpRTa1RpQxpS5b96FG46r1v3ZWCLYcVo51J1ekY8cqqVFNNykpQScRRYqFjCLMjG26dYEwZyn21wGeLJ7RzcTwCpvGDBa2w1P3ycAEJAv9XDPEtJrSQpkvBaD1HaZ6X2JuXmFjPF5MChmVLk4CTXtRQVRis7vP95ByTTmbHbtVdao32kbN3xhCWgJZZdaKkNyKH4vFQn5jyoEmiV7FjQDegWnnaFXu5FW6stx9cbhsxWz5FfGpW1BCMRNNJTCRF6FtYoehrMT74LDRNxHQ38EmMn6mBEpSrhkzDj2jysdFJvDUf8UQjLZQLmUQtgNotfxeAPxiavsT5mLUja3hdWvZPv71FcHxvP53WJHAcn9JPek3vepbH9gxRdmBMW".to_string())
}

/// Get the Basis reserve contract ErgoTree hex (for reserve output in redemption transactions)
pub fn get_basis_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    // This is the raw ErgoTree hex for the P2S contract
    // Can be obtained by parsing the P2S address and serializing the script
    Ok("1012041404140400040005000400044204e02105000400044204000442050004420402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d80fd606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be3070ed60c95e6720b7ce4dc640ae4c6a70564027209e4720b7304d60d99c1a7c17203d60edb6a01ddd60fe4e3020ed610b4720f73057306d611959199a38cc77206017307b3b372097a720a7a7308b372097a720ad612e4e3060ed613b472127309730ad614e4c672060407ea02d1ededededed7205938cb2db63087206730b0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a93e4dc640ce4c6a705640283013c0e0e860272097a9a720c720de4e3050ee4c672030564939f720e7bb4720f730cb1720fa0ee72109f72047bcbb3b3721072117207eded91720d730d90720d99720a720c939f720e7bb47212730eb17212a0ee72139f72147bcbb3b372137211db07027214cd720895937202730fd1eded720593e4c672030564e4c6a705649299c17203c1a77310d17311".to_string())
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

        // For now, we'll verify that we can parse the address and get the ErgoTree
        // The actual byte serialization with ByteArrayConstant wrapper would require
        // additional serialization logic that matches the Scala implementation
        assert!(!ergo_tree_hex.is_empty(), "ErgoTree bytes should not be empty");
        // Updated expected ErgoTree bytes for current P2S address
        assert_eq!(ergo_tree_hex, "1012041404140400040005000400044204e02105000400044204000442050004420402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d80fd606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be3070ed60c95e6720b7ce4dc640ae4c6a70564027209e4720b7304d60d99c1a7c17203d60edb6a01ddd60fe4e3020ed610b4720f73057306d611959199a38cc77206017307b3b372097a720a7a7308b372097a720ad612e4e3060ed613b472127309730ad614e4c672060407ea02d1ededededed7205938cb2db63087206730b0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a93e4dc640ce4c6a705640283013c0e0e860272097a9a720c720de4e3050ee4c672030564939f720e7bb4720f730cb1720fa0ee72109f72047bcbb3b3721072117207eded91720d730d90720d99720a720c939f720e7bb47212730eb17212a0ee72139f72147bcbb3b372137211db07027214cd720895937202730fd1eded720593e4c672030564e4c6a705649299c17203c1a77310d17311",
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
        // Updated for current P2S address: starts with 0edc03 (ByteArrayConstant prefix with length)
        let expected_bytes_hex = "0edc031012041404140400040005000400044204e02105000400044204000442050004420402058084af5f0100d805d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060e959372027302d80fd606b2db6501fe730300d607db07027204d608e4e30107d609cbb37207db07027208d60ae4e30305d60be3070ed60c95e6720b7ce4dc640ae4c6a70564027209e4720b7304d60d99c1a7c17203d60edb6a01ddd60fe4e3020ed610b4720f73057306d611959199a38cc77206017307b3b372097a720a7a7308b372097a720ad612e4e3060ed613b472127309730ad614e4c672060407ea02d1ededededed7205938cb2db63087206730b0001e4c6a7060e937ce4dc640ae4c672060564027209e4e3080e720a93e4dc640ce4c6a705640283013c0e0e860272097a9a720c720de4e3050ee4c672030564939f720e7bb4720f730cb1720fa0ee72109f72047bcbb3b3721072117207eded91720d730d90720d99720a720c939f720e7bb47212730eb17212a0ee72139f72147bcbb3b372137211db07027214cd720895937202730fd1eded720593e4c672030564e4c6a705649299c17203c1a77310d17311";

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
