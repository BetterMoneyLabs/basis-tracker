//! BasisTokenSpec replication tests in Rust
//!
//! These tests replicate the key test scenarios from the Scala `BasisTokenSpec`,
//! adapted for the Rust off-chain implementation. Since we cannot execute
//! on-chain ErgoScript in Rust, these tests focus on:
//!
//! 1. Loading and parsing the compiled `basis-token.es` P2S address.
//! 2. Verifying the ErgoTree hex and ByteArrayConstant serialization.
//! 3. Token-specific calculations: redemption amount, top-up delta, preservation
//!    of token IDs and the reserve NFT.
//! 4. Schnorr signature/message compatibility (the token reserve uses the same
//!    48-byte message format as the ERG-backed reserve).
//! 5. Debt transfer / triangular trade scenarios denominated in token units.
//!
//! See: `scala/tests/BasisTokenSpec.scala` for the original on-chain tests.

#[cfg(test)]
mod tests {
    use crate::contract_compiler::{
        get_basis_token_reserve_contract_p2s, get_basis_token_reserve_ergo_tree_hex,
    };
    use crate::schnorr::{generate_keypair, schnorr_sign, schnorr_verify};
    use crate::{IouNote, NoteKey, PubKey, TrackerStateManager};
    use basis_core::types::signing_message as core_signing_message;
    use blake2::{Blake2b, Digest};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use generic_array::typenum::U32;
    use secp256k1::{Secp256k1, SecretKey};

    // ========== Test Constants ==========

    const TEST_TIMESTAMP: u64 = 1_000_000_000;

    // ========== Helpers ==========

    fn blake2b256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn random_keypair() -> ([u8; 32], PubKey) {
        generate_keypair()
    }

    type TokenId = Vec<u8>;

    #[derive(Debug, Clone)]
    struct TokenBundle {
        tokens: Vec<(TokenId, u64)>,
    }

    impl TokenBundle {
        fn new(
            nft_id: TokenId,
            reserve_token_id: TokenId,
            nft_amount: u64,
            reserve_amount: u64,
        ) -> Self {
            Self {
                tokens: vec![(nft_id, nft_amount), (reserve_token_id, reserve_amount)],
            }
        }

        fn reserve_amount(&self) -> u64 {
            self.tokens[1].1
        }
    }

    /// Mirror of the contract's `tokenIdsPreserved` check:
    /// output must contain exactly two tokens, with token #0 and #1 IDs unchanged.
    fn token_ids_preserved(input: &TokenBundle, output: &TokenBundle) -> bool {
        output.tokens.len() == 2
            && output.tokens[0].0 == input.tokens[0].0
            && output.tokens[1].0 == input.tokens[1].0
    }

    /// Redeemed amount = input token #1 amount - output token #1 amount.
    fn redeemed_amount(input: &TokenBundle, output: &TokenBundle) -> Option<u64> {
        if !token_ids_preserved(input, output) {
            return None;
        }
        let in_amt = input.tokens[1].1;
        let out_amt = output.tokens[1].1;
        (out_amt <= in_amt).then(|| in_amt - out_amt)
    }

    /// Top-up delta = output token #1 amount - input token #1 amount.
    fn top_up_delta(input: &TokenBundle, output: &TokenBundle) -> Option<u64> {
        if !token_ids_preserved(input, output) {
            return None;
        }
        let in_amt = input.tokens[1].1;
        let out_amt = output.tokens[1].1;
        (out_amt > in_amt).then(|| out_amt - in_amt)
    }

    // ========== Contract Compilation / Serialization Tests ==========

    #[test]
    fn basis_token_p2s_is_not_empty() {
        let p2s = get_basis_token_reserve_contract_p2s().unwrap();
        assert!(!p2s.is_empty());
        assert!(p2s.len() > 50);
    }

