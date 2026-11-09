//! HTTP client for the Basis Tracker server.
//!
//! `TrackerClient` wraps `ureq` and exposes typed request/response helpers for
//! every tracker endpoint. It also implements the three supported authentication
//! schemes (anonymous, API-key, secp256k1 Schnorr signatures) and attaches the
//! appropriate headers to each request automatically.
//!
//! The canonical signing format is documented in
//! `specs/server/authentication_authorization.md`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Authentication scheme used by [`TrackerClient`] when talking to a tracker server.
#[derive(Debug, Clone, Default)]
pub enum TrackerAuth {
    /// No authentication (default, backward-compatible).
    #[default]
    None,
    /// Shared API key sent as `Authorization: Bearer <key>`.
    ApiKey(String),
    /// Per-request secp256k1 Schnorr signature.
    Signature {
        /// Hex-encoded 33-byte compressed public key (66 characters).
        pubkey_hex: String,
        /// Hex-encoded 32-byte secret key (64 characters).
        secret_key_hex: String,
    },
}

impl TrackerAuth {
    /// Create an API-key auth config.
    pub fn api_key(key: impl Into<String>) -> Self {
        Self::ApiKey(key.into())
    }

    /// Create a secp256k1 signature auth config.
    pub fn signature(pubkey_hex: impl Into<String>, secret_key_hex: impl Into<String>) -> Self {
        Self::Signature {
            pubkey_hex: pubkey_hex.into(),
            secret_key_hex: secret_key_hex.into(),
        }
    }

