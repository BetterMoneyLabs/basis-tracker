//! Core types for Basis Tracker system

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;

/// Public key type (Secp256k1 compressed)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1 Schnorr) - 65 bytes (33 for 'a' component, 32 for 'z' component)
pub type Signature = [u8; 65];

/// Denomination of a debt record or reserve: native ERG or a specific token.
///
/// The denomination is part of the tracker debt-tree key derivation
/// (specs/ergo/multi_collateral_reserves.md):
/// - Erg:   key = blake2b256(owner ‖ receiver)            (matches contract/basis.es)
/// - Token: key = blake2b256(owner ‖ receiver ‖ tokenId)  (basis-token.es)
///
/// so one (issuer, creditor) pair can hold an independent debt per denomination in
/// the same tracker tree. The reserve redemption tree keys are denomination-free
/// (`blake2b256(owner ‖ receiver)`) in both contracts.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum DenomKey {
    /// Native ERG (empty debt-key suffix)
    #[default]
    Erg,
    /// Token collateral; the 32-byte token id
    Token([u8; 32]),
}

/// Error parsing a denomination id string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DenomKeyParseError {
    #[error("invalid denomination {0:?}: expected \"erg\" or \"token:<64 hex chars>\"")]
    InvalidFormat(String),
    #[error("invalid denomination token id {0:?}: {1}")]
    InvalidTokenId(String, String),
}

impl DenomKey {
    /// Debt-tree key suffix: `[]` for Erg, `token_id` for a token. No tag byte:
    /// both public keys are fixed 33 bytes, so the token id always starts at a
    /// fixed offset and ERG (66-byte input) cannot collide with a token (98-byte
    /// input).
    pub fn debt_key_suffix(&self) -> Vec<u8> {
        match self {
            DenomKey::Erg => Vec::new(),
            DenomKey::Token(token_id) => token_id.to_vec(),
        }
    }

    /// Debt-tree key: blake2b256(owner ‖ receiver ‖ suffix).
    pub fn debt_key(&self, owner_key: &PubKey, receiver_key: &PubKey) -> [u8; 32] {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(owner_key);
        hasher.update(receiver_key);
        hasher.update(self.debt_key_suffix());
        hasher.finalize().into()
    }

    /// Canonical string id: `"erg"` or `"token:<64-hex>"`.
    pub fn denomination_id(&self) -> String {
        match self {
            DenomKey::Erg => "erg".to_string(),
            DenomKey::Token(token_id) => format!("token:{}", hex::encode(token_id)),
        }
    }

    /// Parse a canonical denomination id (`"erg"` or `"token:<64-hex>"`).
    pub fn parse(s: &str) -> Result<Self, DenomKeyParseError> {
        if s.eq_ignore_ascii_case("erg") {
            return Ok(DenomKey::Erg);
        }
        if let Some(hex_part) = s.strip_prefix("token:") {
            let bytes = hex::decode(hex_part).map_err(|e| {
                DenomKeyParseError::InvalidTokenId(hex_part.to_string(), e.to_string())
            })?;
            let arr: [u8; 32] = bytes.try_into().map_err(|b: Vec<u8>| {
                DenomKeyParseError::InvalidTokenId(
                    hex_part.to_string(),
                    format!("expected 32 bytes, got {}", b.len()),
                )
            })?;
            return Ok(DenomKey::Token(arr));
        }
        Err(DenomKeyParseError::InvalidFormat(s.to_string()))
    }

    /// Fixed-width binary encoding: `kind(1) ‖ token_id(32)` (33 bytes);
    /// token_id is all zeros for Erg.
    pub fn encode33(&self) -> [u8; 33] {
        let mut out = [0u8; 33];
        match self {
            DenomKey::Erg => out[0] = 0x00,
            DenomKey::Token(token_id) => {
                out[0] = 0x01;
                out[1..33].copy_from_slice(token_id);
            }
        }
        out
    }

