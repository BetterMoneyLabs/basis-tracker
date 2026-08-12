//! Shared Ergo transaction serialization and context-extension ordering helpers.
//!
//! These are byte-exact building blocks used by both the CLI and the server to construct an
//! unsigned Basis redemption transaction that the Ergo node and the on-chain reserve contract
//! accept. The context-extension key order matters: `UnsignedInput` is `boxId ++ extension`, so
//! the extension byte order is part of `bytes_to_sign`. Scala serializes its
//! `sigma.interpreter.ContextExtension` map (a `Map[Byte, Constant]`, an immutable.HashMap /
//! HashTrieMap) in iteration order, and ergo-lib serializes its insertion-ordered map as-is, so
//! the keys must be inserted in Scala's order or the local `bytes_to_sign` diverges from the node
//! and `proveDlog(receiver)` verification fails.

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;

/// Encode an unsigned integer using Ergo's VLQ (Variable-Length Quantity) encoding.
/// Bytes are emitted least-significant group first (little-endian VLQ).
pub fn vlq_encode(mut value: usize) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let mut bytes = Vec::new();
    while value > 0 {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
    }
    bytes
}

/// Serialize bytes as an Ergo `Coll[Byte]` constant: type prefix `0x0e` + VLQ(length) + data.
/// The length is VLQ-encoded (not a single byte), so collections of 128 bytes or more — e.g.
/// larger AVL lookup/insert proofs — are encoded correctly.
pub fn serialize_coll_bytes(data: &[u8]) -> String {
    let mut bytes = vec![0x0e];
    bytes.extend_from_slice(&vlq_encode(data.len()));
    bytes.extend_from_slice(data);
    hex::encode(bytes)
}

/// Serialize a long value as an Ergo `Long` constant: type prefix `0x05` + zigzag(VLQ),
/// using Ergo's little-endian VLQ byte order.
pub fn serialize_ergo_long(value: i64) -> String {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    format!("05{}", hex::encode(vlq_encode(zigzag as usize)))
}

/// Serialize a byte value as an Ergo constant (prefix `02`).
pub fn serialize_ergo_byte(value: u8) -> String {
    format!("02{:02x}", value)
}

/// Build a serialized `SAvlTree` constant matching Scala's
/// `ValueSerializer.serialize(AvlTreeConstant(tree))`.
/// Format: `0x64 || 33-byte digest || flags || VLQ key_length || VLQ value_length`.
pub fn build_savl_tree_from_digest(digest_hex: &str) -> Vec<u8> {
    let digest_bytes = hex::decode(digest_hex).unwrap_or_else(|_| vec![0u8; 33]);

    // The server returns a 33-byte root digest: 32-byte AVL digest + 1-byte flags.
    // Scala's SAvlTree serialization is: type byte 0x64 || 32-byte digest || flags
    // || VLQ key_length || VLQ value_length.
    let root_digest: Vec<u8> = if digest_bytes.len() >= 33 {
        digest_bytes[..33].to_vec()
    } else {
        // Pad short digests (should not happen in practice).
        let mut padded = vec![0u8; 33];
        padded[..digest_bytes.len()].copy_from_slice(&digest_bytes);
        padded
    };

    let mut r5_bytes = Vec::with_capacity(38);
    r5_bytes.push(0x64u8); // SAvlTree type byte
    r5_bytes.extend_from_slice(&root_digest); // 33-byte digest from the AVL prover
    r5_bytes.push(0x03u8); // flags: insertions and updates allowed (insertOrUpdate contract)
    r5_bytes.extend_from_slice(&vlq_encode(32)); // key length
    r5_bytes.extend_from_slice(&vlq_encode(0)); // value length: variable (0)

    r5_bytes
}

/// Decode a server-returned box id that may be (double) hex-encoded.
///
/// The tracker's reserve DB sometimes stores the 64-char box id itself hex-encoded (a 128-char
/// string whose hex bytes are ASCII hex chars). This unwraps up to two levels of such encoding and
/// returns the canonical box id; a plain box id is returned unchanged.
pub fn decode_box_id(raw: &str) -> String {
    let mut cur = raw.to_string();
    for _ in 0..2 {
        if !cur.len().is_multiple_of(2) {
            break;
        }
        match hex::decode(&cur) {
            Ok(bytes) if bytes.iter().all(|b| b.is_ascii_hexdigit()) => {
                match String::from_utf8(bytes) {
                    Ok(s) if s.len() < cur.len() => {
                        cur = s;
                        continue;
                    }
                    _ => break,
                }
            }
            _ => break,
        }
    }
    cur
}

/// Blake2b-256 hash (Ergo's hashing primitive).
pub fn blake2b256_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.finalize().to_vec().try_into().unwrap()
}

