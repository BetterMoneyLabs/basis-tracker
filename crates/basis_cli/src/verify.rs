//! Client-side checks on what the tracker returns.
//!
//! The tracker is not trusted with anyone's funds, and the clients may reach it over plain HTTP.
//! Its responses are therefore checked here, against what the client asked for and the reserve
//! contracts compiled into this codebase, before the owner's wallet pays for a reserve or the
//! creditor's key signs a redemption. Shared by the CLI and the TUI wallet.

use anyhow::{anyhow, Result};
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::ergo_chain_types::EcPoint;
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder};
use ergo_lib::ergotree_ir::chain::ergo_box::{
    BoxId, BoxTokens, ErgoBox, ErgoBoxCandidate, NonMandatoryRegisterId,
};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::{Constant, TryExtractInto};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

use basis_store::contract_compiler::{
    get_basis_reserve_contract_p2s, get_basis_token_reserve_contract_p2s,
};
use basis_store::DenomKey;

use crate::api::{CreateReserveRequest, ReserveCreationResponse};

/// R5 of a new reserve: the empty AVL tree (digest for `PlasmaParameters(32, None)`), flags 0x03
/// (insert + update), key length 32, variable value length.
const EMPTY_RESERVE_TREE_R5: &str =
    "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000";

/// Check a `POST /reserves/create` payload before the owner's wallet is asked to fund it.
///
/// The owner's node pays exactly what the payload says, so a dishonest tracker could otherwise
/// put its own key in R4 of the genuine contract, use another script, or add requests or assets.
/// The payload must be the single reserve box `request` describes: the reserve contract compiled
/// into this client for its denomination, the requested ERG value, the reserve NFT (amount 1)
/// followed by the collateral token for a token reserve, R4 = owner key, R5 = empty reserve tree,
/// R6 = a 32-byte tracker NFT id (`expected_tracker_nft_id` when the caller knows it) and R7
/// absent or Long 0.
pub fn verify_reserve_payload(
    request: &CreateReserveRequest,
    payload: &ReserveCreationResponse,
    expected_tracker_nft_id: Option<&str>,
) -> Result<()> {
    let [reserve] = payload.requests.as_slice() else {
        return Err(anyhow!(
            "expected exactly one payment request, got {}",
            payload.requests.len()
        ));
    };

    let denomination = if request.token_id.is_empty() {
        DenomKey::Erg
    } else {
        DenomKey::Token(decode_array(&request.token_id, "token id")?)
    };
    if address_tree_bytes(&reserve.address)? != compiled_reserve_tree(&denomination)? {
        return Err(anyhow!(
            "address {} is not the {} reserve contract compiled into this client",
            reserve.address,
            denomination.denomination_id()
        ));
    }
    if reserve.value != request.erg_amount {
        return Err(anyhow!(
            "reserve value is {} nanoERG, requested {}",
            reserve.value,
            request.erg_amount
        ));
    }

    // The reserve NFT first, then the collateral token: the order basis-token.es reads them in.
    let mut expected_assets = vec![(request.nft_id.as_str(), 1)];
    if !request.token_id.is_empty() {
        expected_assets.push((request.token_id.as_str(), request.token_amount));
    }
    let assets_match = reserve.assets.len() == expected_assets.len()
        && reserve
            .assets
            .iter()
            .zip(&expected_assets)
            .all(|(a, (id, amount))| a.token_id.eq_ignore_ascii_case(id) && a.amount == *amount);
    if !assets_match {
        return Err(anyhow!(
            "reserve assets {:?} differ from the requested {:?}",
            reserve.assets,
            expected_assets
        ));
    }

    let registers = &reserve.registers;
    if let Some(other) = registers
        .keys()
        .find(|k| !matches!(k.as_str(), "R4" | "R5" | "R6" | "R7"))
    {
        return Err(anyhow!("unexpected register {} in reserve payload", other));
    }
    let register = |id: &str| -> Result<Constant> {
        let value = registers
            .get(id)
            .ok_or_else(|| anyhow!("register {} missing from reserve payload", id))?;
        parse_constant(value).map_err(|e| anyhow!("register {}: {}", id, e))
    };
    let owner_pk: [u8; 33] = decode_array(&request.owner_pubkey, "owner public key")?;
    if register("R4")? != Constant::from(parse_point(&owner_pk)?) {
        return Err(anyhow!("R4 is not the owner key {}", request.owner_pubkey));
    }
    if register("R5")? != parse_constant(EMPTY_RESERVE_TREE_R5)? {
        return Err(anyhow!("R5 is not the empty reserve tree"));
    }
    let tracker_nft_id = register("R6")?
        .try_extract_into::<Vec<u8>>()
        .map_err(|_| anyhow!("R6 is not a Coll[Byte]"))?;
    if tracker_nft_id.len() != 32 {
        return Err(anyhow!(
            "R6 tracker NFT id is {} bytes, expected 32",
            tracker_nft_id.len()
        ));
    }
    if let Some(expected) = expected_tracker_nft_id {
        if !hex::encode(&tracker_nft_id).eq_ignore_ascii_case(expected) {
            return Err(anyhow!(
                "R6 tracker NFT id {} is not the expected {}",
                hex::encode(&tracker_nft_id),
                expected
            ));
        }
    }
    if registers.contains_key("R7") && register("R7")? != Constant::from(0i64) {
        return Err(anyhow!(
            "R7 must be absent or 0 for a new reserve (no pending refund)"
        ));
    }
    Ok(())
}

