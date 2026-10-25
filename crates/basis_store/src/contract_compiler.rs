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
    Ok("2XoR96YVZfWybxeo7qdMkA1BRhxFivm8gTuLLuKREiW6833XWK4mA43DrJMDCvLqB9AwyduGgHBnGmmg2jrkUZAunRDu7t8718EkwiJTvp9rtEWEXMMJeMStjQB5BeuxsUc9ujW3aCMvxdS8VeYyZ14B8sSGu7CwJFmeaZDHiftsT3tUBqBfgau4wZTvhEtYEsFsqvpwsjcPAe4aGhFzJNSPhKqdSBrFKee3Dg4qFmGhb8Wu6DsdmhTrW6jnucnf17PDi2rQ9pgJvKrfs2KPbeZf9gV89vEga4kZk4u3bbSFTVk3s9EhuoUmfyhvC3ZaNv9DSPtzCcjvnL7x6smyLigsgb4QSEDnzX4HjJg5CfA6mh4Jn9hoL1412EVnY2xb8cYz76VEgU8J12n4DHrVsV9NzYAHxe6ogVNptUbAgykiru81pz3aZ6LfYySJt6ESVVGxm6i49ScGjawqsKCxjykCndh3MNQnRajZMgQMH2qFZhbd9KXCnLQkS6NAjtNfMVcx1KykdiSrLVTu3nphUZhT9MjhfxtZcojSZoQmGAy95zhAJpDMgUXBfuCSoUFUzPAuSMv2pBuEXiUR3wDQC7pDFYUDy68QCtTUTDtn9cYyNgc8EZWQSB76wsKhCNod1Q9RV85piLF5PJDMBXh4M8gjEzsq244xDdW6t2f4W4A2ZVDNw2gdSU3FHYxfB45c5cxkyR5qrXyoJT56c2STfFm3vK1ZfXzvckq48uAE1QRGpM9xALiXtdTx1iQaQjy2LkaURha6ewN5NDufyqntwBPk72NLadL8BDtLyReRtUWqbFZo94h4prjHVT25ABHbAqiJuTCHvmfTqi2V5gDwRW7oLk4gvBwswxJVvKmuKSG6sH5HAzQGoF6Kwcwdvq8Lbv5RbuAnm3N7hLnS1rBUWKXfaJCJjptMkeKxzMPhjfjnZTkMXwW3jSBxhpLeuWxrBrfjTFaVR93erQFVTvF9C1FsULgqUPjzPmcHn5Drp4cp4Y2pjv793k6KdykbugL6M6wt75hmKG5jsmrfsSL6AN13j3".to_string())
}

/// Get the Basis reserve contract ErgoTree hex (for reserve output in redemption transactions)
pub fn get_basis_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    // This is the raw ErgoTree hex for the P2S contract
    // Can be obtained by parsing the P2S address and serializing the script
    Ok("1bf90521041404140402050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d807d6017ee4e3000204d6029d72017300d603c2a7d604e4c6a70407d605b2a59e7201730100d606edededed93b1b5a4d9010663eded93c272067203e6c67206040793e4c6720604077204730293c27205720393db63087205db6308a793e4c672050407720493e4c67205060ee4c6a7060ed607e5c6a707057303959372027304d816d608b2db6501fe730500d609e4c6a7060ed60adb07027204d60be4e30107d60ccbb3720adb0702720bd60de4e30305d60ee4e30405d60fe3070ed610e6720fd6117a720ed6129572107cb4e4dc640ae4c6a7056402720ce4720f730673077308d61399c1a7c17205d6148602720cb372117a9a72127213d615e4e3050ed616db6a01ddd617e4e3020ed618b472177309730ad619b3b3b3720c7a720d72117209d61ae4e3060ed61bb1721ad61c91721b730bd61de4c672080407ea02d1ededededededed7206938cb2db63087208730c00017209937ce4dc640ae4c67208056402720ce4e3080e720d92720e9572107cb4e4dc640ae4c6a7056402720ce4720f730d730e730f93957210e4dc6410e4c6a705640283013c0e0e72147215e4dc640ce4c6a705640283013c0e0e72147215e4c672050564939f72167bb472177310b17217a0ee72189f72047bcbb3b372187219720aeded917213731190721399720d721295721c95721cd801d61eb4721a73127313939f72167bb4721a7314721ba0ee721e9f721d7bcbb3b3721e7219db0702721d73159199a38cc7720801731693e5c67205070573177207cd720b959372027318d1ededed720693e4c672050564e4c6a7056493e5c672050705731972079299c17205c1a7731a95937202731bea02d1edededed720692c17205c1a793e4c672050564e4c6a7056492e4c6720507057ea305937207731ccd720495937202731dea02d1ed917207731e927ea3059a72077e731f05cd7204d17320".to_string())
}