/// Convert a 33-byte compressed secp256k1 public key (hex) to a mainnet P2PK address string.
pub fn pubkey_to_address(pubkey_hex: &str) -> Result<String, String> {
    use ergo_lib::ergo_chain_types::EcPoint;
    use ergo_lib::ergotree_ir::chain::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

    let pubkey_bytes =
        hex::decode(pubkey_hex).map_err(|e| format!("invalid public key hex: {e}"))?;
    if pubkey_bytes.len() != 33 {
        return Err("public key must be 33 bytes".to_string());
    }
    let ec_point = EcPoint::sigma_parse_bytes(&pubkey_bytes)
        .map_err(|e| format!("invalid public key format: {e:?}"))?;
    let address = Address::P2Pk(ProveDlog::new(ec_point));
    let encoder =
        ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

/// Convert a P2S or P2PK address string to its hex-encoded ergoTree.
pub fn address_to_ergo_tree(address_str: &str) -> Result<String, String> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let address = encoder
        .parse_address_from_str(address_str)
        .map_err(|e| format!("invalid address '{address_str}': {e}"))?;
    let tree = address
        .script()
        .map_err(|e| format!("failed to get script for address '{address_str}': {e}"))?;
    hex::encode(
        tree.sigma_serialize_bytes()
            .map_err(|e| format!("failed to serialize ergoTree: {e:?}"))?,
    )
    .pipe(Ok)
}

/// Derive the P2PK address from a P2PK (proveDlog) ergoTree (hex). Used to find the address that
/// owns a fee-input box so the correct local secret is used to sign it. A P2PK ergoTree serializes
/// as `00 08 cd <33-byte compressed point>`, so the public key is bytes `[3..36]`.
pub fn ergo_tree_to_p2pk_address(ergo_tree_hex: &str) -> Result<String, String> {
    use ergo_lib::ergo_chain_types::EcPoint;
    use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

    let bytes = hex::decode(ergo_tree_hex).map_err(|e| format!("invalid ergoTree hex: {e}"))?;
    if bytes.len() != 36 || bytes[0] != 0x00 || bytes[1] != 0x08 || bytes[2] != 0xcd {
        return Err("ergoTree is not a P2PK (proveDlog) script".to_string());
    }
    let point = EcPoint::sigma_parse_bytes(&bytes[3..])
        .map_err(|e| format!("failed to parse P2PK point: {e:?}"))?;
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&Address::P2Pk(ProveDlog::from(point))))
}

/// Scala `immutable.HashMap` (HashTrieMap, Scala 2.12) iteration order for the given byte keys.
/// This is exactly the order `sigma.interpreter.ContextExtension` (a `Map[Byte, Constant]`) is
/// serialized in (`obj.values.foreach`). Validated on-chain against a node-signed redemption for
/// the first-redemption set `{0,1,2,3,4,5,6,8}` (order `0,5,1,6,2,3,8,4`); the same model yields
/// the order for the full set `{0..8}` used by subsequent redemptions (which add `#7`).
pub fn scala_context_extension_order(keys: &[u8]) -> Vec<u8> {
    // `Byte.hashCode` widens the byte to Int; `improve` is Scala HashMap's hash mixing (Murmur3-style).
    fn improve(h: u32) -> u32 {
        let h = h.wrapping_add(!(h.wrapping_shl(9)));
        let h = h ^ (h >> 14);
        let h = h.wrapping_add(h.wrapping_shl(4));
        h ^ (h >> 10)
    }
    fn build(level: u32, keys: &[u8], out: &mut Vec<u8>) {
        use std::collections::BTreeMap;
        // Group by the 5-bit trie index at this level; BTreeMap yields ascending bucket order,
        // matching the bitmap/elems array iteration of HashTrieMap.
        let mut groups: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
        for &k in keys {
            let idx = (improve(k as u32) >> level) & 0x1f;
            groups.entry(idx).or_default().push(k);
        }
        for (_idx, group) in groups {
            if group.len() == 1 {
                out.push(group[0]);
            } else if level >= 30 {
                // HashMapCollision1: iterates its list in insertion order.
                out.extend(group);
            } else {
                build(level + 5, &group, out);
            }
        }
    }
    let mut out = Vec::new();
    build(0, keys, &mut out);
    out
}