/// Check that `reserve` is `issuer_pk`'s reserve box under the contract compiled into this
/// client for `denomination`.
///
/// Redemptions sign the reserve input with the creditor's key. If the tracker named one of the
/// creditor's own P2PK boxes instead, that proof would spend it and its value would go wherever
/// the outputs say.
pub fn verify_reserve_input(
    reserve: &ErgoBox,
    issuer_pk: &[u8; 33],
    denomination: &DenomKey,
) -> Result<()> {
    if tree_bytes(&reserve.ergo_tree)? != compiled_reserve_tree(denomination)? {
        return Err(anyhow!(
            "box {} is not a reserve under the {} Basis contract compiled into this client",
            reserve.box_id(),
            denomination.denomination_id()
        ));
    }
    let owner = reserve
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten();
    if owner != Some(Constant::from(parse_point(issuer_pk)?)) {
        return Err(anyhow!(
            "reserve box {} is not owned by issuer {} (R4)",
            reserve.box_id(),
            hex::encode(issuer_pk)
        ));
    }
    Ok(())
}

/// Check a tracker-built redemption (`POST /redemption/build`) before the creditor signs input 0.
///
/// A `proveDlog(recipient)` proof covers the whole transaction and also satisfies every other
/// input guarded by the recipient's P2PK script, so it is only produced for a transaction that:
/// - describes its boxes truthfully: `input_boxes[i]` and `data_boxes[i]` hash to the ids in
///   `unsigned` (the prover evaluates the boxes it is given, the node the boxes the ids name);
/// - spends the issuer's reserve at input 0 ([`verify_reserve_input`]) and no box with the
///   recipient's P2PK script at any other input;
/// - releases exactly `amount` from the reserve (context var #0 = redeem into output 0), so the
///   creditor's claim is not used up beyond what they are paid;
/// - pays the recipient's P2PK at least `amount` (nanoERG, or the collateral token for a token
///   note) in one output.
pub fn verify_redemption_tx(
    unsigned: &UnsignedTransaction,
    input_boxes: &[ErgoBox],
    data_boxes: &[ErgoBox],
    issuer_pk: &[u8; 33],
    recipient_pk: &[u8; 33],
    denomination: &DenomKey,
    amount: u64,
) -> Result<()> {
    let inputs = unsigned.inputs.as_slice();
    let input_ids: Vec<BoxId> = inputs.iter().map(|i| i.box_id).collect();
    let data_ids: Vec<BoxId> = unsigned
        .data_inputs
        .as_ref()
        .map(|d| d.iter().map(|i| i.box_id).collect())
        .unwrap_or_default();
    check_box_ids("input", &input_ids, input_boxes)?;
    check_box_ids("data input", &data_ids, data_boxes)?;

    // A transaction has at least one input, and the boxes match the inputs one to one.
    let reserve = &input_boxes[0];
    verify_reserve_input(reserve, issuer_pk, denomination)?;
    let recipient_tree = tree_bytes(&p2pk_tree(recipient_pk)?)?;
    // Every other input must be a plain P2PK box of another key. The recipient's proof is valid
    // for every input whose script reduces to proveDlog(recipient), not only boxes at the
    // recipient's P2PK address: another Basis reserve redeeming one of the recipient's notes
    // (the redeem path ends in `proveDlog(receiver)` and does not say where the funds go), the
    // recipient's own reserve completing a refund, or a differently encoded or conditional
    // script. The tracker's fee inputs are plain P2PK boxes of its wallet.
    let recipient_point = parse_point(recipient_pk)?;
    for (i, input) in input_boxes.iter().enumerate().skip(1) {
        match Address::recreate_from_ergo_tree(&input.ergo_tree) {
            Ok(Address::P2Pk(owner)) if *owner.h != recipient_point => {}
            Ok(Address::P2Pk(_)) => {
                return Err(anyhow!(
                    "input {} is a box of the recipient's own P2PK; the reserve proof would \
                     spend it",
                    i
                ))
            }
            _ => {
                return Err(anyhow!(
                    "input {} is not a plain P2PK box; the reserve proof could satisfy its script",
                    i
                ))
            }
        }
    }

    // Context var #0 is `action * 10 + reserve output index`.
    if inputs[0].extension.values.get(&0u8) != Some(&Constant::from(0i8)) {
        return Err(anyhow!(
            "reserve input is not a redemption into output 0 (context var #0)"
        ));
    }
    let outputs = unsigned.output_candidates.as_slice();
    let paid = |o: &ErgoBoxCandidate| match denomination {
        DenomKey::Erg => *o.value.as_u64(),
        DenomKey::Token(id) => token_amount(&o.tokens, id),
    };
    let released = match denomination {
        DenomKey::Erg => reserve
            .value
            .as_u64()
            .checked_sub(*outputs[0].value.as_u64()),
        DenomKey::Token(id) => {
            token_amount(&reserve.tokens, id).checked_sub(token_amount(&outputs[0].tokens, id))
        }
    };
    if released != Some(amount) {
        return Err(anyhow!(
            "the reserve releases {:?} instead of the requested {}",
            released,
            amount
        ));
    }
    let pays_recipient = outputs.iter().any(|o| {
        tree_bytes(&o.ergo_tree).ok().as_ref() == Some(&recipient_tree) && paid(o) >= amount
    });
    if !pays_recipient {
        return Err(anyhow!(
            "no output pays the recipient {} at least {} {}",
            hex::encode(recipient_pk),
            amount,
            denomination.denomination_id()
        ));
    }
    Ok(())
}