/// Get the Basis custom-token reserve contract P2S address
pub fn get_basis_token_reserve_contract_p2s() -> Result<String, CompilerError> {
    Ok("EUNip3sGUdzr5Leg8rRwRMyZGUis42syLiBYwsj5JbmrVNAh41LUzNFnFTEzpEvf3jMUzJdyfkJi6aVjbs6NLskmHiuCPYwcRFjdqfBwc9NXUnf4hv3heFRDnzXDWK92KAz8JpXSx9Rnd2b4HnkBq8HzbnfSRpRVQqRJZqQaVoceZqcT87sBTNii4Hu6zUPrGaQwsR3QFnNjoKsDrALMTELk8eUcxHhyWbJPPcSYWdf5nFTskqt5F2yZ5FBsKfkCsxskM31FKXRXt7RAWrJWA6QTUzrQT2kk4a8G1G7AYrXRFc2kXHGTH9bjXaNvM4JRu1CkDb2hqLzt5jouzT1UdZkCHqG9LwWa7ULSDfzvdgybQhBv8UC9LnHegQ7FEYyqHhL4ZZpsqr8cGbznE1QCWWbxcGLKWoDhCCt3j6Bv36AoaTPRcffevReEWMk7FTE6s2n391CLV6W8G2HS2zkStnmAKDmdardZJJ2tb3zQPrqM5Y2umufxDV9VCj6t18XBPSNDF8YtPXfwkPLUAyfeEE8JM4wCPNQ33vvH8irVphUagT92U5jLp9B8Z54ceJrLFodaj74kxECULy9ufmC6jGDNbeGj6AM6pxV6HBPgYpBNw8Qupc35se4YW7odPjErov8pZuVHnGusNu4huP1LFm2VL4aqMnLpriHTvx61DG7SCEUxZhEjWtnS5xxnp67fAzibCckBsKZeFsqEGro8pJnS1ZiPAwggz2dLorMoK285Rx8QZNLiRzfULwnWzSGgtoqKZCkKc7QJXiEzivAe4FHC32grsU2KvAb9sv79ardmpgpJTmbNLRAWiDwBXbTLGbF5AJkCcfPCvEoiaLCRERVYpJRZ4XZHo5A14eCFgwvUhDtPpZeJJXqNgGmsbaXZGPXKwdzaE8mA9uZ3ANcZ3KrdYL5HV7w7wrNNMWNzvBSfzM3uC5LiZmaxuWNph9YFLXHEbEfbhjXvGeAMZTGKntErw7NobRPQB6yCVeGMUk5HCDxJxkHAbn9oLxC2Nv3Wpp4cVMf6ixeTS5qL7he36pqkNPAcspPGHFJEajbs4fkaQd9wTgtbGCa7CXAWaP5s1S5pWCBSjnYkgxhjRYa6CwCnnV1u9ksA3Gb3PBr6Z8T2wicYqW925b3wxzNJKs5qotwCDzYthZjiGHGdtFA1vg32vKyGbeoPCXMgmn9uh1dcmnu2m7GVBEjQZ7LYHvuPZp34ykfTTFA".to_string())
}

