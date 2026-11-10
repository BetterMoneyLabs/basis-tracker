//! Chain-sourced repair of the reserve AVL tree mirror (issue #15 review N14).
//!
//! The tracker's per-(issuer, denomination) reserve tree normally advances through the
//! redemption API flow alone. When a redemption confirms outside that flow (a creditor
//! broadcasting the signed transaction directly, an emergency redemption, a reorg) or an
//! optimistic update never confirms (a top-up won the race), the mirror diverges from the
//! reserve box's on-chain R5 and `/redemption/build` skips the reserve permanently.
//!
//! This module walks the reserve lineage backwards over the extra-indexed scanner node,
//! extracting each redemption generation's operation from the spending transaction's
//! context extension (var #1 = receiver key, var #4 = note timestamp; the redeemed delta is
//! the collateral decrease between the spent box and its successor), until it reaches a
//! generation whose R5 equals the current mirror digest ([`ReserveSyncPoint::Mirror`]) or
//! the lineage root with the empty tree ([`ReserveSyncPoint::Genesis`]). The collected ops
//! are then staged and verified by
//! [`basis_store::BasisTracker::apply_chain_reserve_ops`].

use basis_store::{ChainReserveOp, ReserveSyncPoint};

use crate::redemption_build::NodeClient;

/// Maximum generations walked backwards when collecting missed redemptions. A deeper
/// divergence aborts the repair (and the build) with an error rather than hammering the
/// node; the next build retries.
const MAX_LINEAGE_WALK: usize = 64;

/// The 33-byte digest of the empty reserve AVL tree — the R5 of a freshly created reserve
/// (`64 || digest || 03 || 20 || 00` as a serialized SAvlTree). A lineage generation whose
/// predecessor has this R5 is the first tree mutation after the root box.
/// Cross-checked against `basis_trees::BasisAvlTree::new()` in basis_store tests.
pub(crate) const EMPTY_RESERVE_TREE_DIGEST_HEX: &str =
    "4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900";

/// One generation of the reserve lineage: the box being spent (predecessor) as an indexed
/// input of the spending transaction.
#[derive(Debug, Clone)]
pub(crate) struct ReserveInput {
    /// Creating transaction of the predecessor box (the walk's next step).
    pub creating_tx_id: String,
    /// Predecessor's reserve tree digest (hex, 66 chars), extracted from its R5.
    pub r5_digest: String,
    /// Predecessor's collateral: nanoERG value for ERG reserves, collateral token amount
    /// for token reserves.
    pub collateral: u64,
    /// Spending proof extension: var index as decimal string → hex SValue.
    pub extension: std::collections::HashMap<String, String>,
}

/// What the walk decided about one generation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WalkStep {
    /// Move on to the predecessor's creating transaction.
    Descend { creating_tx_id: String },
    /// A generation's R5 matches the mirror; apply the collected ops on top.
    MirrorMatch,
    /// Reached the lineage root (empty-tree R5) without a mirror match; rebuild.
    Genesis,
}

/// Extract the 33-byte AVL digest from a serialized SAvlTree register value
/// (`0x64 || 33-byte digest || flags || keylen || valuelen`).
pub(crate) fn r5_digest_from_register(r5_hex: &str) -> Option<String> {
    if r5_hex.len() >= 68 && r5_hex.starts_with("64") {
        Some(r5_hex[2..68].to_string())
    } else {
        None
    }
}

/// Decode extension var #1 (receiver GroupElement, `0x07 || 33-byte compressed point`).
fn decode_receiver(value: &str) -> Result<[u8; 33], String> {
    let hex_part = value
        .strip_prefix("07")
        .ok_or_else(|| format!("extension var #1 is not a GroupElement: {value}"))?;
    let bytes =
        hex::decode(hex_part).map_err(|e| format!("extension var #1 is not hex: {e}"))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| format!("extension var #1 is {} bytes, expected 33", bytes.len()))
}

/// Decode extension var #4 (note timestamp: Long, `0x05 || zigzag-VLQ`).
fn decode_timestamp(value: &str) -> Result<u64, String> {
    let hex_part = value
        .strip_prefix("05")
        .ok_or_else(|| format!("extension var #4 is not a Long: {value}"))?;
    let bytes =
        hex::decode(hex_part).map_err(|e| format!("extension var #4 is not hex: {e}"))?;
    let mut zigzag: u64 = 0;
    let mut shift = 0;
    for &byte in &bytes {
        zigzag |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            let n = zigzag as i64;
            let decoded = (n >> 1) ^ -(n & 1);
            return u64::try_from(decoded)
                .map_err(|_| format!("extension var #4 is negative: {decoded}"));
        }
        shift += 7;
        if shift > 63 {
            return Err("extension var #4 VLQ overflow".to_string());
        }
    }
    Err("extension var #4 has an incomplete VLQ".to_string())
}