    /// Build auth settings from the CLI configuration.
    pub fn from_config(config: &crate::config::CliConfig) -> Self {
        match config
            .server_auth_mode
            .as_deref()
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("api_key") => config
                .server_api_key
                .as_ref()
                .filter(|k| !k.is_empty())
                .map(Self::api_key)
                .unwrap_or_default(),
            Some("signature") => {
                match (
                    config.server_auth_pubkey.as_ref(),
                    config.server_auth_secret_key.as_ref(),
                ) {
                    (Some(pk), Some(sk)) if !pk.is_empty() && !sk.is_empty() => {
                        Self::signature(pk, sk)
                    }
                    _ => Self::None,
                }
            }
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String,
    pub tracker_nft_id: String,
    /// Denomination of the note ("erg" or "token:<64-hex>"); default "erg".
    #[serde(default)]
    pub denomination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIouNote {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    #[serde(default)]
    pub tracker_nft_id: String,
    /// Denomination of the debt: "erg" or "token:<64-hex>".
    #[serde(default = "default_denomination_erg")]
    pub denomination: String,
}

fn default_denomination_erg() -> String {
    "erg".to_string()
}

impl SerializableIouNote {
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }

    #[allow(dead_code)]
    pub fn is_fully_redeemed(&self) -> bool {
        self.amount_collected == self.amount_redeemed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatusDenomination {
    /// "erg" or "token:<64-hex>"
    pub denomination: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub note_count: usize,
    #[serde(default)]
    pub reserve_box_id: Option<String>,
    #[serde(default)]
    pub has_pending_refund: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatusResponse {
    pub issuer_pubkey: String,
    /// Unit-free aggregate: total number of notes across all denominations.
    pub note_count: usize,
    pub last_updated: u64,
    pub has_pending_refund: bool,
    /// Per-denomination breakdown (debt/collateral are never summed across denominations).
    #[serde(default)]
    pub denominations: Vec<KeyStatusDenomination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReserveRequest {
    pub nft_id: String,
    pub owner_pubkey: String,
    pub erg_amount: u64,
    #[serde(default)]
    pub token_amount: u64,
    #[serde(default)]
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveCreationResponse {
    pub requests: Vec<ReservePaymentRequest>,
    pub fee: u64,
    /// Owner's change address (derived from the owner pubkey by the tracker).
    ///
    /// The tracker only *builds* this payload: the reserve owner submits it to
    /// their own Ergo node's `/wallet/payment/send` (the requests array is the
    /// request body), so the tracker's wallet is never involved. Use
    /// [`submit_reserve_to_node`] or submit the payload manually.
    pub change_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservePaymentRequest {
    pub address: String,
    pub value: u64,
    pub assets: Vec<Asset>,
    pub registers: std::collections::HashMap<String, String>,
}

/// Reserve AVL tree state digest returned by `GET /reserves/state-digest`:
/// the tracker's reserve tree for an (issuer, denomination) pair, both as the
/// raw 33-byte root digest and as the serialized SAvlTree value a reserve
/// (re)creation must carry in R5 (issue #15 review N9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveStateDigestResponse {
    pub issuer_pubkey: String,
    pub denomination: String,
    pub reserve_state_digest: String,
    pub reserve_r5: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub token_id: String,
    pub amount: u64,
}

/// Asset shape the Ergo node's `/wallet/payment/send` expects (`tokenId`, camelCase).
#[derive(Debug, Clone, Serialize)]
struct NodeAsset {
    #[serde(rename = "tokenId")]
    token_id: String,
    amount: u64,
}

/// Payment request shape the Ergo node's `/wallet/payment/send` expects.
#[derive(Debug, Clone, Serialize)]
struct NodePaymentRequest {
    address: String,
    value: u64,
    assets: Vec<NodeAsset>,
    registers: std::collections::HashMap<String, String>,
}

/// ErgoTree hex prefix of a pay-to-public-key (P2PK) script: `0008cd` followed by the
/// 33-byte compressed public key.
const P2PK_ERGO_TREE_PREFIX: &str = "0008cd";

/// Value of the pre-deployment UTXO sent to the owner's own P2PK address when the
/// wallet holds no P2PK(owner) box yet (0.001 ERG).
const PRE_DEPLOY_UTXO_VALUE: u64 = 1_000_000;

/// Bounded wait for the pre-deployment UTXO to appear in the node wallet.
const PRE_DEPLOY_POLL_ATTEMPTS: u32 = 15;
const PRE_DEPLOY_POLL_INTERVAL_SECS: u64 = 2;

/// ErgoTree of the P2PK script locking boxes to `pubkey_hex` (33-byte compressed key).
fn p2pk_ergo_tree(pubkey_hex: &str) -> String {
    format!("{}{}", P2PK_ERGO_TREE_PREFIX, pubkey_hex)
}

/// Recover the reserve owner public key from the payload's R4 register (`07` + pubkey).
fn payload_owner_pubkey(payload: &ReserveCreationResponse) -> Result<String> {
    for request in &payload.requests {
        if let Some(r4) = request.registers.get("R4") {
            if let Some(pubkey) = r4.strip_prefix("07") {
                if pubkey.len() == 66 && hex::decode(pubkey).is_ok() {
                    return Ok(pubkey.to_string());
                }
            }
        }
    }
    Err(anyhow::anyhow!(
        "reserve creation payload has no valid R4 owner public key register"
    ))
}

fn node_get(node_url: &str, api_key: Option<&str>, path: &str) -> ureq::Request {
    let mut request = ureq::get(&format!("{}{}", node_url.trim_end_matches('/'), path));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.set("api_key", key);
    }
    request
}

fn node_post(node_url: &str, api_key: Option<&str>, path: &str) -> ureq::Request {
    let mut request = ureq::post(&format!("{}{}", node_url.trim_end_matches('/'), path));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.set("api_key", key);
    }
    request
}

/// Fetch unspent wallet boxes (including unconfirmed ones) from the node.
fn fetch_wallet_boxes(node_url: &str, api_key: Option<&str>) -> Result<Vec<ErgoBoxDetails>> {
    #[derive(Debug, Deserialize)]
    struct WalletBoxEntry {
        #[serde(rename = "box")]
        details: ErgoBoxDetails,
    }
    let response = node_get(
        node_url,
        api_key,
        "/wallet/boxes/unspent?minConfirmations=-1&maxConfirmations=-1",
    )
    .call()
    .map_err(|e| anyhow::anyhow!("Ergo node /wallet/boxes/unspent failed: {}", e))?;
    let entries: Vec<WalletBoxEntry> = response.into_json()?;
    Ok(entries.into_iter().map(|e| e.details).collect())
}

/// Fetch the hex-encoded serialized bytes of a box (for `inputsRaw`).
fn fetch_box_binary(node_url: &str, api_key: Option<&str>, box_id: &str) -> Result<String> {
    let path = format!("/utxo/byIdBinary/{}", box_id);
    let response = node_get(node_url, api_key, &path)
        .call()
        .map_err(|e| anyhow::anyhow!("Ergo node {} failed: {}", path, e))?;
    let body: serde_json::Value = response.into_json()?;
    body["bytes"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing 'bytes' in {} response", path))
}

/// POST payment requests to `/wallet/payment/send`; returns the transaction id.
fn payment_send(
    node_url: &str,
    api_key: Option<&str>,
    requests: &[NodePaymentRequest],
) -> Result<String> {
    let response = node_post(node_url, api_key, "/wallet/payment/send")
        .send_json(requests)
        .map_err(|e| anyhow::anyhow!("Ergo node /wallet/payment/send failed: {}", e))?;
    // The node returns the transaction id as a quoted string.
    let body = response.into_string()?;
    Ok(body.trim().trim_matches('"').to_string())
}

/// Total amount of `token_id` held by the selected boxes.
fn selected_token_sum(selected: &[&ErgoBoxDetails], token_id: &str) -> u64 {
    selected
        .iter()
        .flat_map(|b| b.assets.iter())
        .filter(|t| t.token_id == token_id)
        .map(|t| t.amount)
        .sum()
}

/// Tokens the selected boxes hold beyond what the reserve output needs; these must be
/// returned in an explicit change output.
fn leftover_tokens(
    selected: &[&ErgoBoxDetails],
    token_targets: &[(String, u64)],
) -> Vec<(String, u64)> {
    let mut totals: Vec<(String, u64)> = Vec::new();
    for token in selected.iter().flat_map(|b| b.assets.iter()) {
        match totals.iter_mut().find(|(id, _)| *id == token.token_id) {
            Some((_, total)) => *total += token.amount,
            None => totals.push((token.token_id.clone(), token.amount)),
        }
    }
    totals
        .into_iter()
        .map(|(id, total)| {
            let used = token_targets
                .iter()
                .find(|(id2, _)| *id2 == id)
                .map(|(_, n)| *n)
                .unwrap_or(0);
            (id, total.saturating_sub(used))
        })
        .filter(|(_, left)| *left > 0)
        .collect()
}

/// Pick the boxes the deployment transaction must spend: a P2PK(`p2pk_tree`) proof box
/// first, then funding boxes until the reserve output, fee, and required tokens are
/// covered (with ERG headroom for a token change output when one is needed).
///
/// Returns `Ok(None)` when the wallet holds no P2PK box at all — the caller must create
/// a pre-deployment UTXO first. `Err` means the wallet cannot fund the deployment.
fn select_deployment_boxes(
    wallet_boxes: &[ErgoBoxDetails],
    p2pk_tree: &str,
    erg_target: u64,
    token_targets: &[(String, u64)],
) -> Result<Option<Vec<ErgoBoxDetails>>> {
    let proof_index = match wallet_boxes.iter().position(|b| b.ergo_tree == p2pk_tree) {
        Some(index) => index,
        None => return Ok(None),
    };

    let covers = |selected: &[&ErgoBoxDetails]| -> bool {
        let erg_sum: u64 = selected.iter().map(|b| b.value).sum();
        let erg_ok = erg_sum >= erg_target;
        let tokens_ok = token_targets
            .iter()
            .all(|(id, needed)| selected_token_sum(selected, id) >= *needed);
        // A change output holding leftover tokens also needs some ERG in it.
        let change_ok = leftover_tokens(selected, token_targets).is_empty() || erg_sum > erg_target;
        erg_ok && tokens_ok && change_ok
    };

    let mut selected: Vec<&ErgoBoxDetails> = vec![&wallet_boxes[proof_index]];
    for (index, wallet_box) in wallet_boxes.iter().enumerate() {
        if index == proof_index || covers(&selected) {
            continue;
        }
        selected.push(wallet_box);
    }
    if !covers(&selected) {
        return Err(anyhow::anyhow!(
            "owner wallet cannot fund the reserve deployment: need {} nanoERG plus tokens {:?}, \
             and ERG headroom for change when leftover tokens must be returned",
            erg_target,
            token_targets
        ));
    }
    Ok(Some(selected.into_iter().cloned().collect()))
}

/// Compute the explicit change (ERG remainder plus leftover tokens) for a pinned-input
/// deployment, where the node requires strict input/output balance.
fn deployment_change(
    selected: &[ErgoBoxDetails],
    erg_target: u64,
    token_targets: &[(String, u64)],
) -> Option<(u64, Vec<NodeAsset>)> {
    let refs: Vec<&ErgoBoxDetails> = selected.iter().collect();
    let erg_sum: u64 = selected.iter().map(|b| b.value).sum();
    let change_erg = erg_sum.saturating_sub(erg_target);
    let change_tokens: Vec<NodeAsset> = leftover_tokens(&refs, token_targets)
        .into_iter()
        .map(|(token_id, amount)| NodeAsset { token_id, amount })
        .collect();
    if change_erg == 0 && change_tokens.is_empty() {
        None
    } else {
        Some((change_erg, change_tokens))
    }
}

/// Send a small amount to the owner's own P2PK address so the wallet holds a box the
/// tracker accepts as the reserve-creation proof, and wait for it to appear.
fn create_predeployment_utxo(
    node_url: &str,
    api_key: Option<&str>,
    payload: &ReserveCreationResponse,
    p2pk_tree: &str,
) -> Result<Vec<ErgoBoxDetails>> {
    let request = [NodePaymentRequest {
        address: payload.change_address.clone(),
        value: PRE_DEPLOY_UTXO_VALUE,
        assets: Vec::new(),
        registers: Default::default(),
    }];
    let tx_id = payment_send(node_url, api_key, &request)?;
    for _ in 0..PRE_DEPLOY_POLL_ATTEMPTS {
        let boxes = fetch_wallet_boxes(node_url, api_key)?;
        if boxes.iter().any(|b| b.ergo_tree == p2pk_tree) {
            return Ok(boxes);
        }
        std::thread::sleep(std::time::Duration::from_secs(
            PRE_DEPLOY_POLL_INTERVAL_SECS,
        ));
    }
    Err(anyhow::anyhow!(
        "pre-deployment transaction {} was sent but no P2PK box for the reserve owner \
         appeared in the wallet; is the owner key derived in this node wallet?",
        tx_id
    ))
}

/// Broadcast the deployment with `inputsRaw` pinning the proof box, via
/// `/wallet/transaction/send`; returns the transaction id.
fn send_pinned_deployment(
    node_url: &str,
    api_key: Option<&str>,
    payload: &ReserveCreationResponse,
    payment_requests: &[NodePaymentRequest],
    selected: &[ErgoBoxDetails],
    erg_target: u64,
    token_targets: &[(String, u64)],
) -> Result<String> {
    let mut inputs_raw = Vec::with_capacity(selected.len());
    for selected_box in selected {
        inputs_raw.push(fetch_box_binary(node_url, api_key, &selected_box.box_id)?);
    }
    let mut requests = payment_requests.to_vec();
    if let Some((change_erg, change_tokens)) =
        deployment_change(selected, erg_target, token_targets)
    {
        requests.push(NodePaymentRequest {
            address: payload.change_address.clone(),
            value: change_erg,
            assets: change_tokens,
            registers: Default::default(),
        });
    }
    let body = serde_json::json!({
        "requests": requests,
        "fee": payload.fee,
        "inputsRaw": inputs_raw,
    });
    let response = node_post(node_url, api_key, "/wallet/transaction/send")
        .send_json(&body)
        .map_err(|e| anyhow::anyhow!("Ergo node /wallet/transaction/send failed: {}", e))?;
    let body = response.into_string()?;
    Ok(body.trim().trim_matches('"').to_string())
}

/// Submit a reserve creation payload to the OWNER's Ergo node for broadcast.
///
/// The node signs it with the owner's wallet key, so the owner's wallet must hold the
/// reserve NFT (`nft_id`) and enough ERG for the reserve value and fee. Returns the
/// transaction id.
///
/// The node pays whatever the payload says, so the payload is first checked to be
/// exactly the reserve `request` asked the tracker for (including R5 = `expected_r5`,
/// the tracker's reserve-tree state digest from `GET /reserves/state-digest`, the
/// owner's change address and a bounded fee,
/// [`crate::verify::verify_reserve_payload`]).
///
/// The tracker only admits a reserve root box whose creating transaction spends a P2PK
/// input of the R4 owner key (see `specs/ergo/reserve_admission.md`), so this function
/// guarantees one:
/// - if every wallet box is P2PK(owner), plain `/wallet/payment/send` suffices;
/// - otherwise the inputs are pinned via `inputsRaw` (P2PK(owner) proof box plus funding
///   boxes, with an explicit change output);
/// - if the wallet holds no P2PK(owner) box at all, a pre-deployment transaction first
///   sends 0.001 ERG to the owner's own address, and the pinned deployment spends it.
pub fn submit_reserve_to_node(
    node_url: &str,
    node_api_key: Option<&str>,
    request: &CreateReserveRequest,
    payload: &ReserveCreationResponse,
    expected_r5: &str,
) -> Result<String> {
    crate::verify::verify_reserve_payload(request, payload, None, expected_r5)?;

    let payment_requests: Vec<NodePaymentRequest> = payload
        .requests
        .iter()
        .map(|req| NodePaymentRequest {
            address: req.address.clone(),
            value: req.value,
            assets: req
                .assets
                .iter()
                .map(|a| NodeAsset {
                    token_id: a.token_id.clone(),
                    amount: a.amount,
                })
                .collect(),
            registers: req.registers.clone(),
        })
        .collect();

    let owner_pubkey = payload_owner_pubkey(payload)?;
    let p2pk_tree = p2pk_ergo_tree(&owner_pubkey);

    let mut wallet_boxes = fetch_wallet_boxes(node_url, node_api_key)?;
    if !wallet_boxes.is_empty() && wallet_boxes.iter().all(|b| b.ergo_tree == p2pk_tree) {
        // Any automatic input selection spends only P2PK(owner) inputs.
        return payment_send(node_url, node_api_key, &payment_requests);
    }
    if !wallet_boxes.iter().any(|b| b.ergo_tree == p2pk_tree) {
        wallet_boxes = create_predeployment_utxo(node_url, node_api_key, payload, &p2pk_tree)?;
    }

    let erg_target = payload
        .requests
        .iter()
        .map(|r| r.value)
        .sum::<u64>()
        .saturating_add(payload.fee);
    let mut token_targets: Vec<(String, u64)> = Vec::new();
    for asset in payload.requests.iter().flat_map(|r| r.assets.iter()) {
        match token_targets
            .iter_mut()
            .find(|(id, _)| *id == asset.token_id)
        {
            Some((_, total)) => *total += asset.amount,
            None => token_targets.push((asset.token_id.clone(), asset.amount)),
        }
    }
    let selected = select_deployment_boxes(&wallet_boxes, &p2pk_tree, erg_target, &token_targets)?
        .ok_or_else(|| {
            anyhow::anyhow!("wallet holds no P2PK box for the reserve owner after pre-deployment")
        })?;
    send_pinned_deployment(
        node_url,
        node_api_key,
        payload,
        &payment_requests,
        &selected,
        erg_target,
        &token_targets,
    )
}

/// Reserve-token configuration returned by the tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveTokenConfig {
    pub reserve_token_id: Option<String>,
    pub reserve_token_decimals: u8,
    pub basis_token_reserve_contract_p2s: String,
}

/// One token the tracker serves as reserve collateral.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServedTokenInfo {
    pub token_id: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(default)]
    pub min_collateralization_ratio: Option<f64>,
}

/// One denomination the tracker serves (ERG plus every served token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServedDenomination {
    pub id: String,
    pub symbol: String,
    pub decimals: u8,
}

/// Reserve-collateral configuration: contract addresses plus the served-token table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveCollateralConfig {
    pub erg_contract_p2s: String,
    pub token_contract_p2s: String,
    #[serde(default)]
    pub tokens: Vec<ServedTokenInfo>,
    #[serde(default)]
    pub denominations: Vec<ServedDenomination>,
}