/// Reorder the reserve input's (`inputs[0]`) context-extension keys in the JSON so that ergo-lib
/// parses them into its insertion-ordered map in Scala's `ContextExtension` serialization order,
/// for whichever variable indices are present (first-redemption set without `#7`, or the full set
/// `{0..8}` for subsequent redemptions).
pub fn reorder_reserve_extension_scala(tx: &mut serde_json::Value) {
    use serde_json::Map;
    let ext = match tx
        .get_mut("inputs")
        .and_then(|i| i.get_mut(0))
        .and_then(|inp| inp.get_mut("extension"))
        .and_then(|e| e.as_object_mut())
    {
        Some(e) => e,
        None => return,
    };
    let present: Vec<u8> = ext.keys().filter_map(|k| k.parse::<u8>().ok()).collect();
    let order = scala_context_extension_order(&present);
    let mut reordered: Map<String, serde_json::Value> = Map::new();
    for k in &order {
        let ks = k.to_string();
        if let Some(v) = ext.remove(&ks) {
            reordered.insert(ks, v);
        }
    }
    let remaining: Vec<String> = ext.keys().cloned().collect();
    for k in remaining {
        if let Some(v) = ext.remove(&k) {
            reordered.insert(k, v);
        }
    }
    *ext = reordered;
}

