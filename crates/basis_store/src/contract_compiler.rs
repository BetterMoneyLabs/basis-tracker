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

/// Get the Basis custom-token reserve contract P2S address
pub fn get_basis_token_reserve_contract_p2s() -> Result<String, CompilerError> {
    Ok("96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P".to_string())
}

/// Get the Basis custom-token reserve contract ErgoTree hex
pub fn get_basis_token_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    Ok("1b90062804140414040204040400040004020500040004000410042005000402040004420400040004000410050004420500040004420442010104e02105000402050004020502040404020500040605000480a3050100d80bd6017ee4e3000204d6029d72017300d603b2a59e7201730100d604db63087203d605db6308a7d606b27205730200d607eded93b172047303938cb27204730400018cb2720573050001938cb27204730600018c720601d608e4c6a70407d609eded93c27203c2a793e4c672030407720893e4c67203060ee4c6a7060ed60a8c720602d60be5c6a707057307959372027308d813d60cb2db6501fe730900d60ddb07027208d60ee4e30107d60fcbb3720ddb0702720ed610e4e30305d611e4e30405d612e3070ed613e67212d6147a7211d6159572137cb4e4dc640ae4c6a7056402720fe47212730a730b730cd61699720a8cb27204730d0002d617db6a01ddd618e4e3020ed619b47218730e730fd61ab3b3720f7a72107214d61be4e3060ed61cb1721bd61d91721c7310d61ee4c6720c0407ea02d1edededededededed72097207938cb2db6308720c73110001e4c6a7060e937ce4dc640ae4c6720c056402720fe4e3080e72109172119572137cb4e4dc640ae4c6a7056402720fe4721273127313731493e4dc6410e4c6a705640283013c0e0e8602720fb372147a9a72157216e4e3050ee4c672030564939f72177bb472187315b17218a0ee72199f72087bcbb3b37219721a720deded9172167316907216997210721595721d95721dd801d61fb4721b73177318939f72177bb4721b7319721ca0ee721f9f721e7bcbb3b3721f721adb0702721e731a9199a38cc7720c01731b93e5c672030705731c720bcd720e95937202731dd1edededed7209720793e4c672030564e4c6a7056493e5c672030705731e720b92998cb27204731f0002720a7320959372027321ea02d1edededededed72097207928cb2720473220002720a92c17203c1a793e4c672030564e4c6a7056492e4c6720307057ea30593720b7323cd7208959372027324ea02d1ed91720b7325927ea3059a720b7e732605cd7208d17327".to_string())
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
