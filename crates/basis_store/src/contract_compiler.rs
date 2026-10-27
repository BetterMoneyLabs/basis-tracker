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
    Ok("8LeDK9LdaWwuYnQYQNkBJEucHSWd7bMR7uFyPyfoM4BNCS7JZFZy2FWzPexceCabw9MKgosUWi3h3ZwYRbX8tx36JJ9h4fJKWX4ppE9fjY2N2MPSNs6hKNTswixCryZBz4hWazaV7tVvtbkweaYf6ty4cGYDHaYoRcZAvk3dxZKxntStqrF5oX9v5D7tr3D6aqDs2fswYYfKvYN7H5P1ddk3jJh4wN9wQVLy1X7a5Ap546ajYs763dVG5n4jCqkp41ihWfDu8XYd5nEipMPvCPYo3RfrKU2xJXKnBUWCwyueHDSwRYvF3c9244Edz9w1WieUYMyHiEz3xuee9gKECFtGP1rjETLF8HrbRWCTcub2QF2PnYYdiGwNEvWNrbnVT4fdCpSEBY7HBo3tr5dsqJuV59ncw8qSyzHdPyhHLK2Fanx3LXKqkwB2vEtnfdL1oktvEGLmkGqodPsGNBzhFDiJwJXpPYUbUsfUBnJwVkZbFmvVatnEJq9RcmAKpTbH5JN4hvrrMMoVAiVVK1ivkPrQZ7jFPSGftCTrMyxL3j1AdkCzUTwjKHJKfwbQiQt3zBRx82yu3nA9mCoduuXC2K57gCgznzitXBNxVsda3X7ifYxoA7ejsL2c2Q8NJ1QJSgcGuogjGTZBQF4tSMe1245tC3hhhrJxyydAGNWin6WefCZbzb5YJiSkGG8kjNvHhHXYZZ2va9LZGskvUuwgNNEXyjgmMu57F4HWdgdb51R1WfS8FQ42D6kF26Mhtg9U4RAmEi82peGnEJogzaL6A6YMbsACXNhuhDPtgotuLyQ73E3e6FHiebU9gz7FAZer1SGckeETx82AAZgypKVSzCqRtTMbzGqjeMfG57t5zF1r8dF7kBCGAam7EE2M2mFnVSddpLFcrhAjgwryp1Kp73v6SaE6MH9eU4hQJiFVsQkTM9dwCWSB7c2qL6UfviJMiNt37XZe56XVJrKkceKzqoACZmCwv3o4C6jycUcqMN55VFzrnHg77ah927yNZ3DP4X5uaRy44imbc6pm2Gr3rVaX1Tje7hKW6EraH1CAceNP8oH3JEZH3DZbj4wuSoo6kbPXQbfwd5vPX4VFUjESGSE2k7nn4xxpYzvgwrVoAqAmsSJycx8YPXHPsT2NiPG1DdJzjrBS1V7suQVVU9Ramxwf8avhWGyxRZBfExWbu3gvvHH2D38shF8ofEprueYrhxcZmtHoDSTFeTxV6JZj8GGFcePZVxuAu".to_string())
}

/// Get the Basis custom-token reserve contract ErgoTree hex
pub fn get_basis_token_reserve_ergo_tree_hex() -> Result<String, CompilerError> {
    Ok("1b8b072a0414041404000402040404000400040204020500040004000410042005000402040004420400040004000410050004420500040004420442010104e02105000402050004020502040404020500040605000480a3050100d80fd6017ee4e3000204d6029d72017300d603b2a59e7201730100d604db63087203d605db6308a7d606b27205730200d607b27205730300d6088c720701d609ededed93b172047304938cb27204730500018c720601938cb27204730600028c720602938cb27204730700017208d60ac2a7d60be4c6a70407d60cededed93b1b5a4d9010c63eded93c2720c720ae6c6720c040793e4c6720c0407720b730893c27203720a93e4c672030407720b93e4c67203060ee4c6a7060ed60d92c17203c1a7d60e8c720702d60fe5c6a70705730995937202730ad818d610b2db6501fe730b00d611e4c6a7060ed612db0702720bd613e4e30107d614b37212db07027213d615cbb372147208d616e4e30305d617e4e30405d618e3070ed619e67218d61acb7214d61b7a7217d61c9572197cb4e4dc640ae4c6a7056402721ae47218730c730d730ed61d99720e8cb27204730f0002d61e8602721ab3721b7a9a721c721dd61fe4e3050ed620db6a01ddd621e4e3020ed622b4722173107311d623b3b3b372157a7216721b7211d624e4e3060ed625b17224d6269172257312d627e4c672100407ea02d1ededededededededed720c7209720d938cb2db63087210731300017211937ce4dc640ae4c672100564027215e4e3080e72169272179572197cb4e4dc640ae4c6a7056402721ae4721873147315731693957219e4dc6410e4c6a705640283013c0e0e721e721fe4dc640ce4c6a705640283013c0e0e721e721fe4c672030564939f72207bb472217317b17221a0ee72229f720b7bcbb3b3722272237212eded91721d731890721d997216721c957226957226d801d628b472247319731a939f72207bb47224731b7225a0ee72289f72277bcbb3b372287223db07027227731c9199a38cc7721001731d93e5c672030705731e720fcd721395937202731fd1ededededed720c7209720d93e4c672030564e4c6a7056493e5c6720307057320720f92998cb2720473210002720e7322959372027323ea02d1edededededed720c7209928cb2720473240002720e720d93e4c672030564e4c6a7056492e4c6720307057ea30593720f7325cd720b959372027326ea02d1ed91720f7327927ea3059a720f7e732805cd720bd17329".to_string())
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