/// Get the Basis custom-token reserve contract ErgoTree hex
pub fn get_basis_token_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    Ok("1bfb062a0414041404000402040404000400040204020500040004000410042005000402040004420400040004000410050004420500040004420442010104e02105000402050004020502040404020500040605000480a3050100d80ed6017ee4e3000204d6029d72017300d603b2a59e7201730100d604db63087203d605db6308a7d606b27205730200d607b27205730300d608ededed93b172047304938cb27204730500018c720601938cb27204730600028c720602938cb27204730700018c720701d609c2a7d60ae4c6a70407d60bededed93b1b5a4d9010b63eded93c2720b7209e6c6720b040793e4c6720b0407720a730893c27203720993e4c672030407720a93e4c67203060ee4c6a7060ed60c92c17203c1a7d60d8c720702d60ee5c6a70705730995937202730ad816d60fb2db6501fe730b00d610e4c6a7060ed611db0702720ad612e4e30107d613cbb37211db07027212d614e4e30305d615e4e30405d616e3070ed617e67216d6187a7215d6199572177cb4e4dc640ae4c6a70564027213e47216730c730d730ed61a99720d8cb27204730f0002d61b86027213b372187a9a7219721ad61ce4e3050ed61ddb6a01ddd61ee4e3020ed61fb4721e73107311d620b3b3b372137a721472187210d621e4e3060ed622b17221d6239172227312d624e4c6720f0407ea02d1ededededededededed720b7208720c938cb2db6308720f731300017210937ce4dc640ae4c6720f0564027213e4e3080e72149272159572177cb4e4dc640ae4c6a70564027213e4721673147315731693957217e4dc6410e4c6a705640283013c0e0e721b721ce4dc640ce4c6a705640283013c0e0e721b721ce4c672030564939f721d7bb4721e7317b1721ea0ee721f9f720a7bcbb3b3721f72207211eded91721a731890721a9972147219957223957223d801d625b472217319731a939f721d7bb47221731b7222a0ee72259f72247bcbb3b372257220db07027224731c9199a38cc7720f01731d93e5c672030705731e720ecd721295937202731fd1ededededed720b7208720c93e4c672030564e4c6a7056493e5c6720307057320720e92998cb2720473210002720d7322959372027323ea02d1edededededed720b7208928cb2720473240002720d720c93e4c672030564e4c6a7056492e4c6720307057ea30593720e7325cd720a959372027326ea02d1ed91720e7327927ea3059a720e7e732805cd720ad17329".to_string())
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
        assert_eq!(ergo_tree_hex, "1bf90521041404140402050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d807d6017ee4e3000204d6029d72017300d603c2a7d604e4c6a70407d605b2a59e7201730100d606edededed93b1b5a4d9010663eded93c272067203e6c67206040793e4c6720604077204730293c27205720393db63087205db6308a793e4c672050407720493e4c67205060ee4c6a7060ed607e5c6a707057303959372027304d816d608b2db6501fe730500d609e4c6a7060ed60adb07027204d60be4e30107d60ccbb3720adb0702720bd60de4e30305d60ee4e30405d60fe3070ed610e6720fd6117a720ed6129572107cb4e4dc640ae4c6a7056402720ce4720f730673077308d61399c1a7c17205d6148602720cb372117a9a72127213d615e4e3050ed616db6a01ddd617e4e3020ed618b472177309730ad619b3b3b3720c7a720d72117209d61ae4e3060ed61bb1721ad61c91721b730bd61de4c672080407ea02d1ededededededed7206938cb2db63087208730c00017209937ce4dc640ae4c67208056402720ce4e3080e720d92720e9572107cb4e4dc640ae4c6a7056402720ce4720f730d730e730f93957210e4dc6410e4c6a705640283013c0e0e72147215e4dc640ce4c6a705640283013c0e0e72147215e4c672050564939f72167bb472177310b17217a0ee72189f72047bcbb3b372187219720aeded917213731190721399720d721295721c95721cd801d61eb4721a73127313939f72167bb4721a7314721ba0ee721e9f721d7bcbb3b3721e7219db0702721d73159199a38cc7720801731693e5c67205070573177207cd720b959372027318d1ededed720693e4c672050564e4c6a7056493e5c672050705731972079299c17205c1a7731a95937202731bea02d1edededed720692c17205c1a793e4c672050564e4c6a7056492e4c6720507057ea305937207731ccd720495937202731dea02d1ed917207731e927ea3059a72077e731f05cd7204d17320",
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
        let expected_bytes_hex = "0efc051bf90521041404140402050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d807d6017ee4e3000204d6029d72017300d603c2a7d604e4c6a70407d605b2a59e7201730100d606edededed93b1b5a4d9010663eded93c272067203e6c67206040793e4c6720604077204730293c27205720393db63087205db6308a793e4c672050407720493e4c67205060ee4c6a7060ed607e5c6a707057303959372027304d816d608b2db6501fe730500d609e4c6a7060ed60adb07027204d60be4e30107d60ccbb3720adb0702720bd60de4e30305d60ee4e30405d60fe3070ed610e6720fd6117a720ed6129572107cb4e4dc640ae4c6a7056402720ce4720f730673077308d61399c1a7c17205d6148602720cb372117a9a72127213d615e4e3050ed616db6a01ddd617e4e3020ed618b472177309730ad619b3b3b3720c7a720d72117209d61ae4e3060ed61bb1721ad61c91721b730bd61de4c672080407ea02d1ededededededed7206938cb2db63087208730c00017209937ce4dc640ae4c67208056402720ce4e3080e720d92720e9572107cb4e4dc640ae4c6a7056402720ce4720f730d730e730f93957210e4dc6410e4c6a705640283013c0e0e72147215e4dc640ce4c6a705640283013c0e0e72147215e4c672050564939f72167bb472177310b17217a0ee72189f72047bcbb3b372187219720aeded917213731190721399720d721295721c95721cd801d61eb4721a73127313939f72167bb4721a7314721ba0ee721e9f721d7bcbb3b3721e7219db0702721d73159199a38cc7720801731693e5c67205070573177207cd720b959372027318d1ededed720693e4c672050564e4c6a7056493e5c672050705731972079299c17205c1a7731a95937202731bea02d1edededed720692c17205c1a793e4c672050564e4c6a7056492e4c6720507057ea305937207731ccd720495937202731dea02d1ed917207731e927ea3059a72077e731f05cd7204d17320";

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