// Tracker signature request/response for redemption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerSignatureRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Legacy field, ignored by the server: the stored note's total debt is signed.
    pub total_debt: u64,
    /// Legacy field, ignored by the server: the stored note's payment timestamp is signed.
    pub timestamp: u64,
    #[serde(default)]
    pub emergency: bool,
    /// Denomination of the note ("erg" or "token:<64-hex>"); default "erg".
    #[serde(default)]
    pub denomination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerSignatureResponse {
    pub success: bool,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub message_signed: String,
    pub is_emergency: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerEvent {
    pub id: u64,
    pub event_type: String,
    pub timestamp: u64,
    pub issuer_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub amount: Option<u64>,
    pub reserve_box_id: Option<String>,
    pub collateral_amount: Option<u64>,
    pub redeemed_amount: Option<u64>,
    pub height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

// Upload policy request/response for acceptance predicates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPolicyRequest {
    pub recipient_pubkey: String,
    pub policy_json: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPolicyResponse {
    pub uploaded_at: u64,
    pub policy_hash: String,
}

// Response for getting a recipient's acceptance policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPolicyResponse {
    pub recipient_pubkey: String,
    pub policy_json: String,
    pub signature: String,
    pub uploaded_at: u64,
}

// Request for checking note acceptance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAcceptanceRequest {
    pub issuer_pubkey: String,
    pub total_debt: u64,
    #[serde(default)]
    pub recipient_pubkey: Option<String>,
    /// Denomination of the note ("erg" or "token:<64-hex>"); default "erg".
    #[serde(default)]
    pub denomination: String,
}

// Response for checking note acceptance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAcceptanceResponse {
    pub acceptable: bool,
    pub reason: Option<String>,
}

#[derive(Debug)]
pub struct TrackerClient {
    base_url: String,
    auth: TrackerAuth,
}

