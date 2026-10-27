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

#[cfg(test)]
mod tests {
    use super::*;

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