    /// Decode the fixed-width binary form produced by `encode33`.
    pub fn decode33(bytes: &[u8; 33]) -> Result<Self, DenomKeyParseError> {
        match bytes[0] {
            0x00 => Ok(DenomKey::Erg),
            0x01 => {
                let mut id = [0u8; 32];
                id.copy_from_slice(&bytes[1..33]);
                Ok(DenomKey::Token(id))
            }
            other => Err(DenomKeyParseError::InvalidFormat(format!(
                "unknown denomination kind byte 0x{other:02x}"
            ))),
        }
    }
}

impl std::fmt::Display for DenomKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.denomination_id())
    }
}

impl serde::Serialize for DenomKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.denomination_id())
    }
}

impl<'de> serde::Deserialize<'de> for DenomKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        DenomKey::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Compute the tracker debt-tree key for an (issuer, receiver, denomination) triple.
///
/// Erg:   blake2b256(owner ‖ receiver)
/// Token: blake2b256(owner ‖ receiver ‖ tokenId)
pub fn debt_tree_key(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    denomination: &DenomKey,
) -> [u8; 32] {
    denomination.debt_key(owner_key, receiver_key)
}

/// Generate the signing message used when creating off-chain IOU notes.
///
/// message = debtKey || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId
///
/// Where:
/// - debtKey = denomination-derived debt-tree key (32 bytes), see [`DenomKey::debt_key`]
/// - totalDebt: 8-byte big-endian total cumulative debt amount
/// - timestamp: 8-byte big-endian payment timestamp (milliseconds since Unix epoch)
/// - trackerNftId: 32-byte tracker NFT ID binding the signature to one tracker
///   (and thus one reserve contract deployment and one chain — issue #15 C3)
///
/// # Returns
/// * 80 bytes: debtKey (32) || totalDebt (8 BE) || timestamp (8 BE) || trackerNftId (32)
pub fn signing_message(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    denomination: &DenomKey,
    total_debt: u64,
    timestamp: u64,
    tracker_nft_id: &[u8; 32],
) -> Vec<u8> {
    let key_hash = denomination.debt_key(owner_key, receiver_key);

    // Build message: debtKey || totalDebt || timestamp || trackerNftId (always 80 bytes)
    let mut message = Vec::with_capacity(80);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message.extend_from_slice(tracker_nft_id);

    message
}

/// The tag every prior-acknowledgment message starts with: the 17 ASCII bytes
/// `basis/prior-ack/1`, no terminator (see [`prior_ack_message`]).
pub const PRIOR_ACK_TAG: &[u8; 17] = b"basis/prior-ack/1";

/// Length of a prior-acknowledgment message: tag (17) || debtKey (32) ||
/// prior total (8) || prior timestamp (8) || timestamp (8) || trackerNftId (32).
pub const PRIOR_ACK_MESSAGE_LEN: usize = 105;