impl TrackerClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            auth: TrackerAuth::None,
        }
    }

    /// Create a client with authentication credentials.
    pub fn with_auth(base_url: String, auth: TrackerAuth) -> Self {
        Self { base_url, auth }
    }

    /// Replace the authentication configuration.
    pub fn set_auth(&mut self, auth: TrackerAuth) {
        self.auth = auth;
    }

    /// Build a `ureq::Request` for a GET call, applying any configured auth.
    fn get(&self, url: &str) -> Result<ureq::Request> {
        self.apply_auth(ureq::get(url), "GET", url, None)
    }

    /// Build a `ureq::Request` for a POST call, applying any configured auth.
    fn post(&self, url: &str, body: &serde_json::Value) -> Result<ureq::Request> {
        self.apply_auth(ureq::post(url), "POST", url, Some(body))
    }

    /// Attach authentication headers to a `ureq::Request` according to the
    /// configured `TrackerAuth` variant.
    fn apply_auth(
        &self,
        request: ureq::Request,
        method: &str,
        full_url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<ureq::Request> {
        match &self.auth {
            // No authentication: pass the request through unchanged.
            TrackerAuth::None => Ok(request),
            // API-key authentication: set a bearer token header.
            TrackerAuth::ApiKey(key) => {
                Ok(request.set("Authorization", &format!("Bearer {}", key)))
            }
            // Signature authentication: build the canonical message, sign it,
            // and attach the pubkey, signature, timestamp, and nonce headers.
            TrackerAuth::Signature {
                pubkey_hex,
                secret_key_hex,
            } => {
                let (path, query) = parse_relative_url(full_url, &self.base_url)?;
                let body_bytes = match body {
                    Some(v) => serde_json::to_vec(v)?,
                    None => Vec::new(),
                };
                let body_hash = hex::encode(sha2::Sha256::digest(&body_bytes));
                let timestamp = current_timestamp_ms();
                let nonce = generate_nonce();
                let canonical = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}",
                    method, path, query, timestamp, nonce, body_hash
                );
                let signature = sign_canonical(&canonical, secret_key_hex, pubkey_hex)?;
                Ok(request
                    .set("X-Signature-Pubkey", pubkey_hex)
                    .set("X-Signature", &signature)
                    .set("X-Signature-Timestamp", &timestamp.to_string())
                    .set("X-Signature-Nonce", &nonce))
            }
        }
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/", self.base_url);
        let response = self.get(&url)?.call()?;

        Ok(response.status() == 200)
    }

    // Note operations
    pub async fn create_note(&self, request: CreateNoteRequest) -> Result<()> {
        let url = format!("{}/notes", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 || response.status() == 201 {
            Ok(())
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to create note: {}", error_text))
        }
    }

    pub async fn get_issuer_notes(&self, pubkey: &str) -> Result<Vec<SerializableIouNote>> {
        let url = format!("{}/notes/issuer/{}", self.base_url, pubkey);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get issuer notes: {}",
                error_text
            ))
        }
    }

    pub async fn get_recipient_notes(&self, pubkey: &str) -> Result<Vec<SerializableIouNote>> {
        let url = format!("{}/notes/recipient/{}", self.base_url, pubkey);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get recipient notes: {}",
                error_text
            ))
        }
    }

    pub async fn get_note(
        &self,
        issuer: &str,
        recipient: &str,
    ) -> Result<Option<SerializableIouNote>> {
        self.get_note_denominated(issuer, recipient, None).await
    }

    /// Get a note, optionally restricted to a denomination ("erg" or "token:<64-hex>").
    pub async fn get_note_denominated(
        &self,
        issuer: &str,
        recipient: &str,
        denomination: Option<&str>,
    ) -> Result<Option<SerializableIouNote>> {
        let mut url = format!(
            "{}/notes/issuer/{}/recipient/{}",
            self.base_url, issuer, recipient
        );
        if let Some(d) = denomination {
            // Validate and normalize before building the URL: the server only
            // accepts "erg" or "token:<64-hex>", and raw user input must not be
            // concatenated into the query string unencoded. Empty means "no
            // restriction" (legacy notes).
            if let Some(id) = canonical_denom_param(Some(d))? {
                url = format!("{}?denomination={}", url, id);
            }
        }
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Option<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or(None))
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get note: {}", error_text))
        }
    }

    // Reserve operations
    pub async fn get_reserve_status(&self, pubkey: &str) -> Result<KeyStatusResponse> {
        let url = format!("{}/key-status/{}", self.base_url, pubkey);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<KeyStatusResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve status: {}",
                error_text
            ))
        }
    }

    // Redemption
    /// Request tracker signature for redemption
    /// Following the Basis protocol specification - POST /tracker/signature
    ///
    /// The server signs the stored note's total debt and timestamp; the
    /// `total_debt` and `timestamp` arguments are sent for backward
    /// compatibility but ignored by the server.
    pub async fn request_tracker_signature(
        &self,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        total_debt: u64,
        timestamp: u64,
        emergency: bool,
        denomination: Option<&str>,
    ) -> Result<TrackerSignatureResponse> {
        // "erg"/"token:<hex>" when known so the server evaluates the acceptance
        // policy against the right reserve; empty keeps the old server default.
        let denomination = canonical_denom_param(denomination)?.unwrap_or_default();
        let request = TrackerSignatureRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            total_debt,
            timestamp,
            emergency,

            denomination,
        };

        let url = format!("{}/tracker/signature", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerSignatureResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to request tracker signature: {}",
                error_text
            ))
        }
    }
}

// Define structs outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionPreparationRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Intended redemption amount. Used by the server only for the
    /// acceptance-policy check; the signed message always covers the stored
    /// note's cumulative debt.
    pub amount: u64,
    /// Legacy field, ignored by the server: the stored note's payment
    /// timestamp is signed.
    pub timestamp: u64,
    /// Denomination of the note ("erg" or "token:<64-hex>"); default "erg".
    #[serde(default)]
    pub denomination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionPreparationResponse {
    pub redemption_id: String,
    pub avl_proof: String,            // Hex-encoded AVL proof
    pub tracker_signature: String,    // Hex-encoded 65-byte Schnorr signature
    pub tracker_pubkey: String,       // Hex-encoded tracker public key
    pub tracker_state_digest: String, // Hex-encoded 33-byte AVL tree root digest
    pub block_height: u64,
    pub tracker_box_id: String, // ID of the tracker box used for the proof
}

// Tracker-assisted 2-phase redemption build/submit (POST /redemption/build, /redemption/submit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionBuildRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    /// Legacy field, ignored by the server: the stored note's payment
    /// timestamp is used for the signature, context extension, and reserve
    /// tree value.
    pub timestamp: u64,
    /// Issuer's 65-byte Schnorr signature (hex) over the redemption signing message.
    pub issuer_signature: String,
    #[serde(default)]
    pub emergency: bool,
    #[serde(default)]
    pub tracker_box_id: Option<String>,
    /// Deprecated: ignored by the tracker, which always pays change to its own wallet.
    #[serde(default)]
    pub change_address: Option<String>,
    /// Denomination of the note ("erg" or "token:<64-hex>"); default "erg".
    #[serde(default)]
    pub denomination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub partial_tx: serde_json::Value,
    pub input_box_binaries: Vec<String>,
    pub data_box_binaries: Vec<String>,
    pub headers: Vec<ergo_lib::ergo_chain_types::Header>,
    pub reserve_box_id: String,
    pub tracker_box_id: String,
    pub reserve_output_value: u64,
    pub recipient_output_value: u64,
    pub total_debt: u64,
    pub change_amount: u64,
    pub change_address: String,
    pub recipient_address: String,
    pub is_first_redemption: bool,
    pub fee: u64,
    /// Cumulative reserve-tree `already_redeemed` proven by the build; round-trip to submit.
    #[serde(default)]
    pub new_already_redeemed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionSubmitRequest {
    pub signed_tx: serde_json::Value,
    /// The exact `unsigned_tx` value returned by `POST /redemption/build`; the server
    /// binds the submit to the pending build entry it identifies (PR #16 B6).
    pub unsigned_tx: serde_json::Value,
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub redeemed_amount: u64,
    pub new_already_redeemed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionSubmitResponse {
    pub tx_id: String,
}

// Tracker proof response for context var #8
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerProofResponse {
    pub key: String,
    pub value: String,
    pub proof: String,
    pub total_debt: u64,
    pub tracker_state_digest: String,
}

// Reserve proof response for context var #5 (insert) and #7 (lookup)
// GET /reserve/proof endpoint response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveProofResponse {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: already_redeemed as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes (None for first redemption) - for context var #7 (lookup)
    pub proof: Option<String>,
    /// Already redeemed amount as integer
    pub already_redeemed: u64,
    /// Whether this is the first redemption (no lookup proof needed)
    pub is_first_redemption: bool,
    /// Hex-encoded AVL insert proof for context var #5 (insert operation)
    /// This proof is used to INSERT the new already_redeemed amount into the reserve tree
    pub insert_proof: String,
    /// Hex-encoded updated reserve state digest after the insert operation (R5 register value)
    pub new_reserve_state_digest: String,
}