// Small helper to keep the address helpers readable without an extra combinator dep.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T: Sized> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    const P2PK_TREE: &str =
        "0008cd02725e8878d5198ca7f5853dddf35560ddab05ab0a26adae7e664b84162c9962e5";
    const GENERATOR_GE: &str =
        "070279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const SCALA_EXT_ORDER_NO_R7: &[&str] = &["0", "5", "1", "6", "2", "3", "8", "4"];

    fn tx_json_with_ext_keys(keys_in_order: &[&str]) -> Value {
        let mut ext: Map<String, Value> = Map::new();
        for k in keys_in_order {
            let v = match *k {
                "0" => serialize_ergo_byte(0),
                "1" => GENERATOR_GE.to_string(),
                "2" => serialize_coll_bytes(&[0u8; 65]),
                "3" => serialize_ergo_long(400_000_000),
                "4" => serialize_ergo_long(1_783_612_740_170),
                "5" => serialize_coll_bytes(&[]),
                "6" => serialize_coll_bytes(&[0u8; 65]),
                "7" => serialize_coll_bytes(&[0u8; 32]),
                "8" => serialize_coll_bytes(&[]),
                _ => serialize_coll_bytes(&[]),
            };
            ext.insert((*k).to_string(), json!(v));
        }
        json!({
            "inputs": [
                { "boxId": "01".repeat(32), "extension": Value::Object(ext) }
            ],
            "dataInputs": [],
            "outputs": [
                {
                    "value": 1_000_000,
                    "ergoTree": P2PK_TREE,
                    "creationHeight": 1,
                    "assets": [],
                    "additionalRegisters": {}
                }
            ]
        })
    }

    fn ext_json_keys(tx: &Value) -> Vec<String> {
        tx["inputs"][0]["extension"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn reorder_produces_scala_order_for_first_redemption_set() {
        let mut tx = tx_json_with_ext_keys(&["6", "1", "3", "4", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(ext_json_keys(&tx), SCALA_EXT_ORDER_NO_R7);
    }

    #[test]
    fn reorder_is_idempotent_when_already_in_scala_order() {
        let mut tx = tx_json_with_ext_keys(SCALA_EXT_ORDER_NO_R7);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(ext_json_keys(&tx), SCALA_EXT_ORDER_NO_R7);
    }

    #[test]
    fn reorder_produces_scala_order_for_subsequent_redemption_set() {
        let mut tx = tx_json_with_ext_keys(&["6", "1", "3", "4", "7", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(
            ext_json_keys(&tx),
            &["0", "5", "1", "6", "2", "7", "3", "8", "4"]
        );
    }

    #[test]
    fn reorder_orders_arbitrary_key_set_in_scala_order() {
        // Full {0..9} set: keys are ordered by Scala HashTrieMap iteration, not by insertion.
        let mut tx = tx_json_with_ext_keys(&["9", "6", "1", "7", "3", "4", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(
            ext_json_keys(&tx),
            &["0", "5", "1", "6", "9", "2", "7", "3", "8", "4"]
        );
    }

    #[test]
    fn scala_context_extension_order_matches_known_sets() {
        assert_eq!(
            scala_context_extension_order(&[0, 1, 2, 3, 4, 5, 6, 8]),
            vec![0, 5, 1, 6, 2, 3, 8, 4]
        );
        assert_eq!(
            scala_context_extension_order(&[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            vec![0, 5, 1, 6, 2, 7, 3, 8, 4]
        );
        // Order is independent of the input slice order.
        assert_eq!(
            scala_context_extension_order(&[8, 6, 5, 4, 3, 2, 1, 0]),
            vec![0, 5, 1, 6, 2, 3, 8, 4]
        );
    }

    #[test]
    fn vlq_encode_matches_ergo_encoding() {
        assert_eq!(vlq_encode(0), vec![0x00]);
        assert_eq!(vlq_encode(127), vec![0x7f]);
        assert_eq!(vlq_encode(128), vec![0x80, 0x01]);
        assert_eq!(vlq_encode(200), vec![0xc8, 0x01]);
        assert_eq!(vlq_encode(16384), vec![0x80, 0x80, 0x01]);
    }

    #[test]
    fn serialize_ergo_byte_encodes_type_and_value() {
        assert_eq!(serialize_ergo_byte(0), "0200");
        assert_eq!(serialize_ergo_byte(255), "02ff");
    }

    #[test]
    fn serialize_ergo_long_encodes_zigzag_vlq() {
        assert_eq!(serialize_ergo_long(0), "0500");
        // -1 zigzags to 1.
        assert_eq!(serialize_ergo_long(-1), "0501");
        // 1 zigzags to 2.
        assert_eq!(serialize_ergo_long(1), "0502");
    }

    #[test]
    fn serialize_coll_bytes_uses_vlq_length() {
        // Empty: type 0x0e + length 0.
        assert_eq!(serialize_coll_bytes(&[]), "0e00");
        // 65 bytes (< 128): single-byte length 0x41.
        let sig = serialize_coll_bytes(&[0u8; 65]);
        assert!(sig.starts_with("0e41"));
        assert_eq!(sig.len(), 2 + 2 + 65 * 2);
        // 200 bytes (>= 128): length is VLQ 200 -> [0xc8, 0x01].
        let long = serialize_coll_bytes(&[0u8; 200]);
        assert!(long.starts_with("0ec801"));
        assert_eq!(long.len(), 2 + 4 + 200 * 2);
    }

    #[test]
    fn address_round_trip_yields_p2pk_ergo_tree() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let address = pubkey_to_address(pubkey).unwrap();
        let tree = address_to_ergo_tree(&address).unwrap();
        assert_eq!(tree, format!("0008cd{}", pubkey));
    }

    #[test]
    fn decode_box_id_unwraps_double_hex() {
        let id = "40ba67cdced22fb033695ef3af5fd91276d3801d72924b23f0ab7f7b0c86208f";
        // Plain id passes through.
        assert_eq!(decode_box_id(id), id);
        // Double-hex-encoded (128-char) unwraps to the plain id.
        let encoded = hex::encode(id.as_bytes());
        assert_eq!(encoded.len(), 128);
        assert_eq!(decode_box_id(&encoded), id);
    }

    #[test]
    fn ergo_tree_to_p2pk_address_parses_point() {
        // P2PK tree -> address -> back to the same tree.
        let addr = ergo_tree_to_p2pk_address(P2PK_TREE).unwrap();
        assert_eq!(address_to_ergo_tree(&addr).unwrap(), P2PK_TREE);
        // Non-P2PK tree rejected.
        assert!(ergo_tree_to_p2pk_address("0008cd00").is_err());
    }

    #[test]
    fn build_savl_tree_from_digest_matches_onchain_r5_format() {
        // Real empty-tree reserve R5 created on-chain (tx 2a5e653a...): the digest of an empty
        // AVL tree is 4ec61f48... and the serialized SAvlTree register is
        // 0x64 || 33-byte digest || 0x03 flags (insert+update) || VLQ(32) keylen || VLQ(0) valuelen.
        let empty_digest = "4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900";
        let bytes = build_savl_tree_from_digest(empty_digest);
        assert_eq!(
            hex::encode(&bytes),
            "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000"
        );
        // Layout invariants.
        assert_eq!(bytes.len(), 37);
        assert_eq!(bytes[0], 0x64);
        assert_eq!(&bytes[1..34], &hex::decode(empty_digest).unwrap()[..]);
        assert_eq!(&bytes[34..], &[0x03, 0x20, 0x00]);
    }

    #[test]
    fn build_savl_tree_from_digest_handles_bad_input() {
        // Invalid hex falls back to a zero digest; short hex is zero-padded. Both must still
        // produce a well-formed 37-byte register value.
        let bad = build_savl_tree_from_digest("zz");
        assert_eq!(bad.len(), 37);
        assert_eq!(bad[0], 0x64);
        let short = build_savl_tree_from_digest("abcd");
        assert_eq!(short.len(), 37);
        assert_eq!(&short[1..3], &[0xab, 0xcd]);
        assert!(short[3..34].iter().all(|b| *b == 0));
    }

    #[test]
    fn decode_box_id_edge_cases() {
        // Odd-length string passes through unchanged.
        assert_eq!(decode_box_id("abc"), "abc");
        // Non-hex passes through unchanged.
        assert_eq!(decode_box_id("not a box id!"), "not a box id!");
        // Empty string passes through.
        assert_eq!(decode_box_id(""), "");
        // Even-length hex whose bytes are NOT ASCII hex chars passes through unchanged
        // (a plain 64-char box id must never be "unwrapped").
        let id = "9f1a413a239a9ad61561924834c0e2b9acbb6a3d0443fdc4b6aa355a83aaf450";
        assert_eq!(decode_box_id(id), id);
    }
}
