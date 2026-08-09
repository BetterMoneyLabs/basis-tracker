//! Fail-closed Basis v2 redemption manifest construction and validation.
//!
//! This module prepares unsigned bytes only. It has no signing, submission,
//! broadcast, or activation path. Confirmed input authority is intentionally
//! constructible only by this crate so a future scanner/reconciler must own the
//! chain-observation boundary before the builder can become reachable.
//!
//! ```compile_fail
//! use basis_store::basis_v2_builder::VerifiedSigningTipV2;
//! let _ = VerifiedSigningTipV2 { block_id: [0; 32], height: 1 };
//! ```

use crate::basis_v2_state::{
    ReserveRedeemedStoreV2, ReserveRedemptionWitnessV2, TrackerClaimStoreV2, TrackerClaimWitnessV2,
    V2StateError,
};
use crate::contract_compiler::BasisV2ContractKind;
use basis_core::basis_v2::{
    verify_basis_v2_signature, ClaimDomainV2, ClaimV2, RedeemedStateV2, ReserveAssetV2,
};
use basis_core::types::Signature;
use basis_offchain::ergo_tx::{
    scala_context_extension_order, serialize_coll_bytes, serialize_ergo_byte, serialize_ergo_long,
};
use basis_trees::{ReserveAvlTree, TrackerAvlTree};
use ergo_lib::ergo_chain_types::ADDigest;
use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::mir::avl_tree_data::{AvlTreeData, AvlTreeFlags};
use ergo_lib::ergotree_ir::mir::constant::{Constant, TryExtractInto};
use ergo_lib::ergotree_ir::serialization::{SigmaSerializable, SigmaSerializationError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

pub const BASIS_V2_MANIFEST_SCHEMA: &str = "basis-v2-redemption-manifest/1";
pub const BASIS_V2_MIN_BOX_VALUE: u64 = 1_000_000;
pub const BASIS_V2_MAX_FUNDING_INPUTS: usize = 16;
pub const BASIS_V2_MAX_PROOF_BYTES: usize = 64 * 1024;

/// Miner-fee proposition used by the canonical Scala transaction shape.
pub const BASIS_V2_FEE_ERGO_TREE: &str = "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum V2BuilderError {
    #[error("v2 state evidence failed: {0}")]
    State(String),
    #[error("invalid exact box bytes: {0}")]
    BoxBytes(String),
    #[error("v2 manifest invariant failed: {0}")]
    Invariant(String),
}

impl From<V2StateError> for V2BuilderError {
    fn from(value: V2StateError) -> Self {
        Self::State(value.to_string())
    }
}