/// Canonicalize a denomination argument ("erg" / "token:<64-hex>") for query
/// parameters and request bodies. `None` and the empty string both mean "no
/// restriction", which keeps old behavior for legacy notes whose stored
/// denomination string is empty.
fn canonical_denom_param(denomination: Option<&str>) -> Result<Option<String>> {
    match denomination.filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(d) => {
            let den = basis_store::DenomKey::parse(d)
                .map_err(|e| anyhow::anyhow!("invalid denomination '{}': {}", d, e))?;
            Ok(Some(den.denomination_id()))
        }
    }
}

impl TrackerClient {
    /// Get tracker lookup proof for context var #8.
    ///
    /// `denomination` ("erg" or "token:<64-hex>") scopes the proof to the note's
    /// denomination so multi-denomination trackers return the right debt; the
    /// server resolves an omitted denomination as before.
    pub async fn get_tracker_proof(
        &self,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        denomination: Option<&str>,
    ) -> Result<TrackerProofResponse> {
        let mut url = format!(
            "{}/tracker/proof?issuer_pubkey={}&recipient_pubkey={}",
            self.base_url, issuer_pubkey, recipient_pubkey
        );
        if let Some(id) = canonical_denom_param(denomination)? {
            url = format!("{}&denomination={}", url, id);
        }
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerProofResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get tracker proof: {}",
                error_text
            ))
        }
    }

    /// Get reserve proof for context var #5 (insert) and #7 (lookup).
    ///
    /// `denomination` ("erg" or "token:<64-hex>") scopes the proof to the note's
    /// denomination; the server resolves an omitted denomination as before.
    pub async fn get_reserve_proof(
        &self,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        amount: u64,
        timestamp: u64,
        denomination: Option<&str>,
    ) -> Result<ReserveProofResponse> {
        let mut url = format!(
            "{}/reserve/proof?issuer_pubkey={}&recipient_pubkey={}&amount={}&timestamp={}",
            self.base_url, issuer_pubkey, recipient_pubkey, amount, timestamp
        );
        if let Some(id) = canonical_denom_param(denomination)? {
            url = format!("{}&denomination={}", url, id);
        }
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<ReserveProofResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                // If reserve not found, this might be first redemption
                Err(anyhow::anyhow!(
                    "Reserve record not found: {:?}",
                    api_response.error
                ))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve proof: {}",
                error_text
            ))
        }
    }

    /// Prepare a redemption via POST /redemption/prepare.
    ///
    /// The note must exist in the tracker state. The server signs the stored
    /// note's cumulative debt and payment timestamp; `amount` is used only for
    /// the acceptance-policy check and the `timestamp` sent here is ignored.
    pub async fn prepare_redemption(
        &self,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        amount: u64,
        denomination: Option<&str>,
    ) -> Result<RedemptionPreparationResponse> {
        // "erg"/"token:<hex>" when known so the server's checks run against the
        // right reserve; empty keeps the old server default.
        let denomination = canonical_denom_param(denomination)?.unwrap_or_default();
        let request = RedemptionPreparationRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            amount,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),

            denomination,
        };

        let url = format!("{}/redemption/prepare", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 {
            let api_response: ApiResponse<RedemptionPreparationResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!(
                    "API error during redemption preparation: {:?}",
                    api_response.error
                ))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to prepare redemption: {}",
                error_text
            ))
        }
    }

    /// Build the unsigned redemption transaction on the tracker and have it sign the fee input(s)
    /// locally. Returns the unsigned tx, the fee-signed partial tx, the sigma-serialized input/data
    /// boxes, and the block headers the client needs to add the reserve-input proof.
    /// POST /redemption/build
    pub async fn redemption_build(
        &self,
        request: RedemptionBuildRequest,
    ) -> Result<RedemptionBuildResponse> {
        let url = format!("{}/redemption/build", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = match self.post(&url, &body)?.send_json(body) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let error_text = resp
                    .into_string()
                    .unwrap_or_else(|_| format!("HTTP {}", code));
                return Err(anyhow::anyhow!("redemption build failed: {}", error_text));
            }
            Err(e) => return Err(anyhow::anyhow!("redemption build request failed: {}", e)),
        };

        if response.status() == 200 {
            let api_response: ApiResponse<RedemptionBuildResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!(
                    "redemption build API error: {:?}",
                    api_response.error
                ))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("redemption build failed: {}", error_text))
        }
    }

    /// Broadcast a fully-signed redemption transaction via the tracker. POST /redemption/submit.
    /// Returns the node-accepted transaction id.
    ///
    /// `unsigned_tx` must be the exact value returned by `/redemption/build`; it binds this
    /// submit to the tracker's pending build entry. The wire `redeemed_amount` and
    /// `new_already_redeemed` fields are ignored by the server (it syncs from the stored
    /// build entry) and kept only for wire compatibility.
    pub async fn redemption_submit(
        &self,
        signed_tx: serde_json::Value,
        unsigned_tx: serde_json::Value,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        redeemed_amount: u64,
        new_already_redeemed: u64,
    ) -> Result<String> {
        let url = format!("{}/redemption/submit", self.base_url);
        let request = RedemptionSubmitRequest {
            signed_tx,
            unsigned_tx,
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            redeemed_amount,
            new_already_redeemed,
        };
        let body = serde_json::to_value(request)?;
        let response = match self.post(&url, &body)?.send_json(body) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let error_text = resp
                    .into_string()
                    .unwrap_or_else(|_| format!("HTTP {}", code));
                return Err(anyhow::anyhow!("redemption submit failed: {}", error_text));
            }
            Err(e) => return Err(anyhow::anyhow!("redemption submit request failed: {}", e)),
        };

        if response.status() == 200 {
            let api_response: ApiResponse<RedemptionSubmitResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap().tx_id)
            } else {
                Err(anyhow::anyhow!(
                    "redemption submit API error: {:?}",
                    api_response.error
                ))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("redemption submit failed: {}", error_text))
        }
    }

    // Events & Status
    #[allow(dead_code)]
    pub async fn get_events(&self, page: usize, page_size: usize) -> Result<Vec<TrackerEvent>> {
        let url = format!(
            "{}/events/paginated?page={}&page_size={}",
            self.base_url, page, page_size
        );
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<TrackerEvent>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get events: {}", error_text))
        }
    }

    pub async fn get_recent_events(&self) -> Result<Vec<TrackerEvent>> {
        let url = format!("{}/events", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<TrackerEvent>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get recent events: {}",
                error_text
            ))
        }
    }

    // Reserve operations
    pub async fn create_reserve(
        &self,
        request: CreateReserveRequest,
    ) -> Result<ReserveCreationResponse> {
        let url = format!("{}/reserves/create", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 {
            let api_response: ApiResponse<ReserveCreationResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to create reserve: {}", error_text))
        }
    }

    /// Fetch the tracker's reserve AVL tree state for an (issuer, denomination)
    /// pair — the R5 value a reserve (re)creation must carry (issue #15 review
    /// N9). Used to verify a `/reserves/create` payload before funding it.
    pub async fn get_reserve_state_digest(
        &self,
        issuer_pubkey: &str,
        denomination: &str,
    ) -> Result<ReserveStateDigestResponse> {
        let url = format!(
            "{}/reserves/state-digest?issuer_pubkey={}&denomination={}",
            self.base_url, issuer_pubkey, denomination
        );
        let response = self.get(&url)?.call()?;
        if response.status() == 200 {
            let api_response: ApiResponse<ReserveStateDigestResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve state digest: {}",
                error_text
            ))
        }
    }

    /// Fetch the tracker's reserve-collateral configuration (contract addresses plus
    /// the served-token table).
    pub async fn get_reserve_collateral_config(&self) -> Result<ReserveCollateralConfig> {
        let url = format!("{}/config/reserve-collateral", self.base_url);
        let response = self.get(&url)?.call()?;
        if response.status() == 200 {
            let api_response: ApiResponse<ReserveCollateralConfig> = response.into_json()?;
            if api_response.success {
                return api_response
                    .data
                    .ok_or_else(|| anyhow::anyhow!("missing data in reserve-collateral response"));
            }
            return Err(anyhow::anyhow!("API error: {:?}", api_response.error));
        }
        let error_text = response.into_string()?;
        Err(anyhow::anyhow!(
            "Failed to get reserve collateral config: {}",
            error_text
        ))
    }

    /// Fetch the tracker's reserve-token configuration.
    pub async fn get_reserve_token_config(&self) -> Result<ReserveTokenConfig> {
        let url = format!("{}/config/reserve-token", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<ReserveTokenConfig> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve token config: {}",
                error_text
            ))
        }
    }

    /// Upload acceptance policy to server
    #[allow(dead_code)]
    pub async fn upload_policy(
        &self,
        request: UploadPolicyRequest,
    ) -> Result<UploadPolicyResponse> {
        let url = format!("{}/acceptance/policy", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 {
            let api_response: ApiResponse<UploadPolicyResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to upload policy: {}", error_text))
        }
    }

    /// Get acceptance policy for a recipient from the server
    #[allow(dead_code)]
    pub async fn get_policy(&self, recipient_pubkey: &str) -> Result<GetPolicyResponse> {
        let url = format!("{}/acceptance/policy/{}", self.base_url, recipient_pubkey);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<GetPolicyResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else if response.status() == 404 {
            Err(anyhow::anyhow!("No policy found for this recipient"))
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get policy: {}", error_text))
        }
    }

    /// Check if a note would be accepted by the server's acceptance policy
    #[allow(dead_code)]
    pub async fn check_acceptance(
        &self,
        issuer_pubkey: &str,
        total_debt: u64,
        recipient_pubkey: Option<&str>,
    ) -> Result<CheckAcceptanceResponse> {
        let request = CheckAcceptanceRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            total_debt,
            recipient_pubkey: recipient_pubkey.map(|s| s.to_string()),

            denomination: String::new(),
        };

        let url = format!("{}/acceptance/check", self.base_url);
        let body = serde_json::to_value(request)?;
        let response = self.post(&url, &body)?.send_json(body)?;

        if response.status() == 200 {
            let api_response: ApiResponse<CheckAcceptanceResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to check acceptance: {}",
                error_text
            ))
        }
    }
}