/// Issuer signature for redemption context var #2 over `message`
/// (`debtKey || totalDebt || timestamp || trackerNftId`).
///
/// This is the stored note's signature: the issuer signed this exact message when the note
/// reached `totalDebt` at `timestamp`. It is verified here first, so a mismatch fails with a
/// clear error instead of on-chain. The client never signs a fresh one, even when the issuer's
/// key is local: `totalDebt` comes from the tracker, and an issuer signature over an inflated
/// total is a valid note for that total, and lets the reserve be drained up to it.
pub fn redemption_issuer_signature(
    issuer_pk: &[u8; 33],
    note_signature_hex: &str,
    message: &[u8; 80],
) -> Result<[u8; 65]> {
    let signature: [u8; 65] = decode_array(note_signature_hex, "stored note signature")?;
    basis_offchain::schnorr::schnorr_verify(&signature, message, issuer_pk).map_err(|_| {
        anyhow!(
            "note not yet committed at this amount: the stored issuer signature does not cover \
             the redemption's total debt and timestamp"
        )
    })?;
    Ok(signature)
}

/// ErgoTree bytes of the reserve contract compiled into this client for `denomination`.
pub fn compiled_reserve_tree(denomination: &DenomKey) -> Result<Vec<u8>> {
    let p2s = match denomination {
        DenomKey::Erg => get_basis_reserve_contract_p2s(),
        DenomKey::Token(_) => get_basis_token_reserve_contract_p2s(),
    }
    .map_err(|e| anyhow!("compiled reserve contract unavailable: {}", e))?;
    address_tree_bytes(&p2s)
}

fn check_box_ids(kind: &str, ids: &[BoxId], boxes: &[ErgoBox]) -> Result<()> {
    if ids.len() != boxes.len() {
        return Err(anyhow!(
            "{} {} boxes supplied for {} {}s",
            boxes.len(),
            kind,
            ids.len(),
            kind
        ));
    }
    for (i, (id, b)) in ids.iter().zip(boxes).enumerate() {
        if b.box_id() != *id {
            return Err(anyhow!(
                "{} {} is box {}, but the supplied box is {}",
                kind,
                i,
                id,
                b.box_id()
            ));
        }
    }
    Ok(())
}

/// Amount of token `id` in `tokens` (0 when absent).
fn token_amount(tokens: &Option<BoxTokens>, id: &[u8; 32]) -> u64 {
    tokens
        .iter()
        .flatten()
        .find(|t| t.token_id.as_ref() == id.as_slice())
        .map(|t| *t.amount.as_u64())
        .unwrap_or(0)
}

/// ErgoTree bytes behind an address of either network.
fn address_tree_bytes(address: &str) -> Result<Vec<u8>> {
    let tree = AddressEncoder::unchecked_parse_address_from_str(address)
        .map_err(|e| anyhow!("invalid address '{}': {}", address, e))?
        .script()
        .map_err(|e| anyhow!("no script for address '{}': {}", address, e))?;
    tree_bytes(&tree)
}

