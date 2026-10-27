//! Cross-validation test vectors for Schnorr signatures
//!
//! These test vectors were generated using the Scala-compatible signing algorithm
//! with bitLength <= 255 constraint, matching the reference implementation in
//! `scala/src/main/scala/basis/offchain/SigUtils.scala`.

// No imports needed

/// A single Schnorr signature test vector
#[derive(Debug, Clone)]
pub struct SchnorrTestVector {
    pub id: &'static str,
    pub description: &'static str,
    pub issuer_pubkey_hex: &'static str,
    pub recipient_pubkey_hex: &'static str,
    pub amount: u64,
    pub timestamp: u64,
    pub message_hex: &'static str,
    pub signature_hex: &'static str,
    pub should_verify: bool,
}

/// Cross-validation test vectors for Schnorr signature verification
///
/// Regenerated for the C3 fix (issue #15): every message is now 80 bytes,
/// `key(32) || totalDebt(8 BE) || timestamp(8 BE) || trackerNftId(32)`, with the
/// tracker NFT id fixed to `0x42` repeated 32 times for test purposes. Keypairs are
/// derived from the committed `scala/secrets/participants.csv` fixture.
///
/// TV003 and TV009 carry Fiat-Shamir challenges with the top bit set: `z` satisfies
/// `bitLength <= 255`, but the contract reads the challenge with signed
/// `byteArrayToBigInt`, so `pubKey.exp(e)` throws on-chain. They are kept with
/// `should_verify = false` as regression coverage for that gap (see TV003's SECURITY
/// note and specs/pr14_triage.md S5).
pub const SCHNORR_TEST_VECTORS: &[SchnorrTestVector] = &[
    SchnorrTestVector {
        id: "TV001",
        description: "Standard valid signature",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "027742dac59ea73f7f8710648f17b9fb8ccf570460a727c02b28a6e2f310ebf4711397afdac9e6cc799beedc3309ef2b4989598768f65ea78b95151c844e86408f",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV002",
        description: "All-zero signature should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        should_verify: false,
    },
    SchnorrTestVector {
        // SECURITY: the Fiat-Shamir challenge of this (mathematically valid) signature starts
        // with a top-bit-set byte. The contract reads the challenge with signed
        // `byteArrayToBigInt`, so it becomes negative and `pubKey.exp(e)` throws — a note
        // carrying it could be pinned by the tracker yet never redeemed on-chain. Both the
        // Rust verifier and the Scala signer reject/produce only top-bit-clear challenges;
        // this vector is regression coverage for that gap.
        id: "TV003",
        description: "SIGNATURE_WITH_INCOMPATIBLE_CHALLENGE",
        issuer_pubkey_hex: "02b651a75c3cec621fa493dfa45fbb42ff77943acdceeb9543163d76e3d81cb645",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 500000000u64,
        timestamp: 1743379201000u64,
        message_hex: "52eb0beb0bd0e86206e0a817785aadabbef08730fd5654ea65ebcbe1ef1b3b9e000000001dcd650000000195e97f7be84242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "02d052ebc2a622f61681dc9ad52a8cd062acc4c6acf9ba6cfdd507ae886b8054701319d7378338904e89be9604c16825a06fd831afa598248e3f3902be9d270c39",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV004",
        description: "Wrong signer signature should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "0240d01f20717b244800f147d2c9b559cd836c19aea344071c1639be2a22870e772bdbbaf89bb715fcc8c4ec7d8df464fb196c83a9616ef0ccd392f5ae8022d072",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV005",
        description: "Corrupted signature should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "036eb16f1611ae82edfd2ca30251e6fbbb3eae03173eb16e270e53ce7f755342a16d86d89db441bb8905f86110f3e615ffaf3ceb9d7b1992c0d14d0e3cbf7f3931",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV006",
        description: "Wrong amount should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "028c3d344aefd0caa6a16ffdab0bd154b457585cd998e5ac80b86286a796f33f1e53488afbc8e3160caab535bac018b81bd62acea9b64c2a426d08727d42193720",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV007",
        description: "Wrong timestamp should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "03baecb66840002c131c54134b0abde112a0405f3d6d1d384b5815857ee68b9e624ba6d7c176c8631633c7e609af34f9e389f99c6e3fd64e16aa743956d66e1e11",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV008",
        description: "Wrong recipient should fail",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "0282fcc89cb99e3491bb43d1797ff2567fde0620ea38d78f937ae122ae0acf2f897169bcb397037aeb94eda8ee6e5642781f8fe1d85f33bc6a8ed5a440760fd029",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV009",
        description: "SIGNATURE_WITH_INCOMPATIBLE_CHALLENGE (max u64 message)",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 18446744073709551615u64,
        timestamp: 18446744073709551615u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851fffffffffffffffffffffffffffffffff4242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "03917db7e61b54d50409c0447ebd27caa124d2702aa0ae5651381cd662162307c8055e54abdd0ba40be01c695edf95b9026c94f286a3353f63d1e2348e5d455c2f",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV010",
        description: "Zero amount and timestamp",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 0u64,
        timestamp: 0u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000000000000000000000000000004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "0235f9989903d1e2ee8c274ba5494119b6deef7cd0d0e15dc07190011b524c891d5deec9c943adce1891dc5977cda361da0a81e81bbf489b3a7f6b8d2c15116ad4",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV011",
        description: "Emergency redemption valid reserve signature",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 500000000u64,
        timestamp: 1743379202000u64,
        message_hex: "76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000001dcd650000000195e97f7fd04242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "02001d67f568ace3b32c68bd9cfdd3795d5df0f5057b4d837935578697675c8e334668c7b923ae13df8dd9725ac66c63a4ca6f05cc2e7d1c3d23abdcb750dea923",
        should_verify: true,
    },
    // Token-denominated note: the message key is the namespaced debt key
    // blake2b256(issuer ‖ recipient ‖ tokenId) with token id
    // a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669 (USE).
    // See specs/ergo/multi_collateral_reserves.md.
    SchnorrTestVector {
        id: "TV012",
        description: "Token-denominated note (namespaced debtKey)",
        issuer_pubkey_hex: "02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063",
        recipient_pubkey_hex: "023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "967d7eb07055cb7ab02e6a8956d3e3213715a945050d236d983972bad5c60565000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242",
        signature_hex: "03f0f0df97d7c1cc78cd9f3fa98c944bcc32c04c4827d3ac0488d9b19cc30549783023efc493827d880225ebabda4553cbdd7c1e872891e6ddfc9f7713efe76971",
        should_verify: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schnorr;

    #[test]
    fn test_cross_validation_vectors() {
        for vector in SCHNORR_TEST_VECTORS {
            let issuer_pubkey = hex::decode(vector.issuer_pubkey_hex)
                .expect("Invalid issuer pubkey hex")
                .try_into()
                .expect("Issuer pubkey must be 33 bytes");
            let message = hex::decode(vector.message_hex).expect("Invalid message hex");
            let signature = hex::decode(vector.signature_hex)
                .expect("Invalid signature hex")
                .try_into()
                .expect("Signature must be 65 bytes");

            let result = schnorr::schnorr_verify(&signature, &message, &issuer_pubkey);
            let verified = result.is_ok();

            assert_eq!(
                verified, vector.should_verify,
                "Test vector {} failed: {} (expected {}, got {})",
                vector.id, vector.description, vector.should_verify, verified
            );
        }
    }

    /// TV012 cross-validation: the Rust signing message for a token denomination must
    /// reproduce the Scala-generated message byte-for-byte (namespaced debt key).
    #[test]
    fn test_token_denomination_message_matches_scala_vector() {
        let vector = SCHNORR_TEST_VECTORS
            .iter()
            .find(|v| v.id == "TV012")
            .expect("TV012 present");
        let issuer: crate::PubKey = hex::decode(vector.issuer_pubkey_hex)
            .unwrap()
            .try_into()
            .unwrap();
        let recipient: crate::PubKey = hex::decode(vector.recipient_pubkey_hex)
            .unwrap()
            .try_into()
            .unwrap();
        let token_id: [u8; 32] =
            hex::decode("a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669")
                .unwrap()
                .try_into()
                .unwrap();
        let tracker_nft_id = [0x42u8; 32];

        let message = crate::schnorr::signing_message(
            &issuer,
            &recipient,
            &crate::DenomKey::Token(token_id),
            vector.amount,
            vector.timestamp,
            &tracker_nft_id,
        );
        assert_eq!(
            hex::encode(&message),
            vector.message_hex,
            "token-denominated signing message must match the Scala vector"
        );

        // The ERG message for the same pair is different (different debt key).
        let erg_message = crate::schnorr::signing_message(
            &issuer,
            &recipient,
            &crate::DenomKey::Erg,
            vector.amount,
            vector.timestamp,
            &tracker_nft_id,
        );
        assert_ne!(message, erg_message);
    }
}