// Define the TrackerBoxIdResponse struct outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerBoxIdResponse {
    pub tracker_box_id: String,
    pub timestamp: u64,
    pub height: u64,
    #[serde(default)]
    pub tracker_nft_id: String,
}

/// Response from `GET /tracker/state`.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerStateResponse {
    pub local_digest: String,
    pub confirmed_digest: Option<String>,
    pub confirmed_box_id: Option<String>,
    pub confirmed_height: Option<u64>,
    pub pending_digest: Option<String>,
    pub pending_tx_id: Option<String>,
    pub pending_submitted_height: Option<u64>,
    pub tracker_box_id: Option<String>,
}

// Define helper structs for API response handling
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlattenedReserveInfo {
    #[serde(alias = "boxId")]
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: Option<String>,
    #[serde(default)]
    pub reserve_token_id: Option<String>,
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
    pub collateralization_ratio: Option<f64>,
    #[serde(default)]
    pub refund_initiation_height: u64,
}

fn decode_box_id(raw: &str) -> String {
    if raw.len() == 128 {
        if let Ok(bytes) = hex::decode(raw) {
            if bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                return String::from_utf8(bytes).unwrap_or_else(|_| raw.to_string());
            }
        }
    }
    raw.to_string()
}

impl From<FlattenedReserveInfo> for basis_store::ExtendedReserveInfo {
    fn from(flattened: FlattenedReserveInfo) -> Self {
        use basis_store::{ExtendedReserveInfo, ReserveInfo};

        let base_info = ReserveInfo {
            collateral_amount: flattened.collateral_amount,
            last_updated_height: flattened.last_updated_height,
            contract_address: String::new(), // Set by get_reserves_by_issuer() after fetching from server config
            tracker_nft_id: flattened.tracker_nft_id.unwrap_or_default(),
            refund_initiation_height: flattened.refund_initiation_height,
            reserve_token_id: flattened.reserve_token_id.unwrap_or_default(),
        };

        ExtendedReserveInfo {
            base_info,
            total_debt: flattened.total_debt,
            box_id: decode_box_id(&flattened.box_id),
            owner_pubkey: flattened.owner_pubkey,
            last_updated_timestamp: flattened.last_updated_timestamp,
        }
    }
}

impl TrackerClient {
    // New methods for the redemption transaction generation