fn p2pk_tree(pubkey: &[u8; 33]) -> Result<ErgoTree> {
    Address::P2Pk(ProveDlog::new(parse_point(pubkey)?))
        .script()
        .map_err(|e| anyhow!("P2PK script: {}", e))
}

fn tree_bytes(tree: &ErgoTree) -> Result<Vec<u8>> {
    tree.sigma_serialize_bytes()
        .map_err(|e| anyhow!("ErgoTree serialization failed: {}", e))
}

fn parse_point(pubkey: &[u8; 33]) -> Result<EcPoint> {
    EcPoint::sigma_parse_bytes(pubkey).map_err(|e| anyhow!("invalid public key: {}", e))
}

/// Parse a hex-encoded serialized constant, as the node reads register values.
fn parse_constant(value_hex: &str) -> Result<Constant> {
    let bytes = hex::decode(value_hex).map_err(|e| anyhow!("invalid hex: {}", e))?;
    Constant::sigma_parse_bytes(&bytes).map_err(|e| anyhow!("invalid constant: {}", e))
}

fn decode_array<const N: usize>(value_hex: &str, label: &str) -> Result<[u8; N]> {
    hex::decode(value_hex)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| anyhow!("{} must be {} bytes of hex", label, N))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;
    use crate::api::{Asset, ReservePaymentRequest};
    use basis_offchain::signing::redemption_signing_message;
    use ergo_lib::chain::transaction::{DataInput, TxId, UnsignedInput};
    use ergo_lib::ergo_chain_types::Digest32;
    use ergo_lib::ergotree_ir::chain::context_extension::ContextExtension;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters;
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId};
    use std::collections::HashMap;

    const ISSUER_PK: &str = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
    const RECIPIENT_PK: &str = "02725e8878d5198ca7f5853dddf35560ddab05ab0a26adae7e664b84162c9962e5";
    /// Stands in for the (dishonest) tracker's own key.
    const TRACKER_PK: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const RESERVE_NFT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TRACKER_NFT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TOKEN_ID: &str = "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669";

    fn pk(hex_key: &str) -> [u8; 33] {
        decode_array(hex_key, "test key").unwrap()
    }

    fn token_denom() -> DenomKey {
        DenomKey::Token(decode_array(TOKEN_ID, "test token").unwrap())
    }

    // ------------------------------------------------------------------
    // Reserve-creation payload (POST /reserves/create)
    // ------------------------------------------------------------------

    fn reserve_request(token: bool) -> CreateReserveRequest {
        CreateReserveRequest {
            nft_id: RESERVE_NFT.to_string(),
            owner_pubkey: ISSUER_PK.to_string(),
            erg_amount: 1_000_000_000,
            token_amount: if token { 5_000 } else { 0 },
            token_id: if token {
                TOKEN_ID.to_string()
            } else {
                String::new()
            },
        }
    }

    /// The payload basis_server's `/reserves/create` handler returns for `request`.
    fn honest_payload(request: &CreateReserveRequest) -> ReserveCreationResponse {
        let token = !request.token_id.is_empty();
        let address = if token {
            get_basis_token_reserve_contract_p2s()
        } else {
            get_basis_reserve_contract_p2s()
        }
        .unwrap();
        let mut assets = vec![Asset {
            token_id: request.nft_id.clone(),
            amount: 1,
        }];
        if token {
            assets.push(Asset {
                token_id: request.token_id.clone(),
                amount: request.token_amount,
            });
        }
        let registers = HashMap::from([
            ("R4".to_string(), format!("07{}", request.owner_pubkey)),
            ("R5".to_string(), EMPTY_RESERVE_TREE_R5.to_string()),
            ("R6".to_string(), format!("0e20{}", TRACKER_NFT)),
            ("R7".to_string(), "05000000000000000000".to_string()),
        ]);
        ReserveCreationResponse {
            requests: vec![ReservePaymentRequest {
                address,
                value: request.erg_amount,
                assets,
                registers,
            }],
            fee: 1_000_000,
            change_address: basis_offchain::ergo_tx::pubkey_to_address(ISSUER_PK).unwrap(),
        }
    }

    fn reserve_err(request: &CreateReserveRequest, payload: &ReserveCreationResponse) -> String {
        verify_reserve_payload(request, payload, None)
            .expect_err("payload must be rejected")
            .to_string()
    }

    #[test]
    fn reserve_payload_genuine_erg_and_token_pass() {
        for token in [false, true] {
            let request = reserve_request(token);
            let payload = honest_payload(&request);
            verify_reserve_payload(&request, &payload, None).unwrap();
            verify_reserve_payload(&request, &payload, Some(TRACKER_NFT)).unwrap();
        }
        // R7 may also be left out entirely.
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests[0].registers.remove("R7");
        verify_reserve_payload(&request, &payload, None).unwrap();
    }

    #[test]
    fn reserve_payload_rejects_swapped_owner_key() {
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests[0]
            .registers
            .insert("R4".to_string(), format!("07{}", TRACKER_PK));
        assert!(reserve_err(&request, &payload).contains("R4"));
    }

    #[test]
    fn reserve_payload_rejects_extra_request() {
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests.push(ReservePaymentRequest {
            address: basis_offchain::ergo_tx::pubkey_to_address(TRACKER_PK).unwrap(),
            value: 5_000_000_000,
            assets: vec![],
            registers: HashMap::new(),
        });
        assert!(reserve_err(&request, &payload).contains("exactly one"));
    }

    #[test]
    fn reserve_payload_rejects_wrong_address() {
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests[0].address =
            basis_offchain::ergo_tx::pubkey_to_address(TRACKER_PK).unwrap();
        assert!(reserve_err(&request, &payload).contains("not the erg reserve contract"));
        // The token contract is not the reserve an ERG request asked for either.
        payload.requests[0].address = get_basis_token_reserve_contract_p2s().unwrap();
        assert!(reserve_err(&request, &payload).contains("not the erg reserve contract"));
    }

    #[test]
    fn reserve_payload_rejects_extra_or_changed_asset() {
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests[0].assets.push(Asset {
            token_id: TOKEN_ID.to_string(),
            amount: 1_000,
        });
        assert!(reserve_err(&request, &payload).contains("assets"));

        let request = reserve_request(true);
        let mut payload = honest_payload(&request);
        payload.requests[0].assets[1].amount += 1;
        assert!(reserve_err(&request, &payload).contains("assets"));
    }

    #[test]
    fn reserve_payload_rejects_wrong_value() {
        let request = reserve_request(false);
        let mut payload = honest_payload(&request);
        payload.requests[0].value = request.erg_amount * 10;
        assert!(reserve_err(&request, &payload).contains("value"));
    }

    #[test]
    fn reserve_payload_rejects_bad_tree_tracker_or_refund_registers() {
        let request = reserve_request(false);
        let honest = honest_payload(&request);
        let with = |id: &str, value: &str| {
            let mut payload = honest.clone();
            payload.requests[0]
                .registers
                .insert(id.to_string(), value.to_string());
            payload
        };

        // Non-empty reserve tree.
        let r5 = format!("64{}032000", "11".repeat(33));
        assert!(reserve_err(&request, &with("R5", &r5)).contains("R5"));
        // Tracker NFT id of the wrong length, or not the one the caller expects.
        assert!(reserve_err(&request, &with("R6", "0e0411223344")).contains("R6"));
        let err = verify_reserve_payload(&request, &honest, Some(&"cc".repeat(32)))
            .unwrap_err()
            .to_string();
        assert!(err.contains("R6"));
        // A pending refund on a brand-new reserve.
        assert!(reserve_err(&request, &with("R7", "050a")).contains("R7"));
        // Anything beyond R4..R7.
        assert!(reserve_err(&request, &with("R8", "0500")).contains("R8"));
    }

    // ------------------------------------------------------------------
    // Tracker-built redemption (POST /redemption/build)
    // ------------------------------------------------------------------

    const RESERVE_COLLATERAL: u64 = 1_000_000_000;
    const AMOUNT: u64 = 50_000_000;

    fn value(v: u64) -> BoxValue {
        BoxValue::try_from(v).unwrap()
    }

    fn tokens(list: &[(&str, u64)]) -> Option<BoxTokens> {
        let tokens: Vec<Token> = list
            .iter()
            .map(|(id, amount)| Token {
                token_id: TokenId::from(Digest32::from(
                    decode_array::<32>(id, "test token").unwrap(),
                )),
                amount: TokenAmount::try_from(*amount).unwrap(),
            })
            .collect();
        BoxTokens::from_vec(tokens).ok()
    }

    fn p2pk(owner: &str) -> ErgoTree {
        p2pk_tree(&pk(owner)).unwrap()
    }

    fn candidate(
        v: u64,
        ergo_tree: ErgoTree,
        tokens: Option<BoxTokens>,
        additional_registers: NonMandatoryRegisters,
    ) -> ErgoBoxCandidate {
        ErgoBoxCandidate {
            value: value(v),
            ergo_tree,
            tokens,
            additional_registers,
            creation_height: 100,
        }
    }

    /// An on-chain box; `index` keeps otherwise identical boxes distinct.
    fn on_chain(c: ErgoBoxCandidate, index: u16) -> ErgoBox {
        ErgoBox::from_box_candidate(&c, TxId::zero(), index).unwrap()
    }

    fn reserve_registers(owner: &str) -> NonMandatoryRegisters {
        NonMandatoryRegisters::new([
            (
                NonMandatoryRegisterId::R4,
                Constant::from(parse_point(&pk(owner)).unwrap()),
            ),
            (
                NonMandatoryRegisterId::R5,
                parse_constant(EMPTY_RESERVE_TREE_R5).unwrap(),
            ),
            (
                NonMandatoryRegisterId::R6,
                Constant::from(hex::decode(TRACKER_NFT).unwrap()),
            ),
            (NonMandatoryRegisterId::R7, Constant::from(0i64)),
        ])
        .unwrap()
    }

    /// The issuer's reserve; a token reserve also holds `RESERVE_COLLATERAL` collateral tokens.
    fn reserve_box(denomination: &DenomKey, owner: &str) -> ErgoBox {
        let assets = match denomination {
            DenomKey::Erg => tokens(&[(RESERVE_NFT, 1)]),
            DenomKey::Token(_) => tokens(&[(RESERVE_NFT, 1), (TOKEN_ID, RESERVE_COLLATERAL)]),
        };
        let tree = ErgoTree::sigma_parse_bytes(&compiled_reserve_tree(denomination).unwrap());
        on_chain(
            candidate(
                RESERVE_COLLATERAL,
                tree.unwrap(),
                assets,
                reserve_registers(owner),
            ),
            0,
        )
    }

    /// A redemption shaped like basis_server's `/redemption/build`.
    struct Case {
        denomination: DenomKey,
        inputs: Vec<ErgoBox>,
        data: Vec<ErgoBox>,
        outputs: Vec<ErgoBoxCandidate>,
    }

    impl Case {
        fn honest(denomination: DenomKey) -> Self {
            let reserve = reserve_box(&denomination, ISSUER_PK);
            let fee_input = on_chain(
                candidate(
                    10_000_000,
                    p2pk(TRACKER_PK),
                    None,
                    NonMandatoryRegisters::empty(),
                ),
                1,
            );
            let tracker_box = on_chain(
                candidate(
                    1_000_000,
                    p2pk(TRACKER_PK),
                    tokens(&[(TRACKER_NFT, 1)]),
                    NonMandatoryRegisters::empty(),
                ),
                2,
            );
            let (reserve_out, recipient_out) = match denomination {
                DenomKey::Erg => (
                    candidate(
                        RESERVE_COLLATERAL - AMOUNT,
                        reserve.ergo_tree.clone(),
                        reserve.tokens.clone(),
                        reserve.additional_registers.clone(),
                    ),
                    candidate(
                        AMOUNT,
                        p2pk(RECIPIENT_PK),
                        None,
                        NonMandatoryRegisters::empty(),
                    ),
                ),
                DenomKey::Token(_) => (
                    candidate(
                        RESERVE_COLLATERAL,
                        reserve.ergo_tree.clone(),
                        tokens(&[(RESERVE_NFT, 1), (TOKEN_ID, RESERVE_COLLATERAL - AMOUNT)]),
                        reserve.additional_registers.clone(),
                    ),
                    candidate(
                        1_000_000,
                        p2pk(RECIPIENT_PK),
                        tokens(&[(TOKEN_ID, AMOUNT)]),
                        NonMandatoryRegisters::empty(),
                    ),
                ),
            };
            let change = candidate(
                8_000_000,
                p2pk(TRACKER_PK),
                None,
                NonMandatoryRegisters::empty(),
            );
            Case {
                denomination,
                inputs: vec![reserve, fee_input],
                data: vec![tracker_box],
                outputs: vec![reserve_out, recipient_out, change],
            }
        }

        fn unsigned(&self) -> UnsignedTransaction {
            // Context var #0 as the server writes it (`serialize_ergo_byte(0)`); the other
            // reserve-input vars are not checked here.
            let redeem: ContextExtension =
                serde_json::from_value(serde_json::json!({ "0": "0200" })).unwrap();
            let inputs = self
                .inputs
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let extension = if i == 0 {
                        redeem.clone()
                    } else {
                        ContextExtension::empty()
                    };
                    UnsignedInput::new(b.box_id(), extension)
                })
                .collect();
            let data = self
                .data
                .iter()
                .map(|b| DataInput::from(b.box_id()))
                .collect();
            UnsignedTransaction::new_from_vec(inputs, data, self.outputs.clone()).unwrap()
        }

        fn verify(&self) -> Result<()> {
            verify_redemption_tx(
                &self.unsigned(),
                &self.inputs,
                &self.data,
                &pk(ISSUER_PK),
                &pk(RECIPIENT_PK),
                &self.denomination,
                AMOUNT,
            )
        }

        fn err(&self) -> String {
            self.verify()
                .expect_err("redemption must be rejected")
                .to_string()
        }
    }

    #[test]
    fn redemption_honest_erg_and_token_builds_pass() {
        Case::honest(DenomKey::Erg).verify().unwrap();
        Case::honest(token_denom()).verify().unwrap();
    }

    #[test]
    fn redemption_rejects_misdescribed_boxes() {
        let case = Case::honest(DenomKey::Erg);
        let unsigned = case.unsigned();
        let other = on_chain(
            candidate(
                1_000_000,
                p2pk(RECIPIENT_PK),
                None,
                NonMandatoryRegisters::empty(),
            ),
            7,
        );
        let check = |inputs: &[ErgoBox], data: &[ErgoBox]| {
            verify_redemption_tx(
                &unsigned,
                inputs,
                data,
                &pk(ISSUER_PK),
                &pk(RECIPIENT_PK),
                &DenomKey::Erg,
                AMOUNT,
            )
            .expect_err("box list must be rejected")
            .to_string()
        };

        // The reserve's bytes presented for a different input id (and vice versa).
        let swapped = vec![case.inputs[1].clone(), case.inputs[0].clone()];
        assert!(check(&swapped, &case.data).contains("input 0 is box"));
        assert!(check(&case.inputs[..1], &case.data).contains("boxes supplied"));
        assert!(check(&case.inputs, &[other]).contains("data input 0 is box"));
    }

    #[test]
    fn redemption_rejects_recipient_box_at_input_0() {
        // The attack: the creditor's own box in the reserve's place, outputs paying the tracker.
        let mut case = Case::honest(DenomKey::Erg);
        case.inputs[0] = on_chain(
            candidate(
                RESERVE_COLLATERAL,
                p2pk(RECIPIENT_PK),
                None,
                NonMandatoryRegisters::empty(),
            ),
            0,
        );
        case.outputs[0] = candidate(
            RESERVE_COLLATERAL - AMOUNT,
            p2pk(TRACKER_PK),
            None,
            NonMandatoryRegisters::empty(),
        );
        assert!(case.err().contains("not a reserve under the erg"));
    }

    #[test]
    fn redemption_rejects_reserve_of_another_owner_or_denomination() {
        let mut case = Case::honest(DenomKey::Erg);
        case.inputs[0] = reserve_box(&DenomKey::Erg, TRACKER_PK);
        assert!(case.err().contains("not owned by issuer"));

        // A token note must be redeemed against the token contract.
        let mut case = Case::honest(token_denom());
        case.inputs[0] = reserve_box(&DenomKey::Erg, ISSUER_PK);
        assert!(case.err().contains("not a reserve under the token:"));
    }

    #[test]
    fn redemption_rejects_recipient_box_at_other_input() {
        let mut case = Case::honest(DenomKey::Erg);
        case.inputs.push(on_chain(
            candidate(
                5_000_000_000,
                p2pk(RECIPIENT_PK),
                None,
                NonMandatoryRegisters::empty(),
            ),
            3,
        ));
        case.outputs[2] = candidate(
            5_008_000_000,
            p2pk(TRACKER_PK),
            None,
            NonMandatoryRegisters::empty(),
        );
        assert!(case.err().contains("input 2 is a box of the recipient"));
    }

    /// A reserve redeems into whatever outputs the transaction has once the receiver signs
    /// (`... && proveDlog(receiver)`), so another issuer's reserve, or the issuer's reserve in
    /// another denomination, at a later input would let the tracker redeem the recipient's other
    /// notes to itself with the same proof.
    #[test]
    fn redemption_rejects_other_reserves_at_other_inputs() {
        for extra in [
            reserve_box(&DenomKey::Erg, TRACKER_PK),
            reserve_box(&token_denom(), ISSUER_PK),
            reserve_box(&DenomKey::Erg, RECIPIENT_PK),
        ] {
            let mut case = Case::honest(DenomKey::Erg);
            case.inputs.push(on_chain(
                candidate(
                    *extra.value.as_u64(),
                    extra.ergo_tree.clone(),
                    extra.tokens.clone(),
                    extra.additional_registers.clone(),
                ),
                3,
            ));
            assert!(
                case.err().contains("input 2 is not a plain P2PK box"),
                "{}",
                case.err()
            );
        }
    }

    /// The recipient's key under another encoding of the same proposition (constant
    /// segregation) is still the recipient's box.
    #[test]
    fn redemption_rejects_recipient_key_under_another_tree_encoding() {
        use ergo_lib::ergotree_ir::ergo_tree::ErgoTreeHeader;
        use ergo_lib::ergotree_ir::mir::expr::Expr;
        use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::SigmaProp;

        let proposition = Expr::Const(Constant::from(SigmaProp::from(ProveDlog::new(
            parse_point(&pk(RECIPIENT_PK)).unwrap(),
        ))));
        let segregated = ErgoTree::new(ErgoTreeHeader::v0(true), &proposition).unwrap();
        assert_ne!(
            tree_bytes(&segregated).unwrap(),
            tree_bytes(&p2pk(RECIPIENT_PK)).unwrap()
        );

        let mut case = Case::honest(DenomKey::Erg);
        case.inputs.push(on_chain(
            candidate(
                5_000_000_000,
                segregated,
                None,
                NonMandatoryRegisters::empty(),
            ),
            3,
        ));
        assert!(case.err().contains("input 2 is a box of the recipient"));
    }

    #[test]
    fn redemption_rejects_outputs_not_paying_recipient() {
        // Redeemed value routed to the tracker.
        let mut case = Case::honest(DenomKey::Erg);
        case.outputs[1].ergo_tree = p2pk(TRACKER_PK);
        assert!(case.err().contains("no output pays the recipient"));

        // Recipient underpaid.
        let mut case = Case::honest(DenomKey::Erg);
        case.outputs[1].value = value(AMOUNT - 1);
        case.outputs[2].value = value(8_000_001);
        assert!(case.err().contains("no output pays the recipient"));

        // Token note: ERG to the recipient, the collateral tokens to the tracker.
        let mut case = Case::honest(token_denom());
        case.outputs[1] = candidate(
            AMOUNT,
            p2pk(RECIPIENT_PK),
            None,
            NonMandatoryRegisters::empty(),
        );
        case.outputs[2].tokens = tokens(&[(TOKEN_ID, AMOUNT)]);
        assert!(case.err().contains("no output pays the recipient"));
    }

    #[test]
    fn redemption_rejects_releasing_more_than_requested() {
        // Twice the amount leaves the reserve (using up more of the creditor's claim); the
        // recipient is paid as asked and the rest goes to the tracker.
        let mut case = Case::honest(DenomKey::Erg);
        case.outputs[0].value = value(RESERVE_COLLATERAL - 2 * AMOUNT);
        case.outputs[2].value = value(8_000_000 + AMOUNT);
        assert!(case.err().contains("releases"));

        let mut case = Case::honest(token_denom());
        case.outputs[0].tokens = tokens(&[
            (RESERVE_NFT, 1),
            (TOKEN_ID, RESERVE_COLLATERAL - 2 * AMOUNT),
        ]);
        case.outputs[2].tokens = tokens(&[(TOKEN_ID, AMOUNT)]);
        assert!(case.err().contains("releases"));
    }

    // ------------------------------------------------------------------
    // Issuer signature (context var #2)
    // ------------------------------------------------------------------

    fn redemption_message(issuer: &Account, total_debt: u64) -> [u8; 80] {
        let tracker_nft: [u8; 32] = decode_array(TRACKER_NFT, "test nft").unwrap();
        redemption_signing_message(
            &pk(&issuer.get_pubkey_hex()),
            &pk(RECIPIENT_PK),
            &DenomKey::Erg,
            total_debt,
            1_783_612_740_170,
            &tracker_nft,
        )
    }

    #[test]
    fn issuer_signature_reuses_the_verified_note_signature() {
        let issuer = Account::new("issuer".to_string()).unwrap();
        let issuer_pk = pk(&issuer.get_pubkey_hex());
        let note_signature = issuer
            .sign_message(&redemption_message(&issuer, 100))
            .unwrap();

        // The creditor redeems at the note's committed amount: the stored signature is it.
        let reused = redemption_issuer_signature(
            &issuer_pk,
            &hex::encode(note_signature),
            &redemption_message(&issuer, 100),
        )
        .unwrap();
        assert_eq!(reused, note_signature);

        // A total debt the note was not signed at cannot use it. Nothing is signed instead, so a
        // tracker reporting an inflated total gets no issuer signature over it.
        let err = redemption_issuer_signature(
            &issuer_pk,
            &hex::encode(note_signature),
            &redemption_message(&issuer, 150),
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("note not yet committed at this amount"));
        assert!(redemption_issuer_signature(&issuer_pk, "zz", &[0u8; 80]).is_err());
    }
}