/// Decide what one lineage generation means for the mirror, and where to walk next.
///
/// `input` is the predecessor box as an indexed input of its spending transaction;
/// `cur_r5`/`cur_collateral` describe the generation's successor box. A generation whose
/// R5 changed is a redemption (top-ups and refund initiations preserve R5); its op is
/// recovered from the extension and the collateral decrease.
pub(crate) fn walk_step(
    input: &ReserveInput,
    cur_r5: &str,
    cur_collateral: u64,
    mirror_digest: &str,
) -> Result<(Option<ChainReserveOp>, WalkStep), String> {
    let mut op = None;
    if !input.r5_digest.eq_ignore_ascii_case(cur_r5) {
        let receiver = decode_receiver(
            input
                .extension
                .get("1")
                .ok_or("redemption spend is missing extension var #1 (receiver)")?,
        )?;
        let timestamp = decode_timestamp(
            input
                .extension
                .get("4")
                .ok_or("redemption spend is missing extension var #4 (timestamp)")?,
        )?;
        let delta = input
            .collateral
            .checked_sub(cur_collateral)
            .filter(|d| *d > 0)
            .ok_or_else(|| {
                format!(
                    "redemption generation did not decrease collateral ({} -> {})",
                    input.collateral, cur_collateral
                )
            })?;
        op = Some(ChainReserveOp {
            recipient_pubkey: receiver,
            timestamp,
            redeemed_delta: delta,
        });
    }

    if input.r5_digest.eq_ignore_ascii_case(mirror_digest) {
        return Ok((op, WalkStep::MirrorMatch));
    }
    if input
        .r5_digest
        .eq_ignore_ascii_case(EMPTY_RESERVE_TREE_DIGEST_HEX)
    {
        return Ok((op, WalkStep::Genesis));
    }
    if input.creating_tx_id.is_empty() {
        return Err("lineage walk reached a box with no creating transaction".to_string());
    }
    Ok((
        op,
        WalkStep::Descend {
            creating_tx_id: input.creating_tx_id.clone(),
        },
    ))
}

/// Extract the reserve-contract input of a transaction as a [`ReserveInput`].
fn reserve_input_from_tx(
    tx: &serde_json::Value,
    contract_address: &str,
    collateral_token_id: Option<&str>,
) -> Result<ReserveInput, String> {
    let inputs = tx
        .get("inputs")
        .and_then(|i| i.as_array())
        .ok_or("transaction has no inputs array")?;
    let input = inputs
        .iter()
        .find(|i| {
            i.get("address")
                .and_then(|a| a.as_str())
                .map(|a| a == contract_address)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("transaction has no input at {contract_address}"))?;

    let r5_digest = input
        .get("additionalRegisters")
        .and_then(|r| r.get("R5"))
        .and_then(|v| v.as_str())
        .and_then(r5_digest_from_register)
        .ok_or("reserve input has no usable R5 register")?;
    let collateral = box_collateral(input, collateral_token_id)?;
    let extension = input
        .get("spendingProof")
        .and_then(|p| p.get("extension"))
        .and_then(|e| e.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(ReserveInput {
        creating_tx_id: input
            .get("transactionId")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
        r5_digest,
        collateral,
        extension,
    })
}

/// Collateral carried by a box JSON value: nanoERG for ERG reserves, the collateral
/// token amount for token reserves.
fn box_collateral(box_json: &serde_json::Value, collateral_token_id: Option<&str>) -> Result<u64, String> {
    match collateral_token_id {
        None => box_json
            .get("value")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "box has no value".to_string()),
        Some(token_id) => Ok(box_json
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|assets| {
                assets
                    .iter()
                    .filter(|a| {
                        a.get("tokenId")
                            .and_then(|t| t.as_str())
                            .map(|t| t.eq_ignore_ascii_case(token_id))
                            .unwrap_or(false)
                    })
                    .filter_map(|a| a.get("amount").and_then(|v| v.as_u64()))
                    .sum()
            })
            .unwrap_or(0)),
    }
}