    /// Get reserves for a specific issuer
    pub async fn get_reserves_by_issuer(
        &self,
        pubkey: &str,
    ) -> Result<Vec<basis_store::ExtendedReserveInfo>> {
        let url = format!("{}/reserves/issuer/{}", self.base_url, pubkey);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<FlattenedReserveInfo>> = response.into_json()?;
            if api_response.success {
                let flattened_reserves = api_response.data.unwrap_or_default();

                // Fetch reserve contract P2S address from server config
                let contract_address = match self.get_basis_reserve_contract_p2s().await {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("⚠️  Failed to get reserve contract P2S address: {}", e);
                        String::new()
                    }
                };

                let extended_reserves: Vec<basis_store::ExtendedReserveInfo> = flattened_reserves
                    .into_iter()
                    .map(|flattened| {
                        let mut reserve = basis_store::ExtendedReserveInfo::from(flattened);
                        reserve.base_info.contract_address = contract_address.clone();
                        reserve.box_id = decode_box_id(&reserve.box_id);
                        reserve
                    })
                    .collect();
                Ok(extended_reserves)
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserves for issuer {}: {}",
                pubkey,
                error_text
            ))
        }
    }

    pub async fn get_latest_tracker_box_id(&self) -> Result<TrackerBoxIdResponse> {
        let url = format!("{}/tracker/latest-box-id", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerBoxIdResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else if response.status() == 404 {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("No tracker box found: {}", error_text))
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get latest tracker box ID: {}",
                error_text
            ))
        }
    }

    /// Get the current tracker state: local digest, confirmed on-chain digest/box/height,
    /// and any in-flight pending update transaction.
    #[allow(dead_code)]
    pub async fn get_tracker_state(&self) -> Result<TrackerStateResponse> {
        let url = format!("{}/tracker/state", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerStateResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get tracker state: {}",
                error_text
            ))
        }
    }

    /// Get the Basis reserve contract P2S address from the server configuration
    pub async fn get_basis_reserve_contract_p2s(&self) -> Result<String> {
        let url = format!("{}/config/reserve-contract-p2s", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<String> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve contract P2S address: {}",
                error_text
            ))
        }
    }

    /// Get unspent boxes from the node's wallet.
    /// This follows `/wallet/boxes/unspent` which returns boxes the wallet can sign.
    /// Each item has a nested `box` object containing the actual ErgoBoxDetails.
    pub async fn get_wallet_boxes(
        &self,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<ErgoBoxDetails>> {
        let url = format!(
            "{}/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1",
            node_url.trim_end_matches('/')
        );

        let mut request = ureq::get(&url);
        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            #[derive(Debug, Clone, Serialize, Deserialize)]
            struct WalletBoxEntry {
                #[serde(rename = "box")]
                pub box_details: ErgoBoxDetails,
            }
            let entries: Vec<WalletBoxEntry> = response.into_json()?;
            Ok(entries.into_iter().map(|e| e.box_details).collect())
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get wallet boxes: {}",
                error_text
            ))
        }
    }

    /// Get box details from the Ergo node directly
    pub async fn get_box_from_node(
        &self,
        box_id: &str,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<ErgoBoxDetails> {
        let url = format!("{}/utxo/byId/{}", node_url.trim_end_matches('/'), box_id);
        let mut request_builder = ureq::get(&url);

        // Add API key if provided
        if let Some(key) = api_key {
            request_builder = request_builder.set("api_key", key);
        }

        let response = request_builder.call()?;

        if response.status() == 200 {
            let box_details: ErgoBoxDetails = response.into_json()?;
            Ok(box_details)
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get box from node {}: {}",
                box_id,
                error_text
            ))
        }
    }

    /// Get the hex-encoded serialized bytes of a box from the Ergo node
    /// using the /utxo/byIdBinary/{box_id} endpoint.
    pub async fn get_box_binary(
        &self,
        box_id: &str,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<String> {
        let url = format!(
            "{}/utxo/byIdBinary/{}",
            node_url.trim_end_matches('/'),
            box_id
        );

        let mut request = ureq::get(&url);

        // Add API key if provided
        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            let body: serde_json::Value = response.into_json()?;
            body["bytes"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing 'bytes' field in /utxo/byIdBinary response for box {}",
                        box_id
                    )
                })
        } else {
            Err(anyhow::anyhow!(
                "Failed to get binary box {} from Ergo node: status {}",
                box_id,
                response.status()
            ))
        }
    }

    /// Get current blockchain height from the Ergo node /info endpoint.
    pub async fn get_node_height(&self, node_url: &str, api_key: Option<&str>) -> Result<u32> {
        let url = format!("{}/info", node_url.trim_end_matches('/'));

        let mut request = ureq::get(&url);

        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            let body: serde_json::Value = response.into_json()?;
            body["fullHeight"]
                .as_u64()
                .map(|h| h as u32)
                .ok_or_else(|| anyhow::anyhow!("Missing 'fullHeight' in node /info response"))
        } else {
            Err(anyhow::anyhow!(
                "Failed to get node height: status {}",
                response.status()
            ))
        }
    }

    /// Get the private key for a wallet address from the Ergo node.
    /// This is used to satisfy `proveDlog(receiver)` conditions in the Basis reserve contract.
    pub async fn get_private_key(
        &self,
        node_url: &str,
        api_key: Option<&str>,
        address: &str,
    ) -> Result<String> {
        let url = format!("{}/wallet/getPrivateKey", node_url.trim_end_matches('/'));
        let mut request = ureq::post(&url);

        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let request_body = serde_json::json!({ "address": address });
        let response = request.send_json(request_body)?;

        if response.status() == 200 {
            let secret: String = response.into_json()?;
            Ok(secret)
        } else if response.status() == 404 {
            let error_text = response.into_string().unwrap_or_default();
            Err(anyhow::anyhow!(
                "Address {} not found in node wallet: {}",
                address,
                error_text
            ))
        } else {
            let error_text = response.into_string().unwrap_or_default();
            Err(anyhow::anyhow!(
                "Failed to get private key for {}: {}",
                address,
                error_text
            ))
        }
    }

    pub async fn get_all_notes(&self) -> Result<Vec<SerializableIouNoteWithAge>> {
        let url = format!("{}/notes", self.base_url);
        let response = self.get(&url)?.call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNoteWithAge>> =
                response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get all notes: {}", error_text))
        }
    }
}

// -------------------------------------------------------------------------
// Tracker request auth helpers
// -------------------------------------------------------------------------