    #[test]
    fn basis_token_p2s_parses_as_mainnet_p2s() {
        let p2s = get_basis_token_reserve_contract_p2s().unwrap();
        let encoder = ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::chain::address::NetworkPrefix::Mainnet,
        );
        let address = encoder
            .parse_address_from_str(&p2s)
            .expect("basis-token P2S should parse");
        assert!(
            address.script().is_ok(),
            "compiled basis-token address should yield a valid script"
        );
    }

    #[test]
    fn basis_token_ergo_tree_hex_matches_p2s() {
        let p2s = get_basis_token_reserve_contract_p2s().unwrap();
        let expected_tree = get_basis_token_reserve_ergo_tree_hex().unwrap();

        let encoder = ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::chain::address::NetworkPrefix::Mainnet,
        );
        let address = encoder.parse_address_from_str(&p2s).unwrap();
        let ergo_tree = address.script().expect("should extract script");
        let parsed_hex = hex::encode(ergo_tree.sigma_serialize_bytes().unwrap());

        assert_eq!(
            parsed_hex, expected_tree,
            "parsed ErgoTree hex must match the hard-coded basis-token ErgoTree"
        );
    }

    #[test]
    fn basis_token_byte_array_constant_serialization_is_stable() {
        let p2s = get_basis_token_reserve_contract_p2s().unwrap();

        let encoder = ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::chain::address::NetworkPrefix::Mainnet,
        );
        let address = encoder.parse_address_from_str(&p2s).unwrap();
        let ergo_tree = address.script().unwrap();
        let tree_bytes = ergo_tree.sigma_serialize_bytes().unwrap();

        let constant = ergo_lib::ergotree_ir::mir::constant::Constant::from(tree_bytes.clone());
        let serialized1 = hex::encode(constant.sigma_serialize_bytes().unwrap());
        let serialized2 = hex::encode(constant.sigma_serialize_bytes().unwrap());

        assert_eq!(
            serialized1, serialized2,
            "ByteArrayConstant serialization must be deterministic"
        );

        // The serialized form is a ByteArrayConstant: 0x0e prefix + VLQ length + raw bytes.
        assert!(
            serialized1.starts_with('0'),
            "ByteArrayConstant serialization should start with a type prefix"
        );
        assert!(
            serialized1.len() > tree_bytes.len() * 2,
            "serialization must include prefix bytes"
        );
    }

    // ========== Token Amount Calculation Tests ==========

    #[test]
    fn token_redemption_amount_calculation() {
        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 1000);
        let output = TokenBundle::new(nft_id, reserve_id, 1, 750);

        assert_eq!(redeemed_amount(&input, &output), Some(250));
    }

    #[test]
    fn token_redemption_cannot_exceed_debt_delta() {
        // The contract checks `redeemed <= totalDebt - alreadyRedeemed`.
        let total_debt = 500u64;
        let already_redeemed = 100u64;
        let debt_delta = total_debt - already_redeemed;

        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 1000);

        // Valid: redeem exactly the remaining debt.
        let valid_output =
            TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 1000 - debt_delta);
        let redeemed = redeemed_amount(&input, &valid_output).unwrap();
        assert!(redeemed > 0 && redeemed <= debt_delta);

        // Invalid: redeem one token more than remaining debt.
        let invalid_output = TokenBundle::new(nft_id, reserve_id, 1, 1000 - debt_delta - 1);
        let over_redeem = redeemed_amount(&input, &invalid_output).unwrap();
        assert!(
            over_redeem > debt_delta,
            "over-redeem must exceed debt delta"
        );
    }

    #[test]
    fn token_redemption_preserves_nft() {
        let nft_id = vec![0xaau8; 32];
        let reserve_id = vec![0xbbu8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 500);
        let output = TokenBundle::new(nft_id.clone(), reserve_id, 1, 400);

        assert!(token_ids_preserved(&input, &output));
        assert_eq!(output.tokens[0].1, 1, "NFT amount must stay 1");
        assert_eq!(output.tokens[0].0, nft_id, "NFT token ID must be preserved");
    }

    #[test]
    fn token_top_up_minimum_one_unit() {
        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 100);

        // Exactly one unit is the minimum accepted by the contract.
        let output = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 101);
        assert_eq!(top_up_delta(&input, &output), Some(1));

        // Zero delta is not a valid top-up.
        let no_change = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 100);
        assert_eq!(top_up_delta(&input, &no_change), None);

        // Decreasing is not a top-up.
        let decrease = TokenBundle::new(nft_id, reserve_id, 1, 99);
        assert_eq!(top_up_delta(&input, &decrease), None);
    }

    #[test]
    fn token_top_up_preserves_token_ids() {
        let nft_id = vec![0x11u8; 32];
        let reserve_id = vec![0x22u8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 10);
        let output = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 20);

        assert!(token_ids_preserved(&input, &output));
    }

    #[test]
    fn token_ids_must_be_preserved_exactly() {
        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let wrong_id = vec![0xffu8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 100);

        // Swapped token IDs.
        let swapped = TokenBundle::new(reserve_id.clone(), nft_id.clone(), 1, 100);
        assert!(!token_ids_preserved(&input, &swapped));

        // Wrong reserve token ID.
        let wrong_reserve = TokenBundle::new(nft_id.clone(), wrong_id.clone(), 1, 100);
        assert!(!token_ids_preserved(&input, &wrong_reserve));

        // Wrong NFT ID.
        let wrong_nft = TokenBundle::new(wrong_id, reserve_id.clone(), 1, 100);
        assert!(!token_ids_preserved(&input, &wrong_nft));

        // More than two tokens.
        let extra = TokenBundle {
            tokens: vec![
                (nft_id.clone(), 1),
                (reserve_id.clone(), 100),
                (vec![0x33u8; 32], 1),
            ],
        };
        assert!(!token_ids_preserved(&input, &extra));
    }

    #[test]
    fn initiate_refund_preserves_tokens_and_erg() {
        // Action #2 requires the output reserve token amount and ERG value to be
        // >= the input amounts (no value may leave the reserve at initiation).
        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let input = TokenBundle::new(nft_id.clone(), reserve_id.clone(), 1, 1000);

        let output = TokenBundle::new(nft_id, reserve_id, 1, 1000);
        assert!(token_ids_preserved(&input, &output));
        assert!(output.reserve_amount() >= input.reserve_amount());
    }

    #[test]
    fn complete_refund_can_take_all_tokens() {
        // Action #3 has no preservation constraints: the owner may take every token.
        let nft_id = vec![0x01u8; 32];
        let reserve_id = vec![0x02u8; 32];
        let input = TokenBundle::new(nft_id, reserve_id, 1, 1000);

        // After completion the reserve box is destroyed, so any output token bundle
        // (including an empty one) is acceptable from the contract's perspective.
        let output = TokenBundle { tokens: vec![] };
        assert!(!token_ids_preserved(&input, &output));
    }

    // ========== Signature / Message Compatibility Tests ==========

    #[test]
    fn token_reserve_uses_same_message_format() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let total_debt: u64 = 1_000; // token units
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);
        assert_eq!(message.len(), 48);

        let sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        assert!(schnorr_verify(&sig, &message, &owner_pk).is_ok());
    }

    #[test]
    fn token_reserve_signature_verification() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt = 250u64;
        let timestamp = TEST_TIMESTAMP;
        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        let owner_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        let tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();

        assert!(schnorr_verify(&owner_sig, &message, &owner_pk).is_ok());
        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_ok());

        // Signatures from the two signers must differ.
        assert_ne!(owner_sig, tracker_sig);
    }

    #[test]
    fn token_reserve_emergency_redemption_same_message_format() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let total_debt = 100u64;
        let timestamp = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);
        let owner_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();

        assert!(schnorr_verify(&owner_sig, &message, &owner_pk).is_ok());
    }

    #[test]
    fn token_reserve_invalid_signature_rejected() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (_tracker_secret, tracker_pk) = random_keypair();

        let total_debt = 100u64;
        let timestamp = TEST_TIMESTAMP;
        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        // Owner signs correctly.
        let owner_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        assert!(schnorr_verify(&owner_sig, &message, &owner_pk).is_ok());

        // Tracker signed with owner secret must not verify against tracker pubkey.
        let wrong_tracker_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        assert!(schnorr_verify(&wrong_tracker_sig, &message, &tracker_pk).is_err());

        // Corrupted owner signature must fail.
        let mut corrupted = owner_sig;
        corrupted[0] ^= 0x01;
        assert!(schnorr_verify(&corrupted, &message, &owner_pk).is_err());
    }

    // ========== Debt Transfer / Triangular Trade Tests ==========

    #[test]
    fn token_debt_transfer_triangular_trade() {
        let secp = Secp256k1::new();

        let alice_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let alice_pk = secp256k1::PublicKey::from_secret_key(&secp, &alice_secret).serialize();

        let bob_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let bob_pk = secp256k1::PublicKey::from_secret_key(&secp, &bob_secret).serialize();

        let carol_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let carol_pk = secp256k1::PublicKey::from_secret_key(&secp, &carol_secret).serialize();

        let initial_debt = 100u64;
        let transferred = 40u64;
        let remaining = 60u64;

        let msg_bob = core_signing_message(&alice_pk, &bob_pk, remaining, TEST_TIMESTAMP);
        let msg_carol = core_signing_message(&alice_pk, &carol_pk, transferred, TEST_TIMESTAMP + 1);

        let sig_bob = schnorr_sign(&msg_bob, &alice_secret.secret_bytes(), &alice_pk).unwrap();
        let sig_carol = schnorr_sign(&msg_carol, &alice_secret.secret_bytes(), &alice_pk).unwrap();

        assert!(schnorr_verify(&sig_bob, &msg_bob, &alice_pk).is_ok());
        assert!(schnorr_verify(&sig_carol, &msg_carol, &alice_pk).is_ok());

        assert_eq!(remaining + transferred, initial_debt);
        assert_ne!(msg_bob, msg_carol);
    }

    #[test]
    fn token_debt_transfer_fails_without_debtor_consent() {
        let secp = Secp256k1::new();

        let alice_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let alice_pk = secp256k1::PublicKey::from_secret_key(&secp, &alice_secret).serialize();

        let bob_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let bob_pk = secp256k1::PublicKey::from_secret_key(&secp, &bob_secret).serialize();

        let carol_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let carol_pk = secp256k1::PublicKey::from_secret_key(&secp, &carol_secret).serialize();

        let transfer_amount = 40u64;
        let message = core_signing_message(&alice_pk, &carol_pk, transfer_amount, TEST_TIMESTAMP);

        // Bob forges a signature with his own secret.
        let forged = schnorr_sign(&message, &bob_secret.secret_bytes(), &bob_pk).unwrap();
        assert!(schnorr_verify(&forged, &message, &alice_pk).is_err());
    }

    // ========== Tracker State Manager Tests ==========

    #[test]
    fn token_reserve_reserve_tree_value_format() {
        // The token reserve stores the same 16-byte value in R5 as the ERG reserve:
        // timestamp (8 bytes BE) || cumulative redeemed amount (8 bytes BE).
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        let (issuer_secret, issuer_pk) = random_keypair();
        let (_, recipient_pk) = random_keypair();

        let note =
            IouNote::create_and_sign(recipient_pk, 1_000, TEST_TIMESTAMP, &issuer_secret).unwrap();
        tracker.add_note(&issuer_pk, &note).unwrap();

        let proof = tracker
            .generate_reserve_lookup_proof(&issuer_pk, &recipient_pk)
            .unwrap();
        assert_eq!(proof.value.len(), 16);
        assert_eq!(proof.value, vec![0u8; 16]);
    }

    #[test]
    fn token_reserve_note_key_is_hash_of_pubkeys() {
        let issuer = [0x02u8; 33];
        let recipient = [0x03u8; 33];

        let key = NoteKey::from_keys(&issuer, &recipient);
        let expected = {
            let mut input = [0u8; 66];
            input[..33].copy_from_slice(&issuer);
            input[33..].copy_from_slice(&recipient);
            blake2b256(&input)
        };

        assert_eq!(key.key_hash, expected);
    }
}