/// Collect the redemption ops needed to bring the (issuer, denomination) mirror from its
/// current digest to the R5 of `successor_box_id`, by walking the reserve lineage
/// backwards over the extra-indexed scanner node.
///
/// Returns the ops in chain (oldest-first) order and the sync point the walk stopped at.
pub(crate) async fn collect_lineage_redemption_ops(
    node: &NodeClient,
    contract_address: &str,
    collateral_token_id: Option<&str>,
    successor_box_id: &str,
    mirror_digest: &str,
) -> Result<(Vec<ChainReserveOp>, ReserveSyncPoint), String> {
    // The indexed box tells us which transaction created the tracked successor.
    let successor = node
        .get_json(&format!("/blockchain/box/byId/{successor_box_id}"))
        .await?;
    let mut cur_tx_id = successor
        .get("transactionId")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("no creating transaction for box {successor_box_id}"))?
        .to_string();
    let mut cur_r5 = successor
        .get("additionalRegisters")
        .and_then(|r| r.get("R5"))
        .and_then(|v| v.as_str())
        .and_then(r5_digest_from_register)
        .ok_or_else(|| format!("box {successor_box_id} has no usable R5 register"))?;
    let mut cur_collateral = box_collateral(&successor, collateral_token_id)?;

    if cur_r5.eq_ignore_ascii_case(mirror_digest) {
        // Already in sync; nothing to repair.
        return Ok((Vec::new(), ReserveSyncPoint::Mirror));
    }

    let mut ops = Vec::new();
    for _ in 0..MAX_LINEAGE_WALK {
        let tx = node.get_json(&format!("/blockchain/transaction/byId/{cur_tx_id}")).await?;
        let input = reserve_input_from_tx(&tx, contract_address, collateral_token_id)?;
        let (op, step) = walk_step(&input, &cur_r5, cur_collateral, mirror_digest)?;
        if let Some(op) = op {
            ops.push(op);
        }
        match step {
            WalkStep::MirrorMatch => {
                ops.reverse();
                return Ok((ops, ReserveSyncPoint::Mirror));
            }
            WalkStep::Genesis => {
                ops.reverse();
                return Ok((ops, ReserveSyncPoint::Genesis));
            }
            WalkStep::Descend { creating_tx_id } => {
                cur_tx_id = creating_tx_id;
                cur_r5 = input.r5_digest;
                cur_collateral = input.collateral;
            }
        }
    }
    Err(format!(
        "lineage walk exceeded {MAX_LINEAGE_WALK} generations without finding a sync point"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECEIVER: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const R5_A: &str = "64aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa030000";
    const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn input(r5: &str, collateral: u64, extension: &[(&str, &str)]) -> ReserveInput {
        ReserveInput {
            creating_tx_id: "tx-prev".to_string(),
            r5_digest: r5_digest_from_register(r5).unwrap(),
            collateral,
            extension: extension
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn redemption_extension() -> Vec<(&'static str, String)> {
        // Long 1000 zigzag-VLQ: 1000 << 1 = 2000 = 0x07D0 → VLQ d0 0f
        vec![
            ("1", format!("07{RECEIVER}")),
            ("4", "05d00f".to_string()),
        ]
    }

    #[test]
    fn r5_digest_strips_savltree_prefix() {
        assert_eq!(r5_digest_from_register(R5_A).unwrap(), DIGEST_A);
        assert!(r5_digest_from_register("").is_none());
        assert!(r5_digest_from_register("0564").is_none());
    }

    #[test]
    fn decodes_receiver_and_timestamp() {
        assert_eq!(decode_receiver(&format!("07{RECEIVER}")).unwrap()[..], hex::decode(RECEIVER).unwrap()[..]);
        assert!(decode_receiver(RECEIVER).is_err()); // missing 0x07 prefix
        assert_eq!(decode_timestamp("05d00f").unwrap(), 1000);
        assert_eq!(decode_timestamp("0500").unwrap(), 0);
        assert!(decode_timestamp("0ad00f").is_err()); // not a Long
    }

    #[test]
    fn top_up_generation_produces_no_op() {
        // R5 unchanged between generations: top-up or refund initiation.
        let input = input(R5_A, 2_000_000, &[]);
        let (op, step) = walk_step(&input, DIGEST_A, 3_000_000, DIGEST_B).unwrap();
        assert!(op.is_none());
        assert_eq!(
            step,
            WalkStep::Descend {
                creating_tx_id: "tx-prev".to_string()
            }
        );
    }

    #[test]
    fn redemption_generation_extracts_op() {
        let ext = redemption_extension();
        let ext_refs: Vec<(&str, &str)> = ext.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let input = input(R5_A, 5_000_000, &ext_refs);
        // The mirror matches the PREDECESSOR digest: the op bridges to the successor.
        let (op, step) = walk_step(&input, DIGEST_B, 4_000_000, DIGEST_A).unwrap();
        let op = op.expect("redemption op");
        assert_eq!(op.recipient_pubkey[..], hex::decode(RECEIVER).unwrap()[..]);
        assert_eq!(op.timestamp, 1000);
        assert_eq!(op.redeemed_delta, 1_000_000);
        assert_eq!(step, WalkStep::MirrorMatch);
    }

    #[test]
    fn genesis_when_reaching_empty_tree_without_mirror_match() {
        let ext = redemption_extension();
        let ext_refs: Vec<(&str, &str)> = ext.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let empty_r5 = format!("64{EMPTY_RESERVE_TREE_DIGEST_HEX}030000");
        let input = input(&empty_r5, 1_000_000, &ext_refs);
        let (op, step) = walk_step(&input, DIGEST_B, 500_000, DIGEST_A).unwrap();
        assert!(op.is_some());
        assert_eq!(step, WalkStep::Genesis);
    }

    #[test]
    fn redemption_without_collateral_decrease_is_rejected() {
        let ext = redemption_extension();
        let ext_refs: Vec<(&str, &str)> = ext.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let input = input(R5_A, 1_000_000, &ext_refs);
        assert!(walk_step(&input, DIGEST_B, 1_000_000, DIGEST_B).is_err());
    }

    #[test]
    fn missing_extension_vars_are_rejected() {
        let input = input(R5_A, 2_000_000, &[]);
        assert!(walk_step(&input, DIGEST_B, 1_000_000, DIGEST_B).is_err());
    }
}