/// Extract the relative path and query string from a full request URL.
///
/// The canonical signing message on the server uses the path relative to the
/// base URL, so the client must do the same.
fn parse_relative_url(full_url: &str, base_url: &str) -> Result<(String, String)> {
    let relative = full_url
        .strip_prefix(base_url.trim_end_matches('/'))
        .unwrap_or(full_url);
    let relative = relative.strip_prefix('/').unwrap_or(relative);
    match relative.split_once('?') {
        Some((path, query)) => Ok((format!("/{}", path), query.to_string())),
        None => Ok((format!("/{}", relative), String::new())),
    }
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn generate_nonce() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Sign the canonical request string with the configured secp256k1 key.
fn sign_canonical(canonical: &str, secret_key_hex: &str, pubkey_hex: &str) -> Result<String> {
    let secret_bytes = hex::decode(secret_key_hex)
        .map_err(|e| anyhow::anyhow!("Invalid secret key hex: {}", e))?;
    let secret_key = secp256k1::SecretKey::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid secret key: {}", e))?;

    let pubkey_bytes =
        hex::decode(pubkey_hex).map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;
    if pubkey_bytes.len() != 33 {
        return Err(anyhow::anyhow!(
            "Invalid public key length: expected 33 bytes, got {}",
            pubkey_bytes.len()
        ));
    }
    let mut pubkey_array = [0u8; 33];
    pubkey_array.copy_from_slice(&pubkey_bytes);

    let signature =
        basis_offchain::schnorr::schnorr_sign(canonical.as_bytes(), &secret_key, &pubkey_array)
            .map_err(|_| anyhow::anyhow!("Failed to sign request"))?;

    Ok(hex::encode(signature))
}

#[cfg(test)]
mod auth_helpers_tests {
    use super::*;

    // ------------------------------------------------------------------
    // Denomination parameter canonicalization for redemption-path helpers
    // ------------------------------------------------------------------

    #[test]
    fn canonical_denom_param_none_and_empty_mean_unrestricted() {
        assert_eq!(canonical_denom_param(None).unwrap(), None);
        assert_eq!(canonical_denom_param(Some("")).unwrap(), None);
        // Legacy/"no restriction" whitespace tolerance stays an error, not silent ERG.
        assert!(canonical_denom_param(Some(" ")).is_err());
    }

    #[test]
    fn canonical_denom_param_normalizes_known_ids() {
        assert_eq!(canonical_denom_param(Some("erg")).unwrap().unwrap(), "erg");
        assert_eq!(canonical_denom_param(Some("ERG")).unwrap().unwrap(), "erg");
        let token_hex = "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669";
        assert_eq!(
            canonical_denom_param(Some(&format!("token:{token_hex}")))
                .unwrap()
                .unwrap(),
            format!("token:{token_hex}")
        );
        // Canonical output is URL-safe: no raw user text reaches the query string.
        assert!(canonical_denom_param(Some("erg&allow=1")).is_err());
    }

    #[test]
    fn parse_relative_url_splits_query() {
        assert_eq!(
            parse_relative_url(
                "http://localhost:3048/notes?foo=bar",
                "http://localhost:3048"
            )
            .unwrap(),
            ("/notes".to_string(), "foo=bar".to_string())
        );
    }

    #[test]
    fn parse_relative_url_no_query() {
        assert_eq!(
            parse_relative_url("http://localhost:3048/notes", "http://localhost:3048").unwrap(),
            ("/notes".to_string(), String::new())
        );
    }
}

// -------------------------------------------------------------------------
// Reserve deployment helpers (P2PK proof input guarantee)
// -------------------------------------------------------------------------

#[cfg(test)]
mod reserve_deploy_tests {
    use super::*;

    const OWNER_PK: &str = "02a7955281885bf0f0ca4a48678848cad8dc5b328ce8bc1d4481d041c98e891ff3";

    fn test_payload() -> ReserveCreationResponse {
        let mut registers = std::collections::HashMap::new();
        registers.insert("R4".to_string(), format!("07{}", OWNER_PK));
        ReserveCreationResponse {
            requests: vec![ReservePaymentRequest {
                address: "reserveP2S".to_string(),
                value: 10_000_000,
                assets: vec![Asset {
                    token_id: "aa".repeat(32),
                    amount: 1,
                }],
                registers,
            }],
            fee: 1_000_000,
            change_address: "ownerAddress".to_string(),
        }
    }

    fn wallet_box(
        box_id: &str,
        ergo_tree: &str,
        value: u64,
        tokens: Vec<(&str, u64)>,
    ) -> ErgoBoxDetails {
        ErgoBoxDetails {
            box_id: box_id.to_string(),
            value,
            ergo_tree: ergo_tree.to_string(),
            assets: tokens
                .into_iter()
                .map(|(token_id, amount)| Token {
                    token_id: token_id.to_string(),
                    amount,
                })
                .collect(),
            additional_registers: Default::default(),
            creation_height: 1,
            transaction_id: "bb".repeat(32),
            index: 0,
        }
    }

    #[test]
    fn payload_owner_pubkey_strips_type_prefix() {
        assert_eq!(payload_owner_pubkey(&test_payload()).unwrap(), OWNER_PK);
        let mut bad = test_payload();
        bad.requests[0].registers.clear();
        assert!(payload_owner_pubkey(&bad).is_err());
    }

    #[test]
    fn select_returns_none_without_proof_box() {
        let boxes = vec![wallet_box(
            "b1",
            "0008cdother",
            100_000_000,
            vec![(&"aa".repeat(32), 1)],
        )];
        let targets = vec![("aa".repeat(32), 1u64)];
        assert!(
            select_deployment_boxes(&boxes, &p2pk_ergo_tree(OWNER_PK), 11_000_000, &targets)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn select_puts_proof_box_first_and_covers_targets() {
        let tree = p2pk_ergo_tree(OWNER_PK);
        let nft = "aa".repeat(32);
        let boxes = vec![
            wallet_box("funding", "0008cdother", 100_000_000, vec![(&nft, 1)]),
            wallet_box("proof", &tree, 1_000_000, vec![]),
        ];
        let targets = vec![(nft.clone(), 1u64)];
        let selected = select_deployment_boxes(&boxes, &tree, 11_000_000, &targets)
            .unwrap()
            .unwrap();
        assert_eq!(selected[0].box_id, "proof");
        assert!(selected.iter().any(|b| b.box_id == "funding"));

        let change = deployment_change(&selected, 11_000_000, &targets).unwrap();
        assert_eq!(change.0, 101_000_000 - 11_000_000);
        assert!(change.1.is_empty());
    }

    #[test]
    fn select_errors_when_tokens_uncoverable() {
        let tree = p2pk_ergo_tree(OWNER_PK);
        let boxes = vec![wallet_box("proof", &tree, 100_000_000, vec![])];
        let targets = vec![("aa".repeat(32), 1u64)];
        assert!(select_deployment_boxes(&boxes, &tree, 11_000_000, &targets).is_err());
    }

    #[test]
    fn select_adds_box_for_token_change_headroom() {
        let tree = p2pk_ergo_tree(OWNER_PK);
        let nft = "aa".repeat(32);
        let stray = "cc".repeat(32);
        // Proof box exactly covers ERG target and carries the NFT plus a stray token;
        // the stray token forces a change output, so another box must add ERG headroom.
        let boxes = vec![
            wallet_box("proof", &tree, 11_000_000, vec![(&nft, 1), (&stray, 5)]),
            wallet_box("extra", &tree, 2_000_000, vec![]),
        ];
        let targets = vec![(nft, 1u64)];
        let selected = select_deployment_boxes(&boxes, &tree, 11_000_000, &targets)
            .unwrap()
            .unwrap();
        assert_eq!(selected.len(), 2);

        let change = deployment_change(&selected, 11_000_000, &targets).unwrap();
        assert_eq!(change.0, 2_000_000);
        assert_eq!(change.1.len(), 1);
        assert_eq!(change.1[0].token_id, stray);
        assert_eq!(change.1[0].amount, 5);
    }

    #[test]
    fn change_is_none_on_exact_balance() {
        let tree = p2pk_ergo_tree(OWNER_PK);
        let nft = "aa".repeat(32);
        let boxes = vec![wallet_box("proof", &tree, 11_000_000, vec![(&nft, 1)])];
        let targets = vec![(nft, 1u64)];
        assert!(deployment_change(&boxes, 11_000_000, &targets).is_none());
    }
}

// Define the ErgoBoxDetails struct for parsing box data from the Ergo node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBoxDetails {
    #[serde(alias = "boxId")]
    pub box_id: String,
    pub value: u64,
    #[serde(alias = "ergoTree")]
    pub ergo_tree: String,
    pub assets: Vec<Token>,
    #[serde(alias = "additionalRegisters")]
    pub additional_registers: std::collections::HashMap<String, String>,
    #[serde(alias = "creationHeight")]
    pub creation_height: u32,
    #[serde(alias = "transactionId")]
    pub transaction_id: String,
    pub index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    #[serde(alias = "tokenId")]
    pub token_id: String,
    pub amount: u64,
}

// Define the SerializableIouNoteWithAge struct outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIouNoteWithAge {
    pub issuer_pubkey: String, // Changed from issuer_pubkey to match server response
    pub recipient_pubkey: String, // Changed from recipient_pubkey to match server response
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    /// Tracker NFT ID this note is bound to (server sends it; optional for
    /// compatibility with older trackers).
    #[serde(default)]
    pub tracker_nft_id: String,
    pub age_seconds: u64,
    /// Denomination of the debt: "erg" or "token:<64-hex>". Empty string means
    /// legacy note / older tracker without denomination data (treated as ERG).
    #[serde(default)]
    pub denomination: String,
}

impl SerializableIouNoteWithAge {
    /// Calculate the outstanding debt (amount collected minus amount redeemed)
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }
}