/// Generate the prior-acknowledgment message `PA(I, R, X, p, T)` (Valun tracker
/// v3.2): the signer acknowledges that the (owner -> receiver, denomination)
/// edge is at total `prior_total` (X) with stored timestamp `prior_timestamp`
/// (p), for the operation at `timestamp` (T).
///
/// message = TAG || debtKey || longToByteArray(X) || longToByteArray(p) || longToByteArray(T) || trackerNftId
///
/// - TAG = [`PRIOR_ACK_TAG`], the 17 ASCII bytes `basis/prior-ack/1`;
/// - debtKey and trackerNftId exactly as in [`signing_message`];
/// - an edge without a note is acknowledged as X = 0, p = 0. A stored note can
///   be (0, 0) too (an ordinary note of total 0 at timestamp 0); that state is
///   economically identical to the absent edge, and it cannot recur after any
///   write, since every write has a later timestamp.
///
/// It is never 80 bytes and starts with the tag, so it can never be taken for a
/// note message, and it binds the prior state (X, p) and the operation's T, so
/// it cannot be reused at another T or for another state of the edge (it does
/// not name the signer's role: whoever acknowledges (X, p) for T signs it).
///
/// # Returns
/// * 105 bytes: TAG (17) || debtKey (32) || X (8 BE) || p (8 BE) || T (8 BE) || trackerNftId (32)
pub fn prior_ack_message(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    denomination: &DenomKey,
    prior_total: u64,
    prior_timestamp: u64,
    timestamp: u64,
    tracker_nft_id: &[u8; 32],
) -> Vec<u8> {
    let key_hash = denomination.debt_key(owner_key, receiver_key);

    let mut message = Vec::with_capacity(PRIOR_ACK_MESSAGE_LEN);
    message.extend_from_slice(PRIOR_ACK_TAG);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&prior_total.to_be_bytes());
    message.extend_from_slice(&prior_timestamp.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message.extend_from_slice(tracker_nft_id);

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compressed public key of a secret key.
    fn public_key_of(secret: &[u8; 32]) -> PubKey {
        let secp = secp256k1::Secp256k1::new();
        let sk = secp256k1::SecretKey::from_slice(secret).unwrap();
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    #[test]
    fn prior_ack_message_reproduces_the_shared_fixed_vector() {
        // The fixed vector every implementation must reproduce (tracker v3.2 design §7): A and B are the public
        // keys of the secrets 0x01 x 32 and 0x02 x 32, denomination erg, tracker NFT 0xab x 32, X = 1000,
        // p = 1762650000123, T = 1762650060999.
        let a = public_key_of(&[0x01; 32]);
        let b = public_key_of(&[0x02; 32]);
        assert_eq!(hex::encode(a), "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f");
        assert_eq!(hex::encode(b), "024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766");
        assert_eq!(
            hex::encode(DenomKey::Erg.debt_key(&a, &b)),
            "8655e13a7d5c621d55abe7b76cd86399fee984d67ecc48a72f2f8aea80fd94e1"
        );
        let nft = [0xab; 32];
        let pa = prior_ack_message(&a, &b, &DenomKey::Erg, 1000, 1_762_650_000_123, 1_762_650_060_999, &nft);
        assert_eq!(
            hex::encode(&pa),
            concat!(
                "62617369732f7072696f722d61636b2f31",
                "8655e13a7d5c621d55abe7b76cd86399fee984d67ecc48a72f2f8aea80fd94e1",
                "00000000000003e8",
                "0000019a66208afb",
                "0000019a662178c7",
                "abababababababababababababababababababababababababababababababab"
            )
        );
        assert_eq!(pa.len(), 105);
        assert_eq!(pa.len(), PRIOR_ACK_MESSAGE_LEN);
        // For comparison, the 80-byte note message M(A,B,X,T) that tracker v3.1 took as the acknowledgment.
        assert_eq!(
            hex::encode(signing_message(&a, &b, &DenomKey::Erg, 1000, 1_762_650_060_999, &nft)),
            concat!(
                "8655e13a7d5c621d55abe7b76cd86399fee984d67ecc48a72f2f8aea80fd94e1",
                "00000000000003e8",
                "0000019a662178c7",
                "abababababababababababababababababababababababababababababababab"
            )
        );
    }

    #[test]
    fn prior_ack_message_is_105_bytes_starts_with_its_tag_and_binds_every_field() {
        let owner = [0x02u8; 33];
        let receiver = [0x03u8; 33];
        let nft = [0x42u8; 32];
        let token = DenomKey::Token([0xa5u8; 32]);
        assert_eq!(PRIOR_ACK_TAG, b"basis/prior-ack/1");
        for d in [DenomKey::Erg, token] {
            let m = prior_ack_message(&owner, &receiver, &d, 1, 2, 3, &nft);
            assert_eq!(m.len(), 105);
            assert_eq!(&m[..17], PRIOR_ACK_TAG);
            assert_eq!(&m[17..49], &d.debt_key(&owner, &receiver));
            assert_eq!(&m[73..], &nft);
            // Never a note message: another length, and a note message starts with the debt key.
            assert_ne!(m.len(), signing_message(&owner, &receiver, &d, 1, 3, &nft).len());
        }
        // The edge's absent state (0, 0) is a message of its own.
        let base = prior_ack_message(&owner, &receiver, &DenomKey::Erg, 0, 0, 3, &nft);
        // Each field changes the message: the edge, the denomination, X, p, T and the tracker.
        let others = [
            prior_ack_message(&receiver, &owner, &DenomKey::Erg, 0, 0, 3, &nft),
            prior_ack_message(&owner, &receiver, &token, 0, 0, 3, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 1, 0, 3, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 0, 1, 3, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 0, 0, 4, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 0, 0, 3, &[0x43u8; 32]),
        ];
        for other in &others {
            assert_ne!(&base, other);
        }
        // X, p and T are three separate fields: swapping two of them changes the message.
        assert_ne!(
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 5, 6, 7, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 6, 5, 7, &nft)
        );
        assert_ne!(
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 5, 6, 7, &nft),
            prior_ack_message(&owner, &receiver, &DenomKey::Erg, 5, 7, 6, &nft)
        );
    }

    #[test]
    fn denomination_id_roundtrip() {
        assert_eq!(DenomKey::Erg.denomination_id(), "erg");
        let id = [0xa5u8; 32];
        let key = DenomKey::Token(id);
        let s = key.denomination_id();
        assert_eq!(s, format!("token:{}", hex::encode(id)));
        assert_eq!(DenomKey::parse(&s).unwrap(), key);
        assert_eq!(DenomKey::parse("ERG").unwrap(), DenomKey::Erg);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(DenomKey::parse("").is_err());
        assert!(DenomKey::parse("usd").is_err());
        assert!(DenomKey::parse("token:zz").is_err());
        assert!(DenomKey::parse(&format!("token:{}", "aa".repeat(31))).is_err());
    }

    #[test]
    fn encode33_roundtrip() {
        for k in [DenomKey::Erg, DenomKey::Token([0x42u8; 32])] {
            assert_eq!(DenomKey::decode33(&k.encode33()).unwrap(), k);
        }
        assert!(DenomKey::decode33(&[0x07u8; 33]).is_err());
    }

    #[test]
    fn debt_key_namespaces_differ() {
        let owner = [0x02u8; 33];
        let receiver = [0x03u8; 33];
        let erg = DenomKey::Erg.debt_key(&owner, &receiver);
        let tok = DenomKey::Token([0xa5u8; 32]).debt_key(&owner, &receiver);
        let tok2 = DenomKey::Token([0xa6u8; 32]).debt_key(&owner, &receiver);
        assert_ne!(erg, tok);
        assert_ne!(tok, tok2);
    }

    #[test]
    fn erg_debt_key_matches_legacy_derivation() {
        // The ERG key must stay blake2b256(owner ‖ receiver), byte-identical to the
        // pre-denomination derivation (contract/basis.es is unchanged).
        let owner = [0x02u8; 33];
        let receiver = [0x03u8; 33];
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(owner);
        hasher.update(receiver);
        let legacy: [u8; 32] = hasher.finalize().into();
        assert_eq!(DenomKey::Erg.debt_key(&owner, &receiver), legacy);
    }

    #[test]
    fn signing_message_is_80_bytes() {
        let owner = [0x02u8; 33];
        let receiver = [0x03u8; 33];
        let nft = [0x42u8; 32];
        assert_eq!(
            signing_message(&owner, &receiver, &DenomKey::Erg, 1, 2, &nft).len(),
            80
        );
        assert_eq!(
            signing_message(
                &owner,
                &receiver,
                &DenomKey::Token([0xa5u8; 32]),
                1,
                2,
                &nft
            )
            .len(),
            80
        );
    }
}
