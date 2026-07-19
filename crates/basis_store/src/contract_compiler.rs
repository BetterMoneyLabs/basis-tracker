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
    Ok("3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT".to_string())
}

/// Get the Basis reserve contract ErgoTree hex (for reserve output in redemption transactions)
pub fn get_basis_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    // This is the raw ErgoTree hex for the P2S contract
    // Can be obtained by parsing the P2S address and serializing the script
    Ok("102004140414050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d806d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060ed606e5c6a707057302959372027303d813d607b2db6501fe730400d608db07027204d609e4e30107d60acbb37208db07027209d60be4e30305d60ce4e30405d60de3070ed60ee6720dd60f7a720cd61095720e7cb4e4dc640ae4c6a7056402720ae4720d730573067307d61199c1a7c17203d612db6a01ddd613e4e3020ed614b4721373087309d615b3b3720a7a720b720fd616e4e3060ed617b17216d618917217730ad619e4c672070407ea02d1ededededededed7205938cb2db63087207730b0001e4c6a7060e937ce4dc640ae4c67207056402720ae4e3080e720b91720c95720e7cb4e4dc640ae4c6a7056402720ae4720d730c730d730e93e4dc640ce4c6a705640283013c0e0e8602720ab3720f7a9a72107211e4e3050ee4c672030564939f72127bb47213730fb17213a0ee72149f72047bcbb3b3721472157208eded917211731090721199720b7210957218957218d801d61ab4721673117312939f72127bb4721673137217a0ee721a9f72197bcbb3b3721a7215db0702721973149199a38cc7720701731593e5c67203070573167206cd7209959372027317d1ededed720593e4c672030564e4c6a7056493e5c672030705731872069299c17203c1a7731995937202731aea02d1edededed720592c17203c1a793e4c672030564e4c6a7056492e4c6720307057ea305937206731bcd720495937202731cea02d1ed917206731d927ea3059a72067e731e05cd7204d1731f".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::chain::address::AddressEncoder;
    use ergo_lib::ergotree_ir::chain::address::NetworkPrefix;
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
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes().unwrap();
        let ergo_tree_hex = hex::encode(&ergo_tree_bytes);

        // For now, we'll verify that we can parse the address and get the ErgoTree
        // The actual byte serialization with ByteArrayConstant wrapper would require
        // additional serialization logic that matches the Scala implementation
        assert!(
            !ergo_tree_hex.is_empty(),
            "ErgoTree bytes should not be empty"
        );
        // Updated expected ErgoTree bytes for current P2S address
        assert_eq!(ergo_tree_hex, "102004140414050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d806d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060ed606e5c6a707057302959372027303d813d607b2db6501fe730400d608db07027204d609e4e30107d60acbb37208db07027209d60be4e30305d60ce4e30405d60de3070ed60ee6720dd60f7a720cd61095720e7cb4e4dc640ae4c6a7056402720ae4720d730573067307d61199c1a7c17203d612db6a01ddd613e4e3020ed614b4721373087309d615b3b3720a7a720b720fd616e4e3060ed617b17216d618917217730ad619e4c672070407ea02d1ededededededed7205938cb2db63087207730b0001e4c6a7060e937ce4dc640ae4c67207056402720ae4e3080e720b91720c95720e7cb4e4dc640ae4c6a7056402720ae4720d730c730d730e93e4dc640ce4c6a705640283013c0e0e8602720ab3720f7a9a72107211e4e3050ee4c672030564939f72127bb47213730fb17213a0ee72149f72047bcbb3b3721472157208eded917211731090721199720b7210957218957218d801d61ab4721673117312939f72127bb4721673137217a0ee721a9f72197bcbb3b3721a7215db0702721973149199a38cc7720701731593e5c67203070573167206cd7209959372027317d1ededed720593e4c672030564e4c6a7056493e5c672030705731872069299c17203c1a7731995937202731aea02d1edededed720592c17203c1a793e4c672030564e4c6a7056492e4c6720307057ea305937206731bcd720495937202731cea02d1ed917206731d927ea3059a72067e731e05cd7204d1731f",
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
        let ergo_tree_bytes = ergo_tree.sigma_serialize_bytes().unwrap();

        // Create a ByteArrayConstant with the ErgoTree bytes
        // In Rust ergo-lib, this would be equivalent to Constant::from(ergo_tree_bytes)
        let byte_array_constant =
            ergo_lib::ergotree_ir::mir::constant::Constant::from(ergo_tree_bytes);

        // Serialize the ByteArrayConstant to bytes
        // This matches the Scala pattern: ValueSerializer.serialize(ByteArrayConstant(...))
        let serialized_bytes = byte_array_constant.sigma_serialize_bytes().unwrap();
        let serialized_hex = hex::encode(&serialized_bytes);

        // The expected ByteArrayConstant-wrapped bytes that the Ergo node expects for scan registration
        // Updated for current P2S address: starts with 0eaa04 (ByteArrayConstant prefix with length)
        let expected_bytes_hex = "0ead05102004140414050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d806d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060ed606e5c6a707057302959372027303d813d607b2db6501fe730400d608db07027204d609e4e30107d60acbb37208db07027209d60be4e30305d60ce4e30405d60de3070ed60ee6720dd60f7a720cd61095720e7cb4e4dc640ae4c6a7056402720ae4720d730573067307d61199c1a7c17203d612db6a01ddd613e4e3020ed614b4721373087309d615b3b3720a7a720b720fd616e4e3060ed617b17216d618917217730ad619e4c672070407ea02d1ededededededed7205938cb2db63087207730b0001e4c6a7060e937ce4dc640ae4c67207056402720ae4e3080e720b91720c95720e7cb4e4dc640ae4c6a7056402720ae4720d730c730d730e93e4dc640ce4c6a705640283013c0e0e8602720ab3720f7a9a72107211e4e3050ee4c672030564939f72127bb47213730fb17213a0ee72149f72047bcbb3b3721472157208eded917211731090721199720b7210957218957218d801d61ab4721673117312939f72127bb4721673137217a0ee721a9f72197bcbb3b3721a7215db0702721973149199a38cc7720701731593e5c67203070573167206cd7209959372027317d1ededed720593e4c672030564e4c6a7056493e5c672030705731872069299c17203c1a7731995937202731aea02d1edededed720592c17203c1a793e4c672030564e4c6a7056492e4c6720307057ea305937206731bcd720495937202731cea02d1ed917206731d927ea3059a72067e731e05cd7204d1731f";

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