impl From<SigmaSerializationError> for V2BuilderError {
    fn from(value: SigmaSerializationError) -> Self {
        Self::BoxBytes(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenManifestV2 {
    token_id: String,
    amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactBoxManifestV2 {
    box_id: String,
    value: u64,
    ergo_tree: String,
    tokens: Vec<TokenManifestV2>,
    registers: Vec<String>,
    creation_height: u32,
    raw_sigma_hex: String,
}

impl ExactBoxManifestV2 {
    pub fn box_id(&self) -> &str {
        &self.box_id
    }

    pub fn raw_sigma_hex(&self) -> &str {
        &self.raw_sigma_hex
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainObservationManifestV2 {
    inclusion_block_id: String,
    inclusion_height: u32,
    tip_block_id: String,
    tip_height: u32,
    confirmations: u32,
}

/// Opaque evidence supplied only by a confirmed-chain scanner in this crate.
#[derive(Debug, Clone)]
pub struct ConfirmedChainBoxV2 {
    exact: ExactBoxManifestV2,
    observation: ChainObservationManifestV2,
}

impl ConfirmedChainBoxV2 {
    /// Test fixture only. Production deliberately has no constructor until a
    /// sealed header-ancestry reconciler can supply this opaque authority.
    #[cfg(test)]
    fn from_test_observation(
        raw_sigma_hex: &str,
        inclusion_block_id: [u8; 32],
        inclusion_height: u32,
        tip_block_id: [u8; 32],
        tip_height: u32,
    ) -> Result<Self, V2BuilderError> {
        let confirmations = tip_height
            .checked_sub(inclusion_height)
            .and_then(|distance| distance.checked_add(1))
            .ok_or_else(|| invariant("box inclusion height is ahead of the confirmed tip"))?;
        Ok(Self {
            exact: parse_exact_box(raw_sigma_hex)?,
            observation: ChainObservationManifestV2 {
                inclusion_block_id: hex::encode(inclusion_block_id),
                inclusion_height,
                tip_block_id: hex::encode(tip_block_id),
                tip_height,
                confirmations,
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct V2RedemptionBuildRequest {
    claim: ClaimV2,
    amount: u64,
    fee: u64,
    current_height: u32,
    minimum_confirmations: u32,
    reserve: ConfirmedChainBoxV2,
    funding: Vec<ConfirmedChainBoxV2>,
    tracker: Option<ConfirmedChainBoxV2>,
    tracker_signature: Option<Signature>,
}

impl V2RedemptionBuildRequest {
    /// Scanner/state-owned assembly boundary. Kept crate-private until a
    /// confirmed v2 reconciler is wired; this prevents implicit activation.
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    fn from_test_confirmed_state(
        claim: ClaimV2,
        amount: u64,
        fee: u64,
        current_height: u32,
        minimum_confirmations: u32,
        reserve: ConfirmedChainBoxV2,
        funding: Vec<ConfirmedChainBoxV2>,
        tracker: Option<ConfirmedChainBoxV2>,
        tracker_signature: Option<Signature>,
    ) -> Self {
        Self {
            claim,
            amount,
            fee,
            current_height,
            minimum_confirmations,
            reserve,
            funding,
            tracker,
            tracker_signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReserveAssetManifestV2 {
    Erg,
    Token { token_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimManifestV2 {
    reserve_nft_id: String,
    tracker_nft_id: String,
    owner_pubkey: String,
    receiver_pubkey: String,
    asset: ReserveAssetManifestV2,
    total_debt: u64,
    timestamp: u64,
    owner_signature: String,
}

impl ClaimManifestV2 {
    fn from_claim(claim: &ClaimV2) -> Self {
        let domain = claim.domain();
        Self {
            reserve_nft_id: hex::encode(domain.reserve_nft_id()),
            tracker_nft_id: hex::encode(domain.tracker_nft_id()),
            owner_pubkey: hex::encode(domain.owner_pubkey()),
            receiver_pubkey: hex::encode(domain.receiver_pubkey()),
            asset: match domain.asset() {
                ReserveAssetV2::Erg => ReserveAssetManifestV2::Erg,
                ReserveAssetV2::Token { token_id } => ReserveAssetManifestV2::Token {
                    token_id: hex::encode(token_id),
                },
            },
            total_debt: claim.total_debt(),
            timestamp: claim.timestamp(),
            owner_signature: hex::encode(claim.signature()),
        }
    }

    fn to_claim(&self) -> Result<ClaimV2, V2BuilderError> {
        let reserve_nft_id = decode_array::<32>(&self.reserve_nft_id, "claim reserve NFT")?;
        let tracker_nft_id = decode_array::<32>(&self.tracker_nft_id, "claim tracker NFT")?;
        let owner = decode_array::<33>(&self.owner_pubkey, "claim owner key")?;
        let receiver = decode_array::<33>(&self.receiver_pubkey, "claim receiver key")?;
        let domain = match &self.asset {
            ReserveAssetManifestV2::Erg => {
                ClaimDomainV2::erg(reserve_nft_id, tracker_nft_id, owner, receiver)
            }
            ReserveAssetManifestV2::Token { token_id } => ClaimDomainV2::token(
                reserve_nft_id,
                decode_array::<32>(token_id, "claim reserve token")?,
                tracker_nft_id,
                owner,
                receiver,
            ),
        }
        .map_err(|error| invariant(format!("invalid claim domain: {error}")))?;
        ClaimV2::from_signed(
            domain,
            self.total_debt,
            self.timestamp,
            decode_array::<65>(&self.owner_signature, "claim owner signature")?,
        )
        .map_err(|error| invariant(format!("invalid signed claim: {error}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextVariableManifestV2 {
    index: u8,
    serialized_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputManifestV2 {
    value: u64,
    ergo_tree: String,
    creation_height: u32,
    tokens: Vec<TokenManifestV2>,
    additional_registers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedInputManifestV2 {
    exact_box: ExactBoxManifestV2,
    observation: ChainObservationManifestV2,
}

impl From<&ConfirmedChainBoxV2> for AuthenticatedInputManifestV2 {
    fn from(value: &ConfirmedChainBoxV2) -> Self {
        Self {
            exact_box: value.exact.clone(),
            observation: value.observation.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V2RedemptionManifest {
    schema: String,
    claim: ClaimManifestV2,
    amount: u64,
    fee: u64,
    current_height: u32,
    minimum_confirmations: u32,
    emergency: bool,
    reserve_input: AuthenticatedInputManifestV2,
    funding_inputs: Vec<AuthenticatedInputManifestV2>,
    tracker_data_input: Option<AuthenticatedInputManifestV2>,
    tracker_signature: Option<String>,
    reserve_root: String,
    reserve_prior_state: Option<String>,
    reserve_prior_proof: String,
    reserve_next_state: String,
    reserve_update_proof: String,
    reserve_next_root: String,
    tracker_root: Option<String>,
    tracker_lookup_proof: Option<String>,
    tracker_committed_total_debt: Option<u64>,
    context_extension: Vec<ContextVariableManifestV2>,
    outputs: Vec<OutputManifestV2>,
}

impl V2RedemptionManifest {
    pub fn claim(&self) -> &ClaimManifestV2 {
        &self.claim
    }

    pub const fn amount(&self) -> u64 {
        self.amount
    }

    pub const fn emergency(&self) -> bool {
        self.emergency
    }

    /// Node-style unsigned transaction JSON. This remains unsigned and has no
    /// broadcast behavior; context variables retain Scala map iteration order.
    pub fn unsigned_transaction_json(&self) -> Value {
        let mut reserve_extension = Map::new();
        for variable in &self.context_extension {
            reserve_extension.insert(
                variable.index.to_string(),
                Value::String(variable.serialized_value.clone()),
            );
        }
        let mut inputs = vec![json!({
            "boxId": self.reserve_input.exact_box.box_id,
            "extension": reserve_extension,
        })];
        inputs.extend(
            self.funding_inputs
                .iter()
                .map(|input| json!({ "boxId": input.exact_box.box_id, "extension": {} })),
        );
        let data_inputs: Vec<Value> = self
            .tracker_data_input
            .iter()
            .map(|input| json!({ "boxId": input.exact_box.box_id }))
            .collect();
        let outputs: Vec<Value> = self
            .outputs
            .iter()
            .map(|output| {
                json!({
                    "value": output.value,
                    "ergoTree": output.ergo_tree,
                    "creationHeight": output.creation_height,
                    "assets": output.tokens.iter().map(|token| json!({
                        "tokenId": token.token_id,
                        "amount": token.amount,
                    })).collect::<Vec<_>>(),
                    "additionalRegisters": output.additional_registers,
                })
            })
            .collect();
        json!({ "inputs": inputs, "dataInputs": data_inputs, "outputs": outputs })
    }
}

/// Locally configured user intent against which a remote manifest is checked
/// before any signing callback may be entered.
#[derive(Debug, Clone)]
pub struct V2SigningIntent {
    claim: ClaimV2,
    amount: u64,
    fee: u64,
    signing_tip: VerifiedSigningTipV2,
    minimum_confirmations: u32,
    reserve_box_id: [u8; 32],
    funding_owner_pubkey: [u8; 33],
}

/// Opaque signer-side chain tip. Production deliberately has no constructor:
/// a future local header-ancestry verifier must be integrated here before a v2
/// signing intent can exist.
#[derive(Debug, Clone)]
pub struct VerifiedSigningTipV2 {
    block_id: [u8; 32],
    height: u32,
}

impl VerifiedSigningTipV2 {
    #[cfg(test)]
    fn from_test_tip(block_id: [u8; 32], height: u32) -> Self {
        Self { block_id, height }
    }
}

impl V2SigningIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        claim: ClaimV2,
        amount: u64,
        fee: u64,
        signing_tip: VerifiedSigningTipV2,
        minimum_confirmations: u32,
        reserve_box_id: [u8; 32],
        funding_owner_pubkey: [u8; 33],
    ) -> Result<Self, V2BuilderError> {
        claim
            .verify()
            .map_err(|error| invariant(format!("invalid signing intent claim: {error}")))?;
        if amount == 0 || fee == 0 || minimum_confirmations == 0 {
            return Err(invariant(
                "signing intent amount, fee, and confirmations must be non-zero",
            ));
        }
        Ok(Self {
            claim,
            amount,
            fee,
            signing_tip,
            minimum_confirmations,
            reserve_box_id,
            funding_owner_pubkey,
        })
    }
}

/// Proof that the exact manifest has passed all signer-side validations.
pub struct ValidatedV2RedemptionManifest<'a> {
    manifest: &'a V2RedemptionManifest,
}

impl<'a> ValidatedV2RedemptionManifest<'a> {
    pub fn manifest(&self) -> &'a V2RedemptionManifest {
        self.manifest
    }
}

/// Enter a proof or signing boundary only after the exact remote manifest has
/// passed every signer-side v2 check.
///
/// The callback cannot be called with an unvalidated manifest: the proof type
/// is constructed only by [`validate_v2_redemption_manifest`]. Callers that
/// need a fallible callback can return a `Result` as `T` and flatten it after
/// this validation result has been handled.
pub fn with_validated_v2_redemption_manifest<'a, T>(
    manifest: &'a V2RedemptionManifest,
    intent: &V2SigningIntent,
    callback: impl FnOnce(ValidatedV2RedemptionManifest<'a>) -> T,
) -> Result<T, V2BuilderError> {
    let validated = validate_v2_redemption_manifest(manifest, intent)?;
    Ok(callback(validated))
}

/// Build a v2 manifest from confirmed exact boxes and authoritative BNS2/BRS2
/// roots. No signing material is requested or used.
pub fn build_v2_redemption_manifest(
    request: V2RedemptionBuildRequest,
    reserve_state: &mut ReserveRedeemedStoreV2,
    tracker_state: Option<&mut TrackerClaimStoreV2>,
) -> Result<V2RedemptionManifest, V2BuilderError> {
    request
        .claim
        .verify()
        .map_err(|error| invariant(format!("invalid supplied ClaimV2: {error}")))?;
    validate_scalar_request(
        request.amount,
        request.fee,
        request.minimum_confirmations,
        request.funding.len(),
    )?;
    validate_observation(
        &request.reserve.observation,
        request.current_height,
        request.minimum_confirmations,
        None,
    )?;
    for funding in &request.funding {
        validate_observation(
            &funding.observation,
            request.current_height,
            request.minimum_confirmations,
            None,
        )?;
    }

    let reserve_root = reserve_state.root_digest()?;
    let reserve_witness = reserve_state.redemption_witness(&request.claim, request.amount)?;
    let reserve_view =
        validate_reserve_input(&request.reserve.exact, &request.claim, reserve_root)?;
    let emergency = i64::from(request.current_height) >= reserve_view.emergency_height;

    let (tracker_input, tracker_signature, tracker_root, tracker_witness) = if emergency {
        if request.tracker.is_some()
            || request.tracker_signature.is_some()
            || tracker_state.is_some()
        {
            return Err(invariant(
                "emergency mode is height-derived and must omit tracker-only evidence",
            ));
        }
        (None, None, None, None)
    } else {
        let tracker = request
            .tracker
            .as_ref()
            .ok_or_else(|| invariant("normal mode requires confirmed tracker data input"))?;
        validate_observation(
            &tracker.observation,
            request.current_height,
            request.minimum_confirmations,
            None,
        )?;
        let signature = request
            .tracker_signature
            .ok_or_else(|| invariant("normal mode requires the tracker ClaimV2 signature"))?;
        let state = tracker_state
            .ok_or_else(|| invariant("normal mode requires authoritative BNS2 state"))?;
        let root = state.root_digest()?;
        let witness = state.claim_witness(&request.claim)?;
        validate_tracker_input(&tracker.exact, &request.claim, root, &signature, &witness)?;
        (
            Some(AuthenticatedInputManifestV2::from(tracker)),
            Some(hex::encode(signature)),
            Some(root),
            Some(witness),
        )
    };

    let funding_views = validate_funding_inputs(&request.funding, request.fee, &request.claim)?;
    let outputs = expected_outputs(
        &request.claim,
        request.amount,
        request.fee,
        request.current_height,
        &request.reserve.exact,
        &reserve_view,
        &reserve_witness,
        &funding_views,
    )?;
    let contexts = expected_context(
        &request.claim,
        &reserve_witness,
        request.tracker_signature.as_ref(),
        tracker_witness.as_ref(),
        emergency,
    )?;

    Ok(V2RedemptionManifest {
        schema: BASIS_V2_MANIFEST_SCHEMA.to_string(),
        claim: ClaimManifestV2::from_claim(&request.claim),
        amount: request.amount,
        fee: request.fee,
        current_height: request.current_height,
        minimum_confirmations: request.minimum_confirmations,
        emergency,
        reserve_input: AuthenticatedInputManifestV2::from(&request.reserve),
        funding_inputs: request
            .funding
            .iter()
            .map(AuthenticatedInputManifestV2::from)
            .collect(),
        tracker_data_input: tracker_input,
        tracker_signature,
        reserve_root: hex::encode(reserve_root),
        reserve_prior_state: reserve_witness
            .prior_state()
            .map(|state| hex::encode(state.encode())),
        reserve_prior_proof: hex::encode(reserve_witness.prior_proof()),
        reserve_next_state: hex::encode(reserve_witness.next_state().encode()),
        reserve_update_proof: hex::encode(reserve_witness.update_proof()),
        reserve_next_root: hex::encode(reserve_witness.next_root()),
        tracker_root: tracker_root.map(hex::encode),
        tracker_lookup_proof: tracker_witness
            .as_ref()
            .map(|witness| hex::encode(witness.proof())),
        tracker_committed_total_debt: tracker_witness
            .as_ref()
            .map(TrackerClaimWitnessV2::committed_total_debt),
        context_extension: contexts,
        outputs,
    })
}

/// Recompute every deciding field from exact embedded input bytes and local
/// signing intent. This function does not trust the builder's summaries.
pub fn validate_v2_redemption_manifest<'a>(
    manifest: &'a V2RedemptionManifest,
    intent: &V2SigningIntent,
) -> Result<ValidatedV2RedemptionManifest<'a>, V2BuilderError> {
    if manifest.schema != BASIS_V2_MANIFEST_SCHEMA {
        return Err(invariant("unknown v2 manifest schema"));
    }
    validate_scalar_request(
        manifest.amount,
        manifest.fee,
        manifest.minimum_confirmations,
        manifest.funding_inputs.len(),
    )?;
    let claim = manifest.claim.to_claim()?;
    if claim != intent.claim
        || manifest.amount != intent.amount
        || manifest.fee != intent.fee
        || manifest.current_height != intent.signing_tip.height
        || manifest.minimum_confirmations != intent.minimum_confirmations
    {
        return Err(invariant("manifest does not match local signing intent"));
    }

    let reserve_exact = reparse_manifest_box(&manifest.reserve_input.exact_box)?;
    if decode_array::<32>(&reserve_exact.box_id, "reserve box id")? != intent.reserve_box_id {
        return Err(invariant("unexpected reserve input box id"));
    }
    validate_observation(
        &manifest.reserve_input.observation,
        manifest.current_height,
        manifest.minimum_confirmations,
        Some(&intent.signing_tip),
    )?;
    let reserve_root = decode_array::<33>(&manifest.reserve_root, "reserve root")?;
    let reserve_view = validate_reserve_input(&reserve_exact, &claim, reserve_root)?;
    let emergency = i64::from(manifest.current_height) >= reserve_view.emergency_height;
    if manifest.emergency != emergency {
        return Err(invariant(
            "emergency mode is not derived from HEIGHT and immutable R8",
        ));
    }

    let key = claim.domain().claim_key();
    let prior_state = manifest
        .reserve_prior_state
        .as_ref()
        .map(|encoded| decode_array::<24>(encoded, "reserve prior state"))
        .transpose()?
        .map(|encoded| RedeemedStateV2::decode(&encoded))
        .transpose()
        .map_err(|error| invariant(format!("invalid reserve prior state: {error}")))?;
    let prior_proof = decode_proof(&manifest.reserve_prior_proof, "reserve prior proof")?;
    if !ReserveAvlTree::verify_lookup_bytes(
        &reserve_root,
        &key,
        &prior_proof,
        prior_state.map(RedeemedStateV2::encode),
    ) {
        return Err(invariant(
            "reserve prior membership/non-membership proof failed",
        ));
    }
    let next_state = match prior_state {
        Some(state) => state
            .advance(claim.timestamp(), claim.total_debt(), manifest.amount)
            .map_err(|error| invariant(format!("invalid reserve claim progress: {error}")))?,
        None => RedeemedStateV2::new(claim.timestamp(), claim.total_debt(), manifest.amount)
            .map_err(|error| invariant(format!("invalid initial reserve state: {error}")))?,
    };
    if hex::encode(next_state.encode()) != manifest.reserve_next_state {
        return Err(invariant("reserve successor R5 value is not exact"));
    }
    let next_root = decode_array::<33>(&manifest.reserve_next_root, "reserve next root")?;
    let update_proof = decode_proof(&manifest.reserve_update_proof, "reserve update proof")?;
    if !ReserveAvlTree::verify_transition_bytes(
        &reserve_root,
        &key,
        &next_state.encode(),
        &update_proof,
        &next_root,
    ) {
        return Err(invariant(
            "reserve insert-or-update proof or successor root failed",
        ));
    }
    let reserve_witness = ManifestReserveWitness {
        prior_proof,
        update_proof,
        next_root,
    };

    let tracker_signature = manifest
        .tracker_signature
        .as_ref()
        .map(|signature| decode_array::<65>(signature, "tracker signature"))
        .transpose()?;
    let tracker_witness = if emergency {
        if manifest.tracker_data_input.is_some()
            || tracker_signature.is_some()
            || manifest.tracker_root.is_some()
            || manifest.tracker_lookup_proof.is_some()
            || manifest.tracker_committed_total_debt.is_some()
        {
            return Err(invariant(
                "height-derived emergency manifest carries tracker evidence",
            ));
        }
        None
    } else {
        let tracker_input = manifest
            .tracker_data_input
            .as_ref()
            .ok_or_else(|| invariant("normal manifest omits tracker data input"))?;
        validate_observation(
            &tracker_input.observation,
            manifest.current_height,
            manifest.minimum_confirmations,
            Some(&intent.signing_tip),
        )?;
        let tracker_exact = reparse_manifest_box(&tracker_input.exact_box)?;
        let root = decode_array::<33>(
            manifest
                .tracker_root
                .as_ref()
                .ok_or_else(|| invariant("normal manifest omits tracker root"))?,
            "tracker root",
        )?;
        let proof = decode_proof(
            manifest
                .tracker_lookup_proof
                .as_ref()
                .ok_or_else(|| invariant("normal manifest omits tracker lookup proof"))?,
            "tracker lookup proof",
        )?;
        let committed_total_debt = manifest
            .tracker_committed_total_debt
            .ok_or_else(|| invariant("normal manifest omits committed tracker debt"))?;
        let signature = tracker_signature
            .as_ref()
            .ok_or_else(|| invariant("normal manifest omits tracker signature"))?;
        let witness = ManifestTrackerWitness {
            committed_total_debt,
            proof,
        };
        validate_tracker_input_raw(&tracker_exact, &claim, root, signature, &witness)?;
        Some(witness)
    };

    let confirmed_funding: Vec<ConfirmedChainBoxV2> = manifest
        .funding_inputs
        .iter()
        .map(|input| {
            validate_observation(
                &input.observation,
                manifest.current_height,
                manifest.minimum_confirmations,
                Some(&intent.signing_tip),
            )?;
            Ok(ConfirmedChainBoxV2 {
                exact: reparse_manifest_box(&input.exact_box)?,
                observation: input.observation.clone(),
            })
        })
        .collect::<Result<_, V2BuilderError>>()?;
    let funding_views = validate_funding_inputs(&confirmed_funding, manifest.fee, &claim)?;
    if funding_views.owner_pubkey != intent.funding_owner_pubkey {
        return Err(invariant(
            "funding inputs do not belong to the intended owner",
        ));
    }

    let expected_outputs = expected_outputs_raw(
        &claim,
        manifest.amount,
        manifest.fee,
        manifest.current_height,
        &reserve_exact,
        &reserve_view,
        &reserve_witness,
        &funding_views,
    )?;
    if manifest.outputs != expected_outputs {
        return Err(invariant(
            "outputs do not preserve exact reserve payout, fee, and owner change",
        ));
    }
    let expected_context = expected_context_raw(
        &claim,
        &reserve_witness,
        tracker_signature.as_ref(),
        tracker_witness.as_ref(),
        emergency,
    )?;
    if manifest.context_extension != expected_context {
        return Err(invariant("context extension fields or Scala order differ"));
    }

    Ok(ValidatedV2RedemptionManifest { manifest })
}

#[derive(Debug)]
struct ReserveInputView {
    emergency_height: i64,
    owner_register: String,
    tracker_register: String,
    refund_register: String,
    emergency_register: String,
}

#[derive(Debug)]
struct FundingInputView {
    total: u64,
    owner_tree: String,
    owner_pubkey: [u8; 33],
}

trait ReserveWitnessView {
    fn prior_proof(&self) -> &[u8];
    fn update_proof(&self) -> &[u8];
    fn next_root(&self) -> [u8; 33];
}

impl ReserveWitnessView for ReserveRedemptionWitnessV2 {
    fn prior_proof(&self) -> &[u8] {
        self.prior_proof()
    }
    fn update_proof(&self) -> &[u8] {
        self.update_proof()
    }
    fn next_root(&self) -> [u8; 33] {
        self.next_root()
    }
}

struct ManifestReserveWitness {
    prior_proof: Vec<u8>,
    update_proof: Vec<u8>,
    next_root: [u8; 33],
}

impl ReserveWitnessView for ManifestReserveWitness {
    fn prior_proof(&self) -> &[u8] {
        &self.prior_proof
    }
    fn update_proof(&self) -> &[u8] {
        &self.update_proof
    }
    fn next_root(&self) -> [u8; 33] {
        self.next_root
    }
}

trait TrackerWitnessView {
    fn committed_total_debt(&self) -> u64;
    fn proof(&self) -> &[u8];
}

impl TrackerWitnessView for TrackerClaimWitnessV2 {
    fn committed_total_debt(&self) -> u64 {
        self.committed_total_debt()
    }
    fn proof(&self) -> &[u8] {
        self.proof()
    }
}

struct ManifestTrackerWitness {
    committed_total_debt: u64,
    proof: Vec<u8>,
}

impl TrackerWitnessView for ManifestTrackerWitness {
    fn committed_total_debt(&self) -> u64 {
        self.committed_total_debt
    }
    fn proof(&self) -> &[u8] {
        &self.proof
    }
}

fn parse_exact_box(raw_sigma_hex: &str) -> Result<ExactBoxManifestV2, V2BuilderError> {
    let bytes = hex::decode(raw_sigma_hex)
        .map_err(|error| V2BuilderError::BoxBytes(format!("invalid hex: {error}")))?;
    let ergo_box = ErgoBox::sigma_parse_bytes(&bytes)
        .map_err(|error| V2BuilderError::BoxBytes(format!("Sigma parse failed: {error:?}")))?;
    let canonical = ergo_box.sigma_serialize_bytes()?;
    if canonical != bytes {
        return Err(V2BuilderError::BoxBytes(
            "box bytes are not the exact canonical Sigma serialization".to_string(),
        ));
    }
    let ergo_tree = hex::encode(ergo_box.ergo_tree.sigma_serialize_bytes()?);
    let tokens = ergo_box
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|token| TokenManifestV2 {
                    token_id: hex::encode(token.token_id.as_ref()),
                    amount: *token.amount.as_u64(),
                })
                .collect()
        })
        .unwrap_or_default();
    let mut registers = Vec::new();
    let mut absent_seen = false;
    for register_id in NonMandatoryRegisterId::REG_IDS {
        let value = ergo_box
            .additional_registers
            .get_constant(register_id)
            .map_err(|error| V2BuilderError::BoxBytes(error.to_string()))?;
        match value {
            Some(_constant) if absent_seen => {
                return Err(V2BuilderError::BoxBytes(
                    "non-mandatory registers are not densely packed".to_string(),
                ))
            }
            Some(constant) => registers.push(hex::encode(constant.sigma_serialize_bytes()?)),
            None => absent_seen = true,
        }
    }
    Ok(ExactBoxManifestV2 {
        box_id: ergo_box.box_id().to_string(),
        value: *ergo_box.value.as_u64(),
        ergo_tree,
        tokens,
        registers,
        creation_height: ergo_box.creation_height,
        raw_sigma_hex: hex::encode(bytes),
    })
}

fn reparse_manifest_box(
    claimed: &ExactBoxManifestV2,
) -> Result<ExactBoxManifestV2, V2BuilderError> {
    let exact = parse_exact_box(&claimed.raw_sigma_hex)?;
    if &exact != claimed {
        return Err(invariant(
            "box manifest summary differs from embedded exact Sigma bytes",
        ));
    }
    Ok(exact)
}

fn validate_scalar_request(
    amount: u64,
    fee: u64,
    minimum_confirmations: u32,
    funding_count: usize,
) -> Result<(), V2BuilderError> {
    if amount == 0 {
        return Err(invariant("redemption amount must be non-zero"));
    }
    if fee == 0 {
        return Err(invariant("miner fee must be non-zero"));
    }
    if minimum_confirmations == 0 {
        return Err(invariant("minimum confirmations must be non-zero"));
    }
    if funding_count == 0 || funding_count > BASIS_V2_MAX_FUNDING_INPUTS {
        return Err(invariant(
            "funding input count is outside the bounded profile",
        ));
    }
    Ok(())
}

fn validate_observation(
    observation: &ChainObservationManifestV2,
    current_height: u32,
    minimum_confirmations: u32,
    trusted_tip: Option<&VerifiedSigningTipV2>,
) -> Result<(), V2BuilderError> {
    let inclusion_block_id = decode_array::<32>(
        &observation.inclusion_block_id,
        "confirmed inclusion block id",
    )?;
    let tip_block_id = decode_array::<32>(&observation.tip_block_id, "confirmed tip block id")?;
    if inclusion_block_id == [0u8; 32] || tip_block_id == [0u8; 32] {
        return Err(invariant("confirmed block ids cannot be zero"));
    }
    let expected = observation
        .tip_height
        .checked_sub(observation.inclusion_height)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| invariant("confirmed inclusion height exceeds tip"))?;
    if observation.tip_height != current_height
        || observation.confirmations != expected
        || expected < minimum_confirmations
    {
        return Err(invariant(
            "confirmed observation is stale, inconsistent, or insufficiently deep",
        ));
    }
    if let Some(trusted_tip) = trusted_tip {
        if observation.tip_height != trusted_tip.height || tip_block_id != trusted_tip.block_id {
            return Err(invariant(
                "manifest observation is not bound to the independently verified signer tip",
            ));
        }
    }
    Ok(())
}

fn validate_reserve_input(
    exact: &ExactBoxManifestV2,
    claim: &ClaimV2,
    expected_root: [u8; 33],
) -> Result<ReserveInputView, V2BuilderError> {
    let domain = claim.domain();
    let expected_kind = match domain.asset() {
        ReserveAssetV2::Erg => BasisV2ContractKind::Erg,
        ReserveAssetV2::Token { .. } => BasisV2ContractKind::Token,
    };
    if exact.ergo_tree != expected_kind.ergo_tree_hex() {
        return Err(invariant(
            "reserve does not use the exact v2 golden ErgoTree",
        ));
    }
    validate_reserve_tokens(&exact.tokens, domain)?;
    if exact.registers.len() != 6 {
        return Err(invariant("reserve R4-R9 are not all present"));
    }
    let expected_owner = format!("07{}", hex::encode(domain.owner_pubkey()));
    if exact.registers[0] != expected_owner {
        return Err(invariant("reserve R4 owner differs from ClaimV2 domain"));
    }
    let avl = parse_avl_register(&exact.registers[1], "reserve R5")?;
    validate_avl_shape(&avl, expected_root, 24, "reserve R5")?;
    let tracker_id = parse_coll_register(&exact.registers[2], "reserve R6")?;
    if tracker_id != domain.tracker_nft_id() {
        return Err(invariant(
            "reserve R6 tracker NFT differs from ClaimV2 domain",
        ));
    }
    let refund_height = parse_long_register(&exact.registers[3], "reserve R7")?;
    let emergency_height = parse_long_register(&exact.registers[4], "reserve R8")?;
    let predecessor = parse_coll_register(&exact.registers[5], "reserve R9")?;
    if refund_height < 0 || emergency_height <= 0 || predecessor.len() != 32 {
        return Err(invariant("reserve R7/R8/R9 shape is invalid"));
    }
    Ok(ReserveInputView {
        emergency_height,
        owner_register: exact.registers[0].clone(),
        tracker_register: exact.registers[2].clone(),
        refund_register: exact.registers[3].clone(),
        emergency_register: exact.registers[4].clone(),
    })
}

fn validate_reserve_tokens(
    tokens: &[TokenManifestV2],
    domain: ClaimDomainV2,
) -> Result<(), V2BuilderError> {
    let reserve_nft = hex::encode(domain.reserve_nft_id());
    match domain.asset() {
        ReserveAssetV2::Erg => {
            if tokens.len() != 1 || tokens[0].token_id != reserve_nft || tokens[0].amount != 1 {
                return Err(invariant("ERG reserve singleton NFT shape is not exact"));
            }
        }
        ReserveAssetV2::Token { token_id } => {
            if tokens.len() != 2
                || tokens[0].token_id != reserve_nft
                || tokens[0].amount != 1
                || tokens[1].token_id != hex::encode(token_id)
                || tokens[1].amount == 0
            {
                return Err(invariant("token reserve NFT/asset shape is not exact"));
            }
        }
    }
    Ok(())
}

fn validate_tracker_input<W: TrackerWitnessView>(
    exact: &ExactBoxManifestV2,
    claim: &ClaimV2,
    expected_root: [u8; 33],
    signature: &Signature,
    witness: &W,
) -> Result<(), V2BuilderError> {
    validate_tracker_input_raw(exact, claim, expected_root, signature, witness)
}

fn validate_tracker_input_raw<W: TrackerWitnessView>(
    exact: &ExactBoxManifestV2,
    claim: &ClaimV2,
    expected_root: [u8; 33],
    signature: &Signature,
    witness: &W,
) -> Result<(), V2BuilderError> {
    if exact.tokens.len() != 1
        || exact.tokens[0].token_id != hex::encode(claim.domain().tracker_nft_id())
        || exact.tokens[0].amount != 1
    {
        return Err(invariant("tracker singleton NFT shape is not exact"));
    }
    if exact.registers.len() < 2 {
        return Err(invariant("tracker R4/R5 are absent"));
    }
    let tracker_key = parse_group_register(&exact.registers[0], "tracker R4")?;
    let avl = parse_avl_register(&exact.registers[1], "tracker R5")?;
    validate_avl_shape(&avl, expected_root, 8, "tracker R5")?;
    if witness.committed_total_debt() < claim.total_debt()
        || witness.committed_total_debt() > i64::MAX as u64
    {
        return Err(invariant(
            "tracker lookup debt does not cover the supplied signed ClaimV2",
        ));
    }
    let proof = witness.proof();
    validate_proof_size(proof, "tracker lookup proof")?;
    if !TrackerAvlTree::verify_lookup_bytes(
        &expected_root,
        &claim.domain().claim_key(),
        proof,
        Some(witness.committed_total_debt().to_be_bytes()),
    ) {
        return Err(invariant("tracker membership proof failed"));
    }
    let message = claim
        .signing_message()
        .map_err(|error| invariant(format!("claim message failed: {error}")))?;
    verify_basis_v2_signature(signature, &message, &tracker_key)
        .map_err(|error| invariant(format!("tracker ClaimV2 signature failed: {error}")))?;
    Ok(())
}

fn validate_funding_inputs(
    funding: &[ConfirmedChainBoxV2],
    fee: u64,
    claim: &ClaimV2,
) -> Result<FundingInputView, V2BuilderError> {
    if funding.is_empty() || funding.len() > BASIS_V2_MAX_FUNDING_INPUTS {
        return Err(invariant(
            "funding input count is outside the bounded profile",
        ));
    }
    let mut ids = HashSet::new();
    let first_tree = funding[0].exact.ergo_tree.clone();
    let owner_pubkey = parse_p2pk_tree(&first_tree)?;
    let mut total = 0u64;
    for input in funding {
        if !ids.insert(input.exact.box_id.clone()) {
            return Err(invariant("duplicate funding input"));
        }
        if !input.exact.tokens.is_empty() {
            return Err(invariant("funding inputs must be token-free"));
        }
        if input.exact.ergo_tree != first_tree
            || parse_p2pk_tree(&input.exact.ergo_tree)? != owner_pubkey
        {
            return Err(invariant(
                "all fee/change inputs must share one exact P2PK owner",
            ));
        }
        total = total
            .checked_add(input.exact.value)
            .ok_or_else(|| invariant("funding value overflow"))?;
    }
    let payout_erg = match claim.domain().asset() {
        ReserveAssetV2::Erg => 0,
        ReserveAssetV2::Token { .. } => BASIS_V2_MIN_BOX_VALUE,
    };
    let required = fee
        .checked_add(payout_erg)
        .ok_or_else(|| invariant("required funding overflow"))?;
    if total < required {
        return Err(invariant(
            "funding boxes do not cover fee and token payout ERG",
        ));
    }
    let change = total - required;
    if change > 0 && change < BASIS_V2_MIN_BOX_VALUE {
        return Err(invariant("funding change would create a sub-minimum box"));
    }
    Ok(FundingInputView {
        total,
        owner_tree: first_tree,
        owner_pubkey,
    })
}

fn expected_outputs<W: ReserveWitnessView>(
    claim: &ClaimV2,
    amount: u64,
    fee: u64,
    current_height: u32,
    reserve: &ExactBoxManifestV2,
    reserve_view: &ReserveInputView,
    witness: &W,
    funding: &FundingInputView,
) -> Result<Vec<OutputManifestV2>, V2BuilderError> {
    expected_outputs_raw(
        claim,
        amount,
        fee,
        current_height,
        reserve,
        reserve_view,
        witness,
        funding,
    )
}

fn expected_outputs_raw<W: ReserveWitnessView>(
    claim: &ClaimV2,
    amount: u64,
    fee: u64,
    current_height: u32,
    reserve: &ExactBoxManifestV2,
    reserve_view: &ReserveInputView,
    witness: &W,
    funding: &FundingInputView,
) -> Result<Vec<OutputManifestV2>, V2BuilderError> {
    validate_proof_size(witness.prior_proof(), "reserve prior proof")?;
    validate_proof_size(witness.update_proof(), "reserve update proof")?;
    let domain = claim.domain();
    let box_id = decode_array::<32>(&reserve.box_id, "reserve box id")?;
    let successor_r5 = serialize_fixed_avl_register(witness.next_root(), 24)?;
    let mut successor_registers = BTreeMap::new();
    successor_registers.insert("R4".to_string(), reserve_view.owner_register.clone());
    successor_registers.insert("R5".to_string(), successor_r5);
    successor_registers.insert("R6".to_string(), reserve_view.tracker_register.clone());
    successor_registers.insert("R7".to_string(), reserve_view.refund_register.clone());
    successor_registers.insert("R8".to_string(), reserve_view.emergency_register.clone());
    successor_registers.insert("R9".to_string(), serialize_coll_bytes(&box_id));
    let mut payout_registers = BTreeMap::new();
    payout_registers.insert("R4".to_string(), serialize_coll_bytes(&box_id));

    let (successor_value, successor_tokens, payout_value, payout_tokens, external_payout_erg) =
        match domain.asset() {
            ReserveAssetV2::Erg => {
                let successor_value = reserve
                    .value
                    .checked_sub(amount)
                    .ok_or_else(|| invariant("ERG payout exceeds reserve value"))?;
                if successor_value < BASIS_V2_MIN_BOX_VALUE {
                    return Err(invariant(
                        "ERG reserve successor is below minimum box value",
                    ));
                }
                (
                    successor_value,
                    reserve.tokens.clone(),
                    amount,
                    Vec::new(),
                    0,
                )
            }
            ReserveAssetV2::Token { token_id } => {
                let reserve_amount = reserve.tokens[1].amount;
                let successor_amount = reserve_amount
                    .checked_sub(amount)
                    .ok_or_else(|| invariant("token payout exceeds reserve token balance"))?;
                if successor_amount == 0 {
                    return Err(invariant(
                        "token reserve successor must retain its indexed reserve asset",
                    ));
                }
                let mut tokens = reserve.tokens.clone();
                tokens[1].amount = successor_amount;
                (
                    reserve.value,
                    tokens,
                    BASIS_V2_MIN_BOX_VALUE,
                    vec![TokenManifestV2 {
                        token_id: hex::encode(token_id),
                        amount,
                    }],
                    BASIS_V2_MIN_BOX_VALUE,
                )
            }
        };
    let receiver_tree = format!("0008cd{}", hex::encode(domain.receiver_pubkey()));
    let mut outputs = vec![
        OutputManifestV2 {
            value: successor_value,
            ergo_tree: reserve.ergo_tree.clone(),
            creation_height: current_height,
            tokens: successor_tokens,
            additional_registers: successor_registers,
        },
        OutputManifestV2 {
            value: payout_value,
            ergo_tree: receiver_tree,
            creation_height: current_height,
            tokens: payout_tokens,
            additional_registers: payout_registers,
        },
        OutputManifestV2 {
            value: fee,
            ergo_tree: BASIS_V2_FEE_ERGO_TREE.to_string(),
            creation_height: current_height,
            tokens: Vec::new(),
            additional_registers: BTreeMap::new(),
        },
    ];
    let change = funding
        .total
        .checked_sub(fee)
        .and_then(|value| value.checked_sub(external_payout_erg))
        .ok_or_else(|| invariant("funding does not cover external outputs"))?;
    if change > 0 {
        if change < BASIS_V2_MIN_BOX_VALUE {
            return Err(invariant("change output is below minimum box value"));
        }
        outputs.push(OutputManifestV2 {
            value: change,
            ergo_tree: funding.owner_tree.clone(),
            creation_height: current_height,
            tokens: Vec::new(),
            additional_registers: BTreeMap::new(),
        });
    }
    Ok(outputs)
}

fn expected_context<W: ReserveWitnessView, T: TrackerWitnessView>(
    claim: &ClaimV2,
    reserve: &W,
    tracker_signature: Option<&Signature>,
    tracker: Option<&T>,
    emergency: bool,
) -> Result<Vec<ContextVariableManifestV2>, V2BuilderError> {
    expected_context_raw(claim, reserve, tracker_signature, tracker, emergency)
}

fn expected_context_raw<W: ReserveWitnessView, T: TrackerWitnessView>(
    claim: &ClaimV2,
    reserve: &W,
    tracker_signature: Option<&Signature>,
    tracker: Option<&T>,
    emergency: bool,
) -> Result<Vec<ContextVariableManifestV2>, V2BuilderError> {
    validate_proof_size(reserve.prior_proof(), "reserve prior proof")?;
    validate_proof_size(reserve.update_proof(), "reserve update proof")?;
    let mut values = BTreeMap::new();
    values.insert(0, serialize_ergo_byte(0));
    values.insert(
        1,
        format!("07{}", hex::encode(claim.domain().receiver_pubkey())),
    );
    values.insert(2, serialize_coll_bytes(claim.signature()));
    values.insert(3, serialize_ergo_long(to_ergo_long(claim.total_debt())?));
    values.insert(4, serialize_ergo_long(to_ergo_long(claim.timestamp())?));
    values.insert(5, serialize_coll_bytes(reserve.update_proof()));
    values.insert(7, serialize_coll_bytes(reserve.prior_proof()));
    if emergency {
        if tracker_signature.is_some() || tracker.is_some() {
            return Err(invariant(
                "emergency context carries tracker-only variables",
            ));
        }
    } else {
        let signature = tracker_signature
            .ok_or_else(|| invariant("normal context omits tracker signature #6"))?;
        let witness = tracker.ok_or_else(|| invariant("normal context omits tracker proof #8"))?;
        validate_proof_size(witness.proof(), "tracker lookup proof")?;
        values.insert(6, serialize_coll_bytes(signature));
        values.insert(8, serialize_coll_bytes(witness.proof()));
    }
    let keys: Vec<u8> = values.keys().copied().collect();
    let order = scala_context_extension_order(&keys);
    Ok(order
        .into_iter()
        .map(|index| ContextVariableManifestV2 {
            index,
            serialized_value: values.remove(&index).expect("ordered key exists"),
        })
        .collect())
}

fn parse_constant(encoded: &str, label: &str) -> Result<Constant, V2BuilderError> {
    let bytes = hex::decode(encoded).map_err(|error| invariant(format!("{label} hex: {error}")))?;
    Constant::sigma_parse_bytes(&bytes)
        .map_err(|error| invariant(format!("{label} constant parse: {error:?}")))
}

fn parse_avl_register(encoded: &str, label: &str) -> Result<AvlTreeData, V2BuilderError> {
    parse_constant(encoded, label)?
        .try_extract_into::<AvlTreeData>()
        .map_err(|error| invariant(format!("{label} is not an AVL tree: {error}")))
}

fn parse_long_register(encoded: &str, label: &str) -> Result<i64, V2BuilderError> {
    parse_constant(encoded, label)?
        .try_extract_into::<i64>()
        .map_err(|error| invariant(format!("{label} is not a Long: {error}")))
}

fn parse_coll_register(encoded: &str, label: &str) -> Result<Vec<u8>, V2BuilderError> {
    parse_constant(encoded, label)?
        .try_extract_into::<Vec<u8>>()
        .map_err(|error| invariant(format!("{label} is not Coll[Byte]: {error}")))
}

fn parse_group_register(encoded: &str, label: &str) -> Result<[u8; 33], V2BuilderError> {
    let bytes = hex::decode(encoded).map_err(|error| invariant(format!("{label} hex: {error}")))?;
    if bytes.len() != 34 || bytes[0] != 0x07 {
        return Err(invariant(format!(
            "{label} is not a canonical GroupElement"
        )));
    }
    decode_array::<33>(&hex::encode(&bytes[1..]), label)
}

fn validate_avl_shape(
    avl: &AvlTreeData,
    expected_root: [u8; 33],
    value_length: u32,
    label: &str,
) -> Result<(), V2BuilderError> {
    if avl.digest.0 != expected_root
        || avl.key_length != 32
        || avl.value_length_opt.as_deref() != Some(&value_length)
        || !avl.tree_flags.insert_allowed()
        || !avl.tree_flags.update_allowed()
        || avl.tree_flags.remove_allowed()
    {
        return Err(invariant(format!(
            "{label} fixed AVL shape or root differs"
        )));
    }
    Ok(())
}

fn serialize_fixed_avl_register(
    root: [u8; 33],
    value_length: u32,
) -> Result<String, V2BuilderError> {
    let constant: Constant = AvlTreeData {
        digest: ADDigest::from(root),
        tree_flags: AvlTreeFlags::new(true, true, false),
        key_length: 32,
        value_length_opt: Some(Box::new(value_length)),
    }
    .into();
    Ok(hex::encode(constant.sigma_serialize_bytes()?))
}

fn parse_p2pk_tree(tree: &str) -> Result<[u8; 33], V2BuilderError> {
    let bytes =
        hex::decode(tree).map_err(|error| invariant(format!("funding tree hex: {error}")))?;
    if bytes.len() != 36 || bytes[..3] != [0x00, 0x08, 0xcd] {
        return Err(invariant("funding input is not exact P2PK"));
    }
    bytes[3..]
        .try_into()
        .map_err(|_| invariant("funding P2PK key length differs"))
}

fn decode_array<const N: usize>(encoded: &str, label: &str) -> Result<[u8; N], V2BuilderError> {
    let bytes = hex::decode(encoded).map_err(|error| invariant(format!("{label} hex: {error}")))?;
    bytes
        .try_into()
        .map_err(|_| invariant(format!("{label} must be exactly {N} bytes")))
}

fn decode_proof(encoded: &str, label: &str) -> Result<Vec<u8>, V2BuilderError> {
    let proof = hex::decode(encoded).map_err(|error| invariant(format!("{label} hex: {error}")))?;
    validate_proof_size(&proof, label)?;
    Ok(proof)
}

fn validate_proof_size(proof: &[u8], label: &str) -> Result<(), V2BuilderError> {
    if proof.is_empty() || proof.len() > BASIS_V2_MAX_PROOF_BYTES {
        return Err(invariant(format!(
            "{label} is empty or exceeds the bounded profile"
        )));
    }
    Ok(())
}

fn to_ergo_long(value: u64) -> Result<i64, V2BuilderError> {
    i64::try_from(value).map_err(|_| invariant("value exceeds the Ergo Long domain"))
}

fn invariant(message: impl Into<String>) -> V2BuilderError {
    V2BuilderError::Invariant(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis_v2_state::{FreshV2StateApproval, ReserveStoreBindingV2};
    use basis_core::impls::schnorr_sign;
    use ergo_lib::ergo_chain_types::{Digest32, EcPoint};
    use ergo_lib::ergotree_ir::chain::ergo_box::{
        box_value::BoxValue, BoxTokens, NonMandatoryRegisters,
    };
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use tempfile::TempDir;

    const CONTEXT_ABI_VECTOR: &str = include_str!("../tests/fixtures/basis_v2_context_abi.json");

    struct Fixture {
        manifest: V2RedemptionManifest,
        intent: V2SigningIntent,
        old_total: u64,
        committed_total: u64,
    }

    fn key(seed: u8) -> ([u8; 32], [u8; 33]) {
        let mut secret = [0u8; 32];
        secret[31] = seed;
        let key = SecretKey::from_slice(&secret).unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key).serialize();
        (secret, public)
    }

    fn avl_constant(root: [u8; 33], value_length: u32) -> Constant {
        AvlTreeData {
            digest: ADDigest::from(root),
            tree_flags: AvlTreeFlags::new(true, true, false),
            key_length: 32,
            value_length_opt: Some(Box::new(value_length)),
        }
        .into()
    }

    #[test]
    fn successor_avl_register_uses_canonical_sigma_option_encoding() {
        for first_byte in u8::MIN..=u8::MAX {
            let mut root = [0u8; 33];
            root[0] = first_byte;
            root[32] = first_byte.wrapping_add(1);
            let encoded = serialize_fixed_avl_register(root, 24).unwrap();
            let parsed = parse_avl_register(&encoded, "successor R5").unwrap();
            validate_avl_shape(&parsed, root, 24, "successor R5").unwrap();
        }
    }

    fn group_constant(key: [u8; 33]) -> Constant {
        EcPoint::sigma_parse_bytes(&key).unwrap().into()
    }

    fn token(id: [u8; 32], amount: u64) -> Token {
        Token {
            token_id: TokenId::from(Digest32::from(id)),
            amount: TokenAmount::try_from(amount).unwrap(),
        }
    }

    fn exact_box(
        tree_hex: &str,
        value: u64,
        tokens: Vec<Token>,
        registers: Vec<Constant>,
        index: u16,
    ) -> String {
        let tree = ErgoTree::sigma_parse_bytes(&hex::decode(tree_hex).unwrap()).unwrap();
        let tokens = if tokens.is_empty() {
            None
        } else {
            Some(BoxTokens::try_from(tokens).unwrap())
        };
        let registers = NonMandatoryRegisters::try_from(registers).unwrap();
        let ergo_box = ErgoBox::new(
            BoxValue::try_from(value).unwrap(),
            tree,
            tokens,
            registers,
            90,
            TxId::zero(),
            index,
        )
        .unwrap();
        hex::encode(ergo_box.sigma_serialize_bytes().unwrap())
    }

    fn confirmed(raw: &str, current_height: u32) -> ConfirmedChainBoxV2 {
        ConfirmedChainBoxV2::from_test_observation(
            raw,
            [0xabu8; 32],
            current_height - 4,
            [0xcdu8; 32],
            current_height,
        )
        .unwrap()
    }

    fn make_fixture(token_reserve: bool, emergency: bool) -> Fixture {
        let tracker_nft = [0x11u8; 32];
        let reserve_nft = [0x22u8; 32];
        let reserve_token = [0x33u8; 32];
        let (owner_secret, owner) = key(1);
        let (_receiver_secret, receiver) = key(2);
        let (tracker_secret, tracker_key) = key(3);
        let (_funding_secret, funding_key) = key(4);
        let old_total = if token_reserve { 100 } else { 100_000_000 };
        let committed_total = if token_reserve { 150 } else { 150_000_000 };
        let amount = if token_reserve { 20 } else { 20_000_000 };
        let fee = 1_000_000;
        let current_height = if emergency { 110 } else { 100 };
        let domain = if token_reserve {
            ClaimDomainV2::token(reserve_nft, reserve_token, tracker_nft, owner, receiver).unwrap()
        } else {
            ClaimDomainV2::erg(reserve_nft, tracker_nft, owner, receiver).unwrap()
        };
        let claim = ClaimV2::sign(domain, old_total, 10, &owner_secret).unwrap();
        let newer_claim = ClaimV2::sign(domain, committed_total, 11, &owner_secret).unwrap();

        let tracker_dir = TempDir::new().unwrap();
        let reserve_dir = TempDir::new().unwrap();
        let mut tracker_store = TrackerClaimStoreV2::open(
            tracker_dir.path(),
            tracker_nft,
            FreshV2StateApproval::Approve,
        )
        .unwrap();
        tracker_store.record_validated_claim(newer_claim).unwrap();
        let binding = if token_reserve {
            ReserveStoreBindingV2::token(tracker_nft, reserve_nft, reserve_token).unwrap()
        } else {
            ReserveStoreBindingV2::erg(tracker_nft, reserve_nft)
        };
        let mut reserve_store = ReserveRedeemedStoreV2::open(
            reserve_dir.path(),
            binding,
            FreshV2StateApproval::Approve,
        )
        .unwrap();

        let reserve_root = reserve_store.root_digest().unwrap();
        let reserve_kind = if token_reserve {
            BasisV2ContractKind::Token
        } else {
            BasisV2ContractKind::Erg
        };
        let reserve_tokens = if token_reserve {
            vec![token(reserve_nft, 1), token(reserve_token, 100)]
        } else {
            vec![token(reserve_nft, 1)]
        };
        let reserve_raw = exact_box(
            reserve_kind.ergo_tree_hex(),
            if token_reserve {
                2_000_000
            } else {
                100_000_000
            },
            reserve_tokens,
            vec![
                group_constant(owner),
                avl_constant(reserve_root, 24),
                Constant::from(tracker_nft.to_vec()),
                Constant::from(0i64),
                Constant::from(110i64),
                Constant::from([0x44u8; 32].to_vec()),
            ],
            0,
        );
        let tracker_root = tracker_store.root_digest().unwrap();
        let tracker_raw = exact_box(
            &format!("0008cd{}", hex::encode(tracker_key)),
            2_000_000,
            vec![token(tracker_nft, 1)],
            vec![group_constant(tracker_key), avl_constant(tracker_root, 8)],
            1,
        );
        let funding_raw = exact_box(
            &format!("0008cd{}", hex::encode(funding_key)),
            3_000_000,
            Vec::new(),
            Vec::new(),
            2,
        );
        let reserve = confirmed(&reserve_raw, current_height);
        let reserve_box_id = decode_array::<32>(&reserve.exact.box_id, "reserve id").unwrap();
        let funding = confirmed(&funding_raw, current_height);
        let message = claim.signing_message().unwrap();
        let tracker_signature = schnorr_sign(&message, &tracker_secret, &tracker_key).unwrap();
        let (tracker, signature, tracker_state) = if emergency {
            (None, None, None)
        } else {
            (
                Some(confirmed(&tracker_raw, current_height)),
                Some(tracker_signature),
                Some(&mut tracker_store),
            )
        };
        let request = V2RedemptionBuildRequest::from_test_confirmed_state(
            claim.clone(),
            amount,
            fee,
            current_height,
            5,
            reserve,
            vec![funding],
            tracker,
            signature,
        );
        let manifest =
            build_v2_redemption_manifest(request, &mut reserve_store, tracker_state).unwrap();
        let intent = V2SigningIntent::new(
            claim,
            amount,
            fee,
            VerifiedSigningTipV2::from_test_tip([0xcdu8; 32], current_height),
            5,
            reserve_box_id,
            funding_key,
        )
        .unwrap();
        Fixture {
            manifest,
            intent,
            old_total,
            committed_total,
        }
    }

    fn reject(manifest: &V2RedemptionManifest, intent: &V2SigningIntent) {
        let callback_calls = std::cell::Cell::new(0usize);
        let result = with_validated_v2_redemption_manifest(manifest, intent, |_| {
            callback_calls.set(callback_calls.get() + 1);
        });
        assert!(result.is_err());
        assert_eq!(
            callback_calls.get(),
            0,
            "proof/signature callback ran for a rejected manifest"
        );
    }

    #[test]
    fn older_signed_claim_remains_exact_when_tracker_commits_newer_total() {
        let fixture = make_fixture(false, false);
        let callback_calls = std::cell::Cell::new(0usize);
        with_validated_v2_redemption_manifest(&fixture.manifest, &fixture.intent, |validated| {
            callback_calls.set(callback_calls.get() + 1);
            assert!(std::ptr::eq(validated.manifest(), &fixture.manifest));
        })
        .unwrap();
        assert_eq!(callback_calls.get(), 1);
        assert_eq!(
            fixture.manifest.tracker_committed_total_debt,
            Some(fixture.committed_total)
        );
        let claim_total = fixture
            .manifest
            .context_extension
            .iter()
            .find(|variable| variable.index == 3)
            .unwrap();
        assert_eq!(
            claim_total.serialized_value,
            serialize_ergo_long(fixture.old_total as i64)
        );
        assert_ne!(fixture.old_total, fixture.committed_total);
    }

    #[test]
    fn exact_payout_fee_and_owner_change_each_fail_as_single_faults() {
        let fixture = make_fixture(false, false);

        let mut payout = fixture.manifest.clone();
        payout.outputs[1].value += 1;
        reject(&payout, &fixture.intent);

        let mut fee = fixture.manifest.clone();
        fee.outputs[2].value += 1;
        reject(&fee, &fixture.intent);

        let mut change = fixture.manifest.clone();
        change.outputs[3].value += 1;
        reject(&change, &fixture.intent);

        let mut payout_script = fixture.manifest.clone();
        payout_script.outputs[1].ergo_tree.push_str("00");
        reject(&payout_script, &fixture.intent);

        let mut fee_script = fixture.manifest.clone();
        fee_script.outputs[2].ergo_tree.push_str("00");
        reject(&fee_script, &fixture.intent);

        let mut change_owner = fixture.manifest.clone();
        change_owner.outputs[3].ergo_tree.push_str("00");
        reject(&change_owner, &fixture.intent);

        let mut funding_owner = fixture.manifest.clone();
        funding_owner.funding_inputs[0]
            .exact_box
            .ergo_tree
            .push_str("00");
        reject(&funding_owner, &fixture.intent);

        assert_eq!(
            fixture.manifest.outputs[0].value + fixture.manifest.outputs[1].value,
            fixture.manifest.reserve_input.exact_box.value
        );
    }

    #[test]
    fn reserve_successor_r5_r8_r9_and_prior_proof_are_individually_bound() {
        let fixture = make_fixture(false, false);
        for register in ["R5", "R8", "R9"] {
            let mut mutant = fixture.manifest.clone();
            mutant.outputs[0]
                .additional_registers
                .get_mut(register)
                .unwrap()
                .push_str("00");
            reject(&mutant, &fixture.intent);
        }

        let mut absent_prior = fixture.manifest.clone();
        absent_prior.reserve_prior_proof.clear();
        reject(&absent_prior, &fixture.intent);
    }

    #[test]
    fn amount_timestamp_and_context_fields_are_not_builder_controlled() {
        let fixture = make_fixture(false, false);

        let mut amount = fixture.manifest.clone();
        amount.amount += 1;
        reject(&amount, &fixture.intent);

        let mut timestamp = fixture.manifest.clone();
        timestamp.claim.timestamp += 1;
        reject(&timestamp, &fixture.intent);

        let mut receiver = fixture.manifest.clone();
        receiver
            .context_extension
            .iter_mut()
            .find(|variable| variable.index == 1)
            .unwrap()
            .serialized_value
            .push_str("00");
        reject(&receiver, &fixture.intent);
    }

    #[test]
    fn reserve_domain_nft_tracker_nft_and_tracker_proof_are_individually_bound() {
        let fixture = make_fixture(false, false);

        let mut claim_domain = fixture.manifest.clone();
        claim_domain.claim.reserve_nft_id = "44".repeat(32);
        reject(&claim_domain, &fixture.intent);

        let mut reserve_nft = fixture.manifest.clone();
        reserve_nft.reserve_input.exact_box.tokens[0].token_id = "55".repeat(32);
        reject(&reserve_nft, &fixture.intent);

        let mut tracker_nft = fixture.manifest.clone();
        tracker_nft
            .tracker_data_input
            .as_mut()
            .unwrap()
            .exact_box
            .tokens[0]
            .token_id = "66".repeat(32);
        reject(&tracker_nft, &fixture.intent);

        let mut proof = fixture.manifest.clone();
        let mut proof_bytes = hex::decode(proof.tracker_lookup_proof.as_ref().unwrap()).unwrap();
        *proof_bytes.last_mut().unwrap() ^= 1;
        proof.tracker_lookup_proof = Some(hex::encode(proof_bytes));
        reject(&proof, &fixture.intent);
    }

    #[test]
    fn emergency_is_derived_only_from_height_and_immutable_r8() {
        let normal = make_fixture(false, false);
        assert!(!normal.manifest.emergency);
        assert!(normal.manifest.tracker_data_input.is_some());
        validate_v2_redemption_manifest(&normal.manifest, &normal.intent).unwrap();

        let emergency = make_fixture(false, true);
        assert!(emergency.manifest.emergency);
        assert!(emergency.manifest.tracker_data_input.is_none());
        assert!(emergency.manifest.tracker_signature.is_none());
        validate_v2_redemption_manifest(&emergency.manifest, &emergency.intent).unwrap();

        let mut caller_boolean = normal.manifest.clone();
        caller_boolean.emergency = true;
        reject(&caller_boolean, &normal.intent);
    }

    #[test]
    fn emergency_context_omission_matches_the_pinned_scala_source_vector() {
        let vector: Value = serde_json::from_str(CONTEXT_ABI_VECTOR).unwrap();
        assert_eq!(
            vector["source_commit"],
            "9a274396d5f78f7be5ed76bacee5329c42570317"
        );

        let normal = make_fixture(false, false);
        let normal_ids: Vec<u8> = normal
            .manifest
            .context_extension
            .iter()
            .map(|variable| variable.index)
            .collect();
        assert_eq!(
            normal_ids,
            serde_json::from_value::<Vec<u8>>(
                vector["normal"]["scala_serialization_order"].clone()
            )
            .unwrap()
        );
        assert_eq!(
            normal.manifest.unsigned_transaction_json()["dataInputs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let emergency = make_fixture(false, true);
        let emergency_ids: Vec<u8> = emergency
            .manifest
            .context_extension
            .iter()
            .map(|variable| variable.index)
            .collect();
        assert_eq!(
            emergency_ids,
            serde_json::from_value::<Vec<u8>>(
                vector["emergency"]["scala_serialization_order"].clone()
            )
            .unwrap()
        );
        assert!(!emergency_ids.contains(&6));
        assert!(!emergency_ids.contains(&8));
        assert_eq!(
            emergency.manifest.unsigned_transaction_json()["dataInputs"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        let successor_r5 = emergency.manifest.outputs[0]
            .additional_registers
            .get("R5")
            .unwrap();
        let parsed = parse_avl_register(successor_r5, "successor R5").unwrap();
        assert_eq!(parsed.key_length, 32);
        assert_eq!(parsed.value_length_opt.as_deref(), Some(&24));
        assert_eq!(parsed.tree_flags.serialize(), 0x03);
    }

    #[test]
    fn token_reserve_uses_exact_token_payout_and_external_erg_funding() {
        let fixture = make_fixture(true, false);
        validate_v2_redemption_manifest(&fixture.manifest, &fixture.intent).unwrap();
        assert_eq!(fixture.manifest.outputs[0].value, 2_000_000);
        assert_eq!(fixture.manifest.outputs[0].tokens[1].amount, 80);
        assert_eq!(fixture.manifest.outputs[1].value, BASIS_V2_MIN_BOX_VALUE);
        assert_eq!(fixture.manifest.outputs[1].tokens.len(), 1);
        assert_eq!(fixture.manifest.outputs[1].tokens[0].amount, 20);

        let mut token_amount = fixture.manifest.clone();
        token_amount.outputs[1].tokens[0].amount += 1;
        reject(&token_amount, &fixture.intent);

        let mut token_id = fixture.manifest.clone();
        token_id.reserve_input.exact_box.tokens[1].token_id = "88".repeat(32);
        reject(&token_id, &fixture.intent);
    }

    #[test]
    fn opaque_authority_binds_the_independent_signer_tip_and_depth() {
        let fixture = make_fixture(false, false);

        let mut fork_tip = fixture.manifest.clone();
        fork_tip.reserve_input.observation.tip_block_id = "ee".repeat(32);
        reject(&fork_tip, &fixture.intent);

        let mut shallow = fixture.manifest.clone();
        shallow.reserve_input.observation.inclusion_height = 100;
        shallow.reserve_input.observation.confirmations = 1;
        reject(&shallow, &fixture.intent);
    }
}
