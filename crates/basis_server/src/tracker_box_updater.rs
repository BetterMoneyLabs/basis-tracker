//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes. Exact node box bytes and one linked state context are
//! validated locally, signed with ergo-lib, and only the signed transaction is sent to the node.

use ergo_lib::chain::{
    ergo_state_context::{ErgoStateContext, Headers},
    parameters::Parameters,
    transaction::{unsigned::UnsignedTransaction, Transaction, UnsignedInput},
};
use ergo_lib::ergo_chain_types::{EcPoint, Header, PreHeader};
use ergo_lib::ergotree_ir::{
    chain::{
        address::Address,
        context_extension::ContextExtension,
        ergo_box::{
            box_value::BoxValue, ErgoBox, ErgoBoxCandidate, NonMandatoryRegisterId,
            NonMandatoryRegisters,
        },
    },
    ergo_tree::ErgoTree,
    mir::constant::{Constant, TryExtractInto},
    serialization::SigmaSerializable,
    sigma_protocol::sigma_boolean::ProveDlog,
};
use ergo_lib::wallet::{
    secret_key::SecretKey, tx_builder::new_miner_fee_box, tx_context::TransactionContext, Wallet,
};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

/// Create a default tracker public key that looks realistic (compressed format with proper prefix)
fn create_default_tracker_pubkey() -> [u8; 33] {
    [
        0x02, 0xda, 0xda, 0x81, 0x1a, 0x88, 0x8c, 0xd0, 0xdc, 0x7a, 0x0a, 0x41, 0x73, 0x9a, 0x3a,
        0xd9, 0xb0, 0xf4, 0x27, 0x74, 0x1f, 0xe6, 0xca, 0x19, 0x70, 0x0c, 0xf1, 0xa5, 0x12, 0x00,
        0xc9, 0x6b, 0xf7,
    ]
}

/// Snapshot of the tracker box commitment state, as observed on-chain.
#[derive(Debug, Clone, Default)]
pub struct ConfirmedState {
    /// Confirmed on-chain AVL root digest (R5). `None` until observed.
    pub digest: Option<[u8; 33]>,
    /// Box ID of the confirmed tracker box.
    pub box_id: Option<String>,
    /// Height at which the confirmed box was observed.
    pub height: Option<u64>,
}

/// Snapshot of an in-flight tracker box update transaction.
#[derive(Debug, Clone, Default)]
pub struct PendingState {
    /// Digest that the pending update will commit.
    pub digest: Option<[u8; 33]>,
    /// Transaction ID of the in-flight update.
    pub tx_id: Option<String>,
    /// Height at which the update was submitted.
    pub submitted_height: Option<u64>,
}

/// Shared state for the tracker box updater
#[derive(Debug, Clone)]
pub struct SharedTrackerState {
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
    pub tracker_box_id: Arc<RwLock<Option<String>>>,
    pub tracker_nft_id: Arc<RwLock<Option<String>>>,
    pub confirmed: Arc<RwLock<ConfirmedState>>,
    pub pending: Arc<RwLock<PendingState>>,
    publication_health: basis_store::PublicationHealth,
}

impl SharedTrackerState {
    /// Creates a new SharedTrackerState with a default tracker public key for testing.
    /// This should only be used in tests - production code should use new_with_tracker_key.
    pub fn new() -> Self {
        Self {
            tracker_pubkey: Arc::new(RwLock::new(create_default_tracker_pubkey())),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
            confirmed: Arc::new(RwLock::new(ConfirmedState::default())),
            pending: Arc::new(RwLock::new(PendingState::default())),
            publication_health: basis_store::PublicationHealth::new(),
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
            confirmed: Arc::new(RwLock::new(ConfirmedState::default())),
            pending: Arc::new(RwLock::new(PendingState::default())),
            publication_health: basis_store::PublicationHealth::new(),
        }
    }

    pub fn set_tracker_pubkey(&self, pubkey: [u8; 33]) {
        if let Ok(mut pubkey_lock) = self.tracker_pubkey.write() {
            *pubkey_lock = pubkey;
        }
    }

    pub fn set_tracker_box_id(&self, box_id: String) {
        if let Ok(mut id_lock) = self.tracker_box_id.write() {
            *id_lock = Some(box_id);
        }
    }

    pub fn set_tracker_nft_id(&self, nft_id: String) {
        if let Ok(mut id_lock) = self.tracker_nft_id.write() {
            *id_lock = Some(nft_id);
        }
    }

    /// Shared one-way health signal for the state manager and publisher.
    pub fn publication_health(&self) -> basis_store::PublicationHealth {
        self.publication_health.clone()
    }

    pub fn quarantine_publication(&self) {
        self.publication_health.quarantine();
    }

    pub fn is_publication_healthy(&self) -> bool {
        self.publication_health.is_healthy()
    }

    pub fn get_tracker_pubkey(&self) -> [u8; 33] {
        if let Ok(pubkey_lock) = self.tracker_pubkey.read() {
            *pubkey_lock
        } else {
            [0u8; 33]
        }
    }

    pub fn get_tracker_box_id(&self) -> Option<String> {
        if let Ok(id_lock) = self.tracker_box_id.read() {
            id_lock.clone()
        } else {
            None
        }
    }

    pub fn get_tracker_nft_id(&self) -> Option<String> {
        if let Ok(id_lock) = self.tracker_nft_id.read() {
            id_lock.clone()
        } else {
            None
        }
    }

    /// Record an in-flight update transaction.
    pub fn set_pending(&self, digest: [u8; 33], tx_id: String, submitted_height: u64) {
        if let Ok(mut pending) = self.pending.write() {
            pending.digest = Some(digest);
            pending.tx_id = Some(tx_id);
            pending.submitted_height = Some(submitted_height);
        }
    }

    /// Clear the pending state (after confirmation or revert).
    pub fn clear_pending(&self) {
        if let Ok(mut pending) = self.pending.write() {
            *pending = PendingState::default();
        }
    }

    /// Record the confirmed on-chain state.
    pub fn set_confirmed(&self, digest: [u8; 33], box_id: String, height: u64) {
        if let Ok(mut confirmed) = self.confirmed.write() {
            confirmed.digest = Some(digest);
            confirmed.box_id = Some(box_id);
            confirmed.height = Some(height);
        }
    }

    /// Snapshot the pending state.
    pub fn get_pending(&self) -> PendingState {
        self.pending.read().map(|p| p.clone()).unwrap_or_default()
    }

    /// Snapshot the confirmed state.
    pub fn get_confirmed(&self) -> ConfirmedState {
        self.confirmed.read().map(|c| c.clone()).unwrap_or_default()
    }
}

/// Configuration for the tracker box updater
#[derive(Clone)]
pub struct TrackerBoxUpdateConfig {
    pub node_url: String,
    pub api_key: Option<String>,
    pub update_interval_seconds: u64,
    pub fee: u64,
    pub tracker_secret_key: Option<[u8; 32]>,
}

impl std::fmt::Debug for TrackerBoxUpdateConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackerBoxUpdateConfig")
            .field("node_url", &self.node_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("update_interval_seconds", &self.update_interval_seconds)
            .field("fee", &self.fee)
            .field(
                "tracker_secret_key",
                &self.tracker_secret_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9053".to_string(),
            api_key: None,
            update_interval_seconds: 600,
            fee: 1_000_000,
            tracker_secret_key: None,
        }
    }
}

#[cfg(test)]
mod secret_redaction_tests {
    use super::TrackerBoxUpdateConfig;

    #[test]
    fn updater_config_debug_redacts_all_secrets() {
        let api_sentinel = "sentinel-updater-api-key-do-not-log";
        let config = TrackerBoxUpdateConfig {
            api_key: Some(api_sentinel.to_string()),
            tracker_secret_key: Some([0xab; 32]),
            ..TrackerBoxUpdateConfig::default()
        };

        let rendered = format!("{config:?}");
        assert!(!rendered.contains(api_sentinel));
        assert!(!rendered.contains("171, 171"));
        assert!(rendered.matches("<redacted>").count() >= 2);
    }
}

/// Error type for tracker box updater operations
#[derive(Debug, thiserror::Error)]
pub enum TrackerBoxUpdaterError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),
    #[error("No tracker NFT ID configured")]
    NoTrackerNftId,
    #[error("No tracker box found on chain")]
    NoTrackerBoxFound,
    #[error("Transaction not found on chain: {0}")]
    TransactionNotFound(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("No wallet boxes available to pay transaction fee")]
    NoFeeInputs,
    #[error("Insufficient wallet funds to pay transaction fee: {available} < {required}")]
    InsufficientFeeInputs { available: u64, required: u64 },
    #[error("Tracker signing key is not configured")]
    MissingTrackerSecretKey,
    #[error("Tracker input validation failed: {0}")]
    InputValidation(String),
    #[error("Tracker state context validation failed: {0}")]
    StateContextValidation(String),
    #[error("Tracker transaction arithmetic failed: {0}")]
    ArithmeticError(String),
    #[error("Failed to sign transaction locally: {0}")]
    SigningFailed(String),
    #[error("Broadcast outcome is unknown; tracker publication remains fenced: {0}")]
    BroadcastOutcomeUnknown(String),
}

/// Ergo box as returned by the blockchain API
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErgoBoxApi {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub assets: Vec<AssetApi>,
    pub additional_registers: std::collections::HashMap<String, String>,
    pub creation_height: u32,
}

/// Asset in an Ergo box
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetApi {
    pub token_id: String,
    pub amount: u64,
}

/// Wrapper returned by /wallet/boxes/unspent
#[derive(Debug, serde::Deserialize)]
struct WalletBoxEntry {
    #[serde(rename = "box")]
    pub box_details: ErgoBoxApi,
}

/// Wrapper for /utxo/byIdBinary response
#[derive(Debug, serde::Deserialize)]
struct BoxBinaryResponse {
    pub bytes: String,
}

/// Encode an unsigned integer using Ergo's VLQ encoding.
fn vlq_encode(mut value: usize) -> Vec<u8> {
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

/// Tracker box updater service
pub struct TrackerBoxUpdater;

struct PreparedTrackerUpdate {
    signed_tx: serde_json::Value,
    tx_id: String,
    submitted_height: u64,
}

struct LocalSigningMaterial {
    secret: SecretKey,
    tracker_point: EcPoint,
    p2pk_tree: ErgoTree,
}

struct LocalSigningContext {
    state_context: ErgoStateContext,
    creation_height: u32,
}

impl TrackerBoxUpdater {
    #[doc(hidden)]
    pub fn restored_pending_transaction(
        shared_state: &SharedTrackerState,
    ) -> Result<Option<(String, [u8; 33])>, TrackerBoxUpdaterError> {
        let pending = shared_state.get_pending();
        match (pending.tx_id, pending.digest, pending.submitted_height) {
            (Some(tx_id), Some(digest), Some(_)) => Ok(Some((tx_id, digest))),
            (None, None, None) => Ok(None),
            _ => {
                shared_state.quarantine_publication();
                Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                    "incomplete durable publication receipt at updater startup".to_string(),
                ))
            }
        }
    }

    /// Start the tracker box updater service as an async background task
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_state: SharedTrackerState,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        cmd_tx: Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let mut ticker = interval(Duration::from_secs(config.update_interval_seconds));

        let mut last_submitted_digest: Option<[u8; 33]> = None;
        let mut pending_tx = Self::restored_pending_transaction(&shared_state)?;

        info!(
            "Tracker box updater started with {}s interval",
            config.update_interval_seconds
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = shutdown_rx.recv() => {
                    info!("Tracker box updater received shutdown signal, stopping");
                    return Ok(());
                }
            }

            // Terminal storage quarantine is process-wide for publication. Do
            // not even reconcile an older in-flight commitment after the state
            // manager has lost a trustworthy durable outcome.
            if !shared_state.is_publication_healthy() {
                error!("Tracker state is quarantined; refusing all commitment processing");
                continue;
            }

            if let Some((ref tx_id, expected_digest)) = pending_tx {
                match Self::check_transaction_confirmation(&config, tx_id).await {
                    Ok(true) => {
                        info!("Transaction {} confirmed on chain. Update complete.", tx_id);

                        // Look up the confirming box to record its height/id.
                        let (box_id, height) = Self::fetch_tracker_box_summary(&config, tx_id)
                            .await
                            .unwrap_or_else(|_| (tx_id.clone(), 0));
                        if !Self::confirm_publication(
                            &cmd_tx,
                            tx_id.clone(),
                            box_id.clone(),
                            height,
                        )
                        .await
                        {
                            shared_state.quarantine_publication();
                            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                                "confirmed publication was not durably reconciled".to_string(),
                            ));
                        }
                        last_submitted_digest = Some(expected_digest);
                        shared_state.set_confirmed(expected_digest, box_id, height);
                        shared_state.clear_pending();
                        pending_tx = None;
                    }
                    Ok(false) => {
                        info!(
                            "Transaction {} still pending, waiting for next cycle...",
                            tx_id
                        );
                        continue;
                    }
                    Err(e) => {
                        error!(
                            "Failed to check transaction {} status: {}. Will retry.",
                            tx_id, e
                        );
                        continue;
                    }
                }
            }

            let tracker_nft_id = match shared_state.get_tracker_nft_id() {
                Some(id) => id,
                None => {
                    warn!("No tracker NFT ID configured, skipping update cycle");
                    continue;
                }
            };

            let tracker_pubkey = shared_state.get_tracker_pubkey();

            let tracker_box = match Self::find_tracker_box(&config, &tracker_nft_id).await {
                Ok(box_data) => box_data,
                Err(e) => {
                    error!("Failed to find tracker box: {}", e);
                    continue;
                }
            };

            // Refresh the confirmed box id in shared state from the live node.
            shared_state.set_tracker_box_id(tracker_box.box_id.clone());

            let mut publication_lease = None;
            if let Some(r5_value) = tracker_box.additional_registers.get("R5") {
                if let Ok(r5_bytes) = hex::decode(r5_value) {
                    if r5_bytes.len() >= 34 {
                        let onchain_digest = &r5_bytes[1..34];
                        let mut onchain_digest_arr = [0u8; 33];
                        onchain_digest_arr.copy_from_slice(onchain_digest);

                        // Always record the currently-confirmed on-chain state so
                        // clients can read it via /tracker/state.
                        shared_state.set_confirmed(
                            onchain_digest_arr,
                            tracker_box.box_id.clone(),
                            tracker_box.creation_height as u64,
                        );

                        // The actor validates/reconciles the observed generation
                        // and then remains fenced until this exact external
                        // publication attempt is completed or explicitly
                        // aborted.
                        let tracker_nft_bytes: [u8; 32] = match hex::decode(&tracker_nft_id)
                            .ok()
                            .and_then(|bytes| bytes.try_into().ok())
                        {
                            Some(bytes) => bytes,
                            None => {
                                shared_state.quarantine_publication();
                                error!("Configured tracker NFT is not exactly 32 bytes");
                                continue;
                            }
                        };
                        let lease = if let Some(ref tx) = cmd_tx {
                            let (rtx, rrx) = tokio::sync::oneshot::channel();
                            if tx
                                .send(crate::TrackerCommand::BeginPublication {
                                    tracker_nft_id: tracker_nft_bytes,
                                    observed_root: onchain_digest_arr,
                                    box_id: tracker_box.box_id.clone(),
                                    height: tracker_box.creation_height as u64,
                                    response_tx: rtx,
                                })
                                .await
                                .is_err()
                            {
                                None
                            } else {
                                match rrx.await {
                                    Ok(Ok(lease)) => Some(lease),
                                    _ => None,
                                }
                            }
                        } else {
                            None
                        };
                        let lease = match lease {
                            Some(lease) => lease,
                            None => {
                                shared_state.quarantine_publication();
                                error!("Tracker actor refused the publication fence");
                                continue;
                            }
                        };
                        if lease.digest == [0u8; 33] {
                            shared_state.quarantine_publication();
                            error!("Tracker actor returned an uninitialized publication digest");
                            continue;
                        }
                        let current_digest = lease.digest;
                        publication_lease = Some(lease);

                        if onchain_digest == current_digest.as_slice() {
                            info!("On-chain tracker box already has current AVL root digest");
                            last_submitted_digest = Some(current_digest);
                            if !Self::abort_publication(&cmd_tx, lease).await {
                                shared_state.quarantine_publication();
                                return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                                    "tracker actor did not release a no-op publication fence"
                                        .to_string(),
                                ));
                            }
                            continue;
                        }
                    }
                }
            }

            let publication_lease = match publication_lease {
                Some(lease) => lease,
                None => {
                    shared_state.quarantine_publication();
                    error!(
                    "Tracker box has no valid R5 generation root; refusing commitment publication"
                );
                    continue;
                }
            };
            let current_digest = publication_lease.digest;

            // If a previous submission for a different digest is still pending
            // and never confirmed, skip submitting a new one (the confirmation
            // check at the top of the loop handles the in-flight tx).
            if let Some(last) = last_submitted_digest {
                if last == current_digest {
                    info!("AVL root digest unchanged, skipping redundant update");
                    if !Self::abort_publication(&cmd_tx, publication_lease).await {
                        shared_state.quarantine_publication();
                        return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                            "tracker actor did not release a redundant publication fence"
                                .to_string(),
                        ));
                    }
                    continue;
                }
            }

            let prepared = match Self::prepare_tracker_update(
                &tracker_nft_id,
                &config,
                &tracker_box,
                &tracker_pubkey,
                &current_digest,
            )
            .await
            {
                Ok(prepared) => prepared,
                Err(e) => {
                    error!("Failed to prepare tracker box update: {}", e);
                    if !Self::abort_publication(&cmd_tx, publication_lease).await {
                        shared_state.quarantine_publication();
                        return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                            "tracker actor did not release an unbroadcast publication fence"
                                .to_string(),
                        ));
                    }
                    continue;
                }
            };

            let submitted_height = prepared.submitted_height;
            if !Self::record_publication_attempt(
                &cmd_tx,
                publication_lease,
                prepared.tx_id.clone(),
                submitted_height,
            )
            .await
            {
                shared_state.quarantine_publication();
                return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                    "tracker actor did not durably record the transaction before broadcast"
                        .to_string(),
                ));
            }

            shared_state.set_pending(current_digest, prepared.tx_id.clone(), submitted_height);
            pending_tx = Some((prepared.tx_id.clone(), current_digest));

            match Self::broadcast_transaction(
                &config,
                &prepared.signed_tx,
                &prepared.tx_id,
            )
            .await
            {
                Ok(tx_id) => info!(
                    "Tracker box update submitted. Transaction ID: {}, Box ID: {}. Waiting for confirmation...",
                    tx_id, tracker_box.box_id
                ),
                Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(error)) => {
                    // The expected tx id was durably recorded before the
                    // request. Keep the actor fenced and poll that exact id on
                    // the next cycle (and after restart).
                    warn!(
                        error = %error,
                        tx_id = %prepared.tx_id,
                        "Tracker broadcast outcome is unknown; retaining durable publication fence"
                    );
                }
                Err(error) => {
                    shared_state.quarantine_publication();
                    return Err(error);
                }
            }
        }
    }

    async fn abort_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        lease: crate::PublicationLease,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        if tx
            .send(crate::TrackerCommand::AbortPublication { lease, response_tx })
            .await
            .is_err()
        {
            return false;
        }
        matches!(response_rx.await, Ok(Ok(())))
    }

    async fn record_publication_attempt(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        lease: crate::PublicationLease,
        tx_id: String,
        submitted_height: u64,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        if tx
            .send(crate::TrackerCommand::RecordPublicationAttempt {
                lease,
                tx_id,
                submitted_height,
                response_tx,
            })
            .await
            .is_err()
        {
            return false;
        }
        matches!(response_rx.await, Ok(Ok(_)))
    }

    async fn confirm_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        tx_id: String,
        box_id: String,
        height: u64,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        if tx
            .send(crate::TrackerCommand::ConfirmPublication {
                tx_id,
                box_id,
                height,
                response_tx,
            })
            .await
            .is_err()
        {
            return false;
        }
        matches!(response_rx.await, Ok(Ok(_)))
    }

    /// Fetch a minimal summary (box_id, creation_height) for a tracker box by
    /// spending transaction id. Falls back to the supplied tx id on error.
    async fn fetch_tracker_box_summary(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<(String, u64), TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!(
            "{}/blockchain/transaction/byId/{}",
            config.node_url.trim_end_matches('/'),
            tx_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        // The new tracker box is the first output of the update transaction.
        if let Some(outputs) = body.get("outputs").and_then(|o| o.as_array()) {
            if let Some(first) = outputs.first() {
                let box_id = first
                    .get("boxId")
                    .and_then(|b| b.as_str())
                    .unwrap_or(tx_id)
                    .to_string();
                let height = first
                    .get("creationHeight")
                    .and_then(|h| h.as_u64())
                    .unwrap_or(0);
                return Ok((box_id, height));
            }
        }

        Ok((tx_id.to_string(), 0))
    }

    /// Find the tracker box on chain using the tracker NFT ID
    async fn find_tracker_box(
        config: &TrackerBoxUpdateConfig,
        tracker_nft_id: &str,
    ) -> Result<ErgoBoxApi, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!(
            "{}/blockchain/box/unspent/byTokenId/{}?limit=5",
            config.node_url.trim_end_matches('/'),
            tracker_nft_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text_lossy();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let boxes: Vec<ErgoBoxApi> = response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        if boxes.is_empty() {
            return Err(TrackerBoxUpdaterError::NoTrackerBoxFound);
        }

        if boxes.len() > 1 {
            warn!(
                "Found {} tracker boxes for NFT {} - expected at most 1. \
                 Using the first box (box_id={}).",
                boxes.len(),
                tracker_nft_id,
                boxes[0].box_id
            );
        }

        Ok(boxes.into_iter().next().unwrap())
    }

    /// Fetch wallet-owned unspent boxes from the Ergo node.
    async fn get_wallet_boxes(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<Vec<ErgoBoxApi>, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!(
            "{}/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1",
            config.node_url.trim_end_matches('/')
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text_lossy();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching wallet boxes: {}",
                status, body
            )));
        }

        let entries: Vec<WalletBoxEntry> = response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        Ok(entries.into_iter().map(|e| e.box_details).collect())
    }

    /// Select only token-free fee boxes whose advertised tree is the exact local signer P2PK.
    /// The selected JSON is subsequently rebound field-for-field to canonical Sigma bytes.
    fn select_fee_inputs<'a>(
        wallet_boxes: &'a [ErgoBoxApi],
        required: u64,
        tracker_box_id: &str,
        owner_tree: &ErgoTree,
    ) -> Result<(Vec<&'a ErgoBoxApi>, u64), TrackerBoxUpdaterError> {
        let owner_tree_bytes = owner_tree.sigma_serialize_bytes().map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to serialize fee-owner tree: {error}"
            ))
        })?;
        let mut candidates = wallet_boxes
            .iter()
            .filter(|box_| box_.box_id != tracker_box_id && box_.assets.is_empty())
            .filter(|box_| {
                hex::decode(&box_.ergo_tree)
                    .map(|bytes| bytes == owner_tree_bytes)
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|box_| box_.value);

        if let Some(box_) = candidates.iter().find(|box_| box_.value >= required) {
            return Ok((vec![*box_], box_.value));
        }

        let mut selected = Vec::new();
        let mut total = 0u64;
        for box_ in candidates {
            total = total.checked_add(box_.value).ok_or_else(|| {
                TrackerBoxUpdaterError::ArithmeticError(
                    "fee-input value sum overflowed u64".to_string(),
                )
            })?;
            selected.push(box_);
            if total >= required {
                break;
            }
        }
        Ok((selected, total))
    }

    /// Fetch the hex-encoded serialized bytes of a box from the Ergo node.
    async fn get_box_binary(
        config: &TrackerBoxUpdateConfig,
        box_id: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!(
            "{}/utxo/byIdBinary/{}",
            config.node_url.trim_end_matches('/'),
            box_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text_lossy();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching box binary {}: {}",
                status, box_id, body
            )));
        }

        let binary: BoxBinaryResponse = response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        Ok(binary.bytes)
    }

    /// Fetch exactly ten linked headers and the matching live parameter set from one node tip.
    async fn get_signing_context(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<LocalSigningContext, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let base = config.node_url.trim_end_matches('/');
        let headers_url = format!("{base}/blocks/lastHeaders/10");
        let info_url = format!("{base}/info");

        let mut headers_request = client.get(&headers_url);
        let mut info_request = client.get(&info_url);
        if let Some(ref api_key) = config.api_key {
            headers_request = headers_request.header("api_key", api_key);
            info_request = info_request.header("api_key", api_key);
        }

        let headers_response = client
            .execute(headers_request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        if !headers_response.status().is_success() {
            let status = headers_response.status();
            let body = headers_response.text_lossy();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching signing headers: {}",
                status, body
            )));
        }
        let headers: Vec<Header> = headers_response.json().map_err(|error| {
            TrackerBoxUpdaterError::StateContextValidation(format!("invalid header JSON: {error}"))
        })?;

        let info_response = client
            .execute(info_request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        if !info_response.status().is_success() {
            let status = info_response.status();
            let body = info_response.text_lossy();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching signing parameters: {}",
                status, body
            )));
        }
        let info: serde_json::Value = info_response.json().map_err(|error| {
            TrackerBoxUpdaterError::StateContextValidation(format!("invalid /info JSON: {error}"))
        })?;
        Self::validate_signing_context(headers, info)
    }

    fn validate_signing_context(
        headers: Vec<Header>,
        info: serde_json::Value,
    ) -> Result<LocalSigningContext, TrackerBoxUpdaterError> {
        if headers.len() != 10 {
            return Err(TrackerBoxUpdaterError::StateContextValidation(format!(
                "expected exactly 10 headers, got {}",
                headers.len()
            )));
        }
        for pair in headers.windows(2) {
            let expected_parent_height = pair[0].height.checked_sub(1).ok_or_else(|| {
                TrackerBoxUpdaterError::StateContextValidation(
                    "header height underflow".to_string(),
                )
            })?;
            if pair[0].parent_id != pair[1].id || pair[1].height != expected_parent_height {
                return Err(TrackerBoxUpdaterError::StateContextValidation(
                    "headers are not one descending parent-linked chain".to_string(),
                ));
            }
        }

        let tip_height = headers[0].height;
        let info_height = info
            .get("fullHeight")
            .and_then(serde_json::Value::as_u64)
            .and_then(|height| u32::try_from(height).ok())
            .ok_or_else(|| {
                TrackerBoxUpdaterError::StateContextValidation(
                    "/info fullHeight is missing or out of range".to_string(),
                )
            })?;
        if info_height != tip_height {
            return Err(TrackerBoxUpdaterError::StateContextValidation(format!(
                "/info and header tip differ: {info_height} != {tip_height}"
            )));
        }
        let info_tip_id = info
            .get("bestFullHeaderId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::StateContextValidation(
                    "/info bestFullHeaderId is missing".to_string(),
                )
            })?;
        if !info_tip_id.eq_ignore_ascii_case(&headers[0].id.to_string()) {
            return Err(TrackerBoxUpdaterError::StateContextValidation(
                "/info and header chain do not share one tip id".to_string(),
            ));
        }

        let parameters_json = info.get("parameters").cloned().ok_or_else(|| {
            TrackerBoxUpdaterError::StateContextValidation(
                "/info has no parameters object".to_string(),
            )
        })?;
        let parameters_height = parameters_json
            .get("height")
            .and_then(serde_json::Value::as_u64)
            .and_then(|height| u32::try_from(height).ok())
            .ok_or_else(|| {
                TrackerBoxUpdaterError::StateContextValidation(
                    "/info parameter height is missing or out of range".to_string(),
                )
            })?;
        if parameters_height > tip_height {
            return Err(TrackerBoxUpdaterError::StateContextValidation(
                "/info parameter height is ahead of the pinned tip".to_string(),
            ));
        }
        let parameters: Parameters = serde_json::from_value(parameters_json).map_err(|error| {
            TrackerBoxUpdaterError::StateContextValidation(format!(
                "/info has no complete parameter set: {error}"
            ))
        })?;
        if parameters.block_version() != i32::from(headers[0].version)
            || parameters.storage_fee_factor() <= 0
            || parameters.min_value_per_byte() <= 0
            || parameters.max_block_size() <= 0
            || parameters.max_block_cost() <= 0
            || parameters.token_access_cost() < 0
            || parameters.input_cost() < 0
            || parameters.data_input_cost() < 0
            || parameters.output_cost() < 0
        {
            return Err(TrackerBoxUpdaterError::StateContextValidation(
                "/info parameters are invalid or not pinned to the header version".to_string(),
            ));
        }

        let headers: Headers = headers.try_into().map_err(|headers: Vec<Header>| {
            TrackerBoxUpdaterError::StateContextValidation(format!(
                "expected exactly 10 headers, got {}",
                headers.len()
            ))
        })?;
        let pre_header = PreHeader::from(headers[0].clone());
        Ok(LocalSigningContext {
            state_context: ErgoStateContext::new(pre_header, headers, parameters),
            creation_height: tip_height,
        })
    }

    fn bind_exact_box(
        advertised: &ErgoBoxApi,
        raw_hex: &str,
    ) -> Result<ErgoBox, TrackerBoxUpdaterError> {
        if raw_hex.is_empty() || raw_hex.len() % 2 != 0 || raw_hex.len() > ErgoBox::MAX_BOX_SIZE * 2
        {
            return Err(TrackerBoxUpdaterError::InputValidation(format!(
                "box {} raw encoding has an invalid length",
                advertised.box_id
            )));
        }
        let raw = hex::decode(raw_hex).map_err(|_| {
            TrackerBoxUpdaterError::InputValidation(format!(
                "box {} raw encoding is not base16",
                advertised.box_id
            ))
        })?;
        let exact = ErgoBox::sigma_parse_bytes(&raw).map_err(|error| {
            TrackerBoxUpdaterError::InputValidation(format!(
                "box {} is not a canonical Sigma box: {error}",
                advertised.box_id
            ))
        })?;
        let canonical = exact.sigma_serialize_bytes().map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to reserialize exact box {}: {error}",
                advertised.box_id
            ))
        })?;
        if canonical != raw {
            return Err(Self::box_mismatch(advertised, "raw canonical bytes"));
        }
        if !exact
            .box_id()
            .to_string()
            .eq_ignore_ascii_case(&advertised.box_id)
        {
            return Err(Self::box_mismatch(advertised, "box id"));
        }
        if *exact.value.as_u64() != advertised.value {
            return Err(Self::box_mismatch(advertised, "value"));
        }
        let tree_bytes = exact.ergo_tree.sigma_serialize_bytes().map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to serialize exact tree {}: {error}",
                advertised.box_id
            ))
        })?;
        if hex::decode(&advertised.ergo_tree)
            .map(|bytes| bytes != tree_bytes)
            .unwrap_or(true)
        {
            return Err(Self::box_mismatch(advertised, "ergo tree"));
        }

        let exact_assets = exact
            .tokens
            .as_ref()
            .map(|tokens| tokens.as_vec().as_slice())
            .unwrap_or_default();
        if exact_assets.len() != advertised.assets.len() {
            return Err(Self::box_mismatch(advertised, "asset cardinality"));
        }
        for (exact_asset, advertised_asset) in exact_assets.iter().zip(&advertised.assets) {
            if hex::decode(&advertised_asset.token_id)
                .map(|bytes| bytes.as_slice() != exact_asset.token_id.as_ref())
                .unwrap_or(true)
                || *exact_asset.amount.as_u64() != advertised_asset.amount
            {
                return Err(Self::box_mismatch(advertised, "ordered assets"));
            }
        }

        let mut exact_registers = std::collections::HashMap::new();
        for register_id in NonMandatoryRegisterId::REG_IDS {
            if let Some(constant) = exact
                .additional_registers
                .get_constant(register_id)
                .map_err(|error| {
                    TrackerBoxUpdaterError::InputValidation(format!(
                        "box {} has an invalid {register_id}: {error}",
                        advertised.box_id
                    ))
                })?
            {
                exact_registers.insert(
                    register_id.to_string(),
                    constant.sigma_serialize_bytes().map_err(|error| {
                        TrackerBoxUpdaterError::SerializationError(format!(
                            "Failed to serialize box {} {register_id}: {error}",
                            advertised.box_id
                        ))
                    })?,
                );
            }
        }
        if exact_registers.len() != advertised.additional_registers.len() {
            return Err(Self::box_mismatch(advertised, "register key set"));
        }
        for (register_id, advertised_value) in &advertised.additional_registers {
            let Some(exact_value) = exact_registers.get(register_id) else {
                return Err(Self::box_mismatch(advertised, "register key set"));
            };
            if hex::decode(advertised_value)
                .map(|bytes| bytes != *exact_value)
                .unwrap_or(true)
            {
                return Err(Self::box_mismatch(advertised, "register bytes"));
            }
        }
        if exact.creation_height != advertised.creation_height {
            return Err(Self::box_mismatch(advertised, "creation height"));
        }
        Ok(exact)
    }

    fn box_mismatch(advertised: &ErgoBoxApi, field: &str) -> TrackerBoxUpdaterError {
        TrackerBoxUpdaterError::InputValidation(format!(
            "box {} JSON/raw {field} mismatch",
            advertised.box_id
        ))
    }

    fn local_signing_material(
        tracker_pubkey: &[u8; 33],
        tracker_secret_key: Option<&[u8; 32]>,
        tracker_box: &ErgoBox,
    ) -> Result<LocalSigningMaterial, TrackerBoxUpdaterError> {
        let tracker_point = EcPoint::sigma_parse_bytes(tracker_pubkey).map_err(|error| {
            TrackerBoxUpdaterError::InputValidation(format!(
                "configured tracker public key is invalid: {error}"
            ))
        })?;
        if tracker_point
            .sigma_serialize_bytes()
            .map(|bytes| bytes.as_slice() != tracker_pubkey)
            .unwrap_or(true)
        {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "configured tracker public key is not canonical".to_string(),
            ));
        }
        let secret_bytes =
            tracker_secret_key.ok_or(TrackerBoxUpdaterError::MissingTrackerSecretKey)?;
        let secret = SecretKey::dlog_from_bytes(secret_bytes).ok_or_else(|| {
            TrackerBoxUpdaterError::InputValidation(
                "configured tracker secret is not a valid dlog scalar".to_string(),
            )
        })?;
        let expected_address = Address::P2Pk(ProveDlog::new(tracker_point.clone()));
        let derived_address = secret.get_address_from_public_image();
        if derived_address != expected_address {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "tracker secret does not match configured public key".to_string(),
            ));
        }
        let p2pk_tree = derived_address.script().map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to derive tracker P2PK tree: {error}"
            ))
        })?;

        let r4 = tracker_box
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R4)
            .map_err(|error| {
                TrackerBoxUpdaterError::InputValidation(format!("tracker R4 is invalid: {error}"))
            })?
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InputValidation("tracker R4 is missing".to_string())
            })?;
        let r4_point: EcPoint = r4.try_extract_into().map_err(|error| {
            TrackerBoxUpdaterError::InputValidation(format!(
                "tracker R4 is not a GroupElement: {error}"
            ))
        })?;
        if r4_point
            .sigma_serialize_bytes()
            .map(|bytes| bytes.as_slice() != tracker_pubkey)
            .unwrap_or(true)
        {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "tracker R4 does not match configured public key".to_string(),
            ));
        }

        Ok(LocalSigningMaterial {
            secret,
            tracker_point,
            p2pk_tree,
        })
    }

    fn validate_input_closure<T>(
        input_ids: impl Iterator<Item = T>,
        exact_boxes: &[ErgoBox],
    ) -> Result<(), TrackerBoxUpdaterError>
    where
        T: ToString,
    {
        let input_ids = input_ids.map(|id| id.to_string()).collect::<Vec<_>>();
        if input_ids.len() != exact_boxes.len() {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "transaction inputs and exact boxes differ in cardinality".to_string(),
            ));
        }
        let mut unique = HashSet::with_capacity(input_ids.len());
        for (input_id, exact_box) in input_ids.iter().zip(exact_boxes) {
            if !unique.insert(input_id.to_ascii_lowercase()) {
                return Err(TrackerBoxUpdaterError::InputValidation(
                    "transaction input ids are not unique".to_string(),
                ));
            }
            if !input_id.eq_ignore_ascii_case(&exact_box.box_id().to_string()) {
                return Err(TrackerBoxUpdaterError::InputValidation(
                    "transaction input order differs from exact box order".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn validate_output_dust(
        unsigned_tx: &UnsignedTransaction,
        parameters: &Parameters,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let min_value_per_byte = u64::try_from(parameters.min_value_per_byte()).map_err(|_| {
            TrackerBoxUpdaterError::StateContextValidation("negative minValuePerByte".to_string())
        })?;
        for (index, candidate) in unsigned_tx.output_candidates.iter().enumerate() {
            let output = ErgoBox::from_box_candidate(candidate, unsigned_tx.id(), index as u16)
                .map_err(|error| {
                    TrackerBoxUpdaterError::SerializationError(format!(
                        "Failed to materialize output {index}: {error}"
                    ))
                })?;
            let size = u64::try_from(
                output
                    .sigma_serialize_bytes()
                    .map_err(|error| {
                        TrackerBoxUpdaterError::SerializationError(format!(
                            "Failed to size output {index}: {error}"
                        ))
                    })?
                    .len(),
            )
            .map_err(|_| {
                TrackerBoxUpdaterError::ArithmeticError("output size does not fit u64".to_string())
            })?;
            let minimum = size.checked_mul(min_value_per_byte).ok_or_else(|| {
                TrackerBoxUpdaterError::ArithmeticError("dust threshold overflowed u64".to_string())
            })?;
            if *candidate.value.as_u64() < minimum {
                return Err(TrackerBoxUpdaterError::InputValidation(format!(
                    "output {index} is dust: {} < {minimum}",
                    candidate.value.as_u64()
                )));
            }
        }
        Ok(())
    }

    fn sign_locally(
        unsigned_tx: UnsignedTransaction,
        exact_boxes: Vec<ErgoBox>,
        material: &LocalSigningMaterial,
        state_context: &ErgoStateContext,
    ) -> Result<Transaction, TrackerBoxUpdaterError> {
        if unsigned_tx.data_inputs.is_some() {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "tracker update must not contain data inputs".to_string(),
            ));
        }
        Self::validate_input_closure(
            unsigned_tx.inputs.iter().map(|input| input.box_id),
            &exact_boxes,
        )?;
        let unsigned_tx_id = unsigned_tx.id();
        let signing_context = TransactionContext::new(unsigned_tx, exact_boxes.clone(), Vec::new())
            .map_err(|error| TrackerBoxUpdaterError::InputValidation(error.to_string()))?;
        let wallet = Wallet::from_secrets(vec![material.secret.clone()]);
        let signed = wallet
            .sign_transaction(signing_context, state_context, None)
            .map_err(|error| TrackerBoxUpdaterError::SigningFailed(error.to_string()))?;
        if signed.data_inputs.is_some() {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "signed tracker update unexpectedly contains data inputs".to_string(),
            ));
        }
        if signed.id() != unsigned_tx_id {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "local signer changed the unsigned transaction intent".to_string(),
            ));
        }
        Self::validate_input_closure(signed.inputs.iter().map(|input| input.box_id), &exact_boxes)?;
        TransactionContext::new(signed.clone(), exact_boxes, Vec::new())
            .map_err(|error| TrackerBoxUpdaterError::InputValidation(error.to_string()))?
            .validate(state_context)
            .map_err(|error| {
                TrackerBoxUpdaterError::SigningFailed(format!(
                    "post-sign transaction validation failed: {error}"
                ))
            })?;
        Ok(signed)
    }

    /// Broadcast a signed transaction to the Ergo node's /transactions endpoint.
    async fn broadcast_transaction(
        config: &TrackerBoxUpdateConfig,
        signed_tx: &serde_json::Value,
        expected_tx_id: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        info!("Broadcasting signed tracker-box update transaction");

        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::BroadcastOutcomeUnknown(e.to_string()))?;
        let url = format!("{}/transactions", config.node_url.trim_end_matches('/'));

        let mut request = client.post(&url).json(signed_tx);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::BroadcastOutcomeUnknown(e.to_string()))?;

        let status = response.status();
        info!(status = %status, "Transaction broadcast request completed");
        let body_text = response.text_lossy();

        Self::parse_broadcast_response(status, &body_text, expected_tx_id)
    }

    fn parse_broadcast_response(
        status: reqwest::StatusCode,
        body_text: &str,
        expected_tx_id: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        // Once the request crossed the network boundary, an HTTP error does not
        // prove the node failed before admission. Keep the actor fence intact
        // until restart/reconciliation rather than authorizing a competing
        // successor transaction.
        if !status.is_success() {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(format!(
                "HTTP {} returned after transaction submission",
                status
            )));
        }

        let body: serde_json::Value = serde_json::from_str(body_text).map_err(|e| {
            TrackerBoxUpdaterError::BroadcastOutcomeUnknown(format!("JSON parse error: {}", e))
        })?;

        // The Ergo node normally returns a JSON string, while some compatible
        // nodes wrap the id. Neither shape is authoritative until it exactly
        // matches the transaction id computed from the signed transaction.
        let returned = match body {
            serde_json::Value::String(tx_id) => Some(tx_id),
            _ => body["id"]
                .as_str()
                .or_else(|| body["txId"].as_str())
                .map(str::to_owned),
        }
        .ok_or_else(|| {
            TrackerBoxUpdaterError::BroadcastOutcomeUnknown("Missing tx id".to_string())
        })?;
        let returned_is_tx_id = returned.len() == 64
            && hex::decode(&returned)
                .map(|bytes| bytes.len() == 32)
                .unwrap_or(false);
        if !returned_is_tx_id || !returned.eq_ignore_ascii_case(expected_tx_id) {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "Node response did not match the locally derived transaction id".to_string(),
            ));
        }
        Ok(expected_tx_id.to_ascii_lowercase())
    }

    fn signed_transaction_id(
        signed_tx: &serde_json::Value,
    ) -> Result<String, TrackerBoxUpdaterError> {
        let declared_tx_id = signed_tx
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::SerializationError(
                    "Signed transaction JSON is missing its transaction id".to_string(),
                )
            })?;
        if declared_tx_id.len() != 64
            || hex::decode(declared_tx_id)
                .map(|bytes| bytes.len() != 32)
                .unwrap_or(true)
        {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "Signed transaction id is not 32 bytes".to_string(),
            ));
        }
        let transaction: Transaction =
            serde_json::from_value(signed_tx.clone()).map_err(|error| {
                TrackerBoxUpdaterError::SerializationError(format!(
                    "Signed transaction JSON is invalid: {}",
                    error
                ))
            })?;
        let tx_id = transaction.id().to_string();
        if tx_id.len() != 64
            || hex::decode(&tx_id)
                .map(|bytes| bytes.len() != 32)
                .unwrap_or(true)
        {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "Derived transaction id is not 32 bytes".to_string(),
            ));
        }
        if !declared_tx_id.eq_ignore_ascii_case(&tx_id) {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "Signed transaction id does not match its serialized body".to_string(),
            ));
        }
        Ok(tx_id)
    }

    /// Bind exact inputs, build the tracker successor, and sign it locally with ergo-lib.
    async fn prepare_tracker_update(
        tracker_nft_id: &str,
        config: &TrackerBoxUpdateConfig,
        tracker_box: &ErgoBoxApi,
        tracker_pubkey: &[u8; 33],
        avl_root_digest: &[u8; 33],
    ) -> Result<PreparedTrackerUpdate, TrackerBoxUpdaterError> {
        let mut r5_bytes = vec![0x64u8];
        r5_bytes.extend_from_slice(avl_root_digest);
        r5_bytes.push(0x03u8); // insert + update allowed (insertOrUpdate contract)
        r5_bytes.extend_from_slice(&vlq_encode(32));
        r5_bytes.extend_from_slice(&vlq_encode(0));
        let r5_constant = Constant::sigma_parse_bytes(&r5_bytes).map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to construct tracker R5: {error}"
            ))
        })?;
        if r5_constant
            .sigma_serialize_bytes()
            .map(|bytes| bytes != r5_bytes)
            .unwrap_or(true)
        {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "Tracker R5 serialization is not canonical".to_string(),
            ));
        }

        let local_context = Self::get_signing_context(config).await?;
        let wallet_boxes = Self::get_wallet_boxes(config).await?;
        let tracker_raw = Self::get_box_binary(config, &tracker_box.box_id).await?;
        let exact_tracker_box = Self::bind_exact_box(tracker_box, &tracker_raw)?;
        let material = Self::local_signing_material(
            tracker_pubkey,
            config.tracker_secret_key.as_ref(),
            &exact_tracker_box,
        )?;

        let tracker_nft = hex::decode(tracker_nft_id).map_err(|_| {
            TrackerBoxUpdaterError::InputValidation(
                "configured tracker NFT id is not base16".to_string(),
            )
        })?;
        if tracker_nft.len() != 32 {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "configured tracker NFT id is not 32 bytes".to_string(),
            ));
        }
        let tracker_tokens = exact_tracker_box
            .tokens
            .as_ref()
            .map(|tokens| tokens.as_vec().as_slice())
            .unwrap_or_default();
        if tracker_tokens.first().map(|token| {
            token.token_id.as_ref() == tracker_nft.as_slice() && *token.amount.as_u64() == 1
        }) != Some(true)
            || tracker_tokens
                .iter()
                .filter(|token| token.token_id.as_ref() == tracker_nft.as_slice())
                .count()
                != 1
        {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "tracker input must carry its singleton NFT first and exactly once".to_string(),
            ));
        }

        let (fee_inputs, advertised_fee_total) = Self::select_fee_inputs(
            &wallet_boxes,
            config.fee,
            &tracker_box.box_id,
            &material.p2pk_tree,
        )?;

        if fee_inputs.is_empty() {
            return Err(TrackerBoxUpdaterError::NoFeeInputs);
        }
        if advertised_fee_total < config.fee {
            return Err(TrackerBoxUpdaterError::InsufficientFeeInputs {
                available: advertised_fee_total,
                required: config.fee,
            });
        }

        let owner_tree_bytes = material
            .p2pk_tree
            .sigma_serialize_bytes()
            .map_err(|error| {
                TrackerBoxUpdaterError::SerializationError(format!(
                    "Failed to serialize fee-owner tree: {error}"
                ))
            })?;
        let mut exact_boxes = vec![exact_tracker_box.clone()];
        let mut exact_fee_total = 0u64;
        for advertised_fee_box in fee_inputs {
            let raw = Self::get_box_binary(config, &advertised_fee_box.box_id).await?;
            let exact_fee_box = Self::bind_exact_box(advertised_fee_box, &raw)?;
            if exact_fee_box.tokens.is_some()
                || exact_fee_box
                    .ergo_tree
                    .sigma_serialize_bytes()
                    .map(|bytes| bytes != owner_tree_bytes)
                    .unwrap_or(true)
            {
                return Err(TrackerBoxUpdaterError::InputValidation(format!(
                    "fee input {} is not token-free exact signer P2PK",
                    advertised_fee_box.box_id
                )));
            }
            exact_fee_total = exact_fee_total
                .checked_add(*exact_fee_box.value.as_u64())
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::ArithmeticError(
                        "exact fee-input value sum overflowed u64".to_string(),
                    )
                })?;
            exact_boxes.push(exact_fee_box);
        }
        if exact_fee_total < config.fee {
            return Err(TrackerBoxUpdaterError::InsufficientFeeInputs {
                available: exact_fee_total,
                required: config.fee,
            });
        }
        let total_input_value = exact_tracker_box
            .value
            .as_u64()
            .checked_add(exact_fee_total)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::ArithmeticError(
                    "tracker and fee-input sum overflowed u64".to_string(),
                )
            })?;
        if total_input_value > BoxValue::MAX_RAW {
            return Err(TrackerBoxUpdaterError::ArithmeticError(
                "tracker and fee-input sum exceeds BoxValue::MAX_RAW".to_string(),
            ));
        }
        let change_amount = exact_fee_total.checked_sub(config.fee).ok_or_else(|| {
            TrackerBoxUpdaterError::ArithmeticError("fee subtraction underflowed".to_string())
        })?;

        let mut register_constants = Vec::new();
        for register_id in NonMandatoryRegisterId::REG_IDS {
            match exact_tracker_box
                .additional_registers
                .get_constant(register_id)
                .map_err(|error| {
                    TrackerBoxUpdaterError::InputValidation(format!(
                        "tracker {register_id} is invalid: {error}"
                    ))
                })? {
                Some(constant) => register_constants.push(constant),
                None => break,
            }
        }
        if register_constants.len() < 2 {
            return Err(TrackerBoxUpdaterError::InputValidation(
                "tracker input must contain R4 and R5".to_string(),
            ));
        }
        register_constants[0] = Constant::from(material.tracker_point.clone());
        register_constants[1] = r5_constant;
        let output_registers =
            NonMandatoryRegisters::try_from(register_constants).map_err(|error| {
                TrackerBoxUpdaterError::SerializationError(format!(
                    "Failed to construct tracker registers: {error}"
                ))
            })?;

        let current_height = local_context.creation_height;
        let tracker_output = ErgoBoxCandidate {
            value: exact_tracker_box.value,
            ergo_tree: exact_tracker_box.ergo_tree.clone(),
            tokens: exact_tracker_box.tokens.clone(),
            additional_registers: output_registers,
            creation_height: current_height,
        };
        let fee_value = BoxValue::new(config.fee).map_err(|error| {
            TrackerBoxUpdaterError::InputValidation(format!(
                "configured fee is not a valid box value: {error}"
            ))
        })?;
        let fee_output = new_miner_fee_box(fee_value, current_height).map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to construct miner-fee output: {error}"
            ))
        })?;
        let mut outputs = vec![tracker_output, fee_output];
        if change_amount > 0 {
            let change_value = BoxValue::new(change_amount).map_err(|error| {
                TrackerBoxUpdaterError::InputValidation(format!(
                    "fee-input change is not a valid box value: {error}"
                ))
            })?;
            outputs.push(ErgoBoxCandidate {
                value: change_value,
                ergo_tree: material.p2pk_tree.clone(),
                tokens: None,
                additional_registers: NonMandatoryRegisters::empty(),
                creation_height: current_height,
            });
        }

        let inputs = exact_boxes
            .iter()
            .map(|box_| UnsignedInput::new(box_.box_id(), ContextExtension::empty()))
            .collect();
        let unsigned_tx =
            UnsignedTransaction::new_from_vec(inputs, Vec::new(), outputs).map_err(|error| {
                TrackerBoxUpdaterError::SerializationError(format!(
                    "Failed to build unsigned tracker update: {error}"
                ))
            })?;
        Self::validate_input_closure(
            unsigned_tx.inputs.iter().map(|input| input.box_id),
            &exact_boxes,
        )?;
        Self::validate_output_dust(&unsigned_tx, &local_context.state_context.parameters)?;

        let signed = Self::sign_locally(
            unsigned_tx,
            exact_boxes,
            &material,
            &local_context.state_context,
        )?;
        let signed_tx = serde_json::to_value(signed).map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "Failed to serialize signed tracker update: {error}"
            ))
        })?;
        let tx_id = Self::signed_transaction_id(&signed_tx)?;
        Ok(PreparedTrackerUpdate {
            signed_tx,
            tx_id,
            submitted_height: u64::from(current_height),
        })
    }

    /// Check if a transaction has been confirmed on-chain by querying the blockchain API
    pub async fn check_transaction_confirmation(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<bool, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!(
            "{}/blockchain/transaction/byId/{}",
            config.node_url.trim_end_matches('/'),
            tx_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        match response.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => {
                let body = response.text_lossy();
                Err(TrackerBoxUpdaterError::HttpError(format!(
                    "HTTP {} checking transaction: {}",
                    status, body
                )))
            }
        }
    }
}

#[cfg(test)]
mod signing_boundary_tests {
    use super::*;
    use ergo_lib::ergotree_ir::{
        chain::{
            ergo_box::BoxTokens,
            token::{Token, TokenAmount, TokenId},
            tx_id::TxId,
        },
        mir::{
            create_provedlog::CreateProveDlog, expr::Expr, extract_reg_as::ExtractRegisterAs,
            global_vars::GlobalVars, option_get::OptionGet, unary_op::OneArgOpTryBuild,
        },
        types::stype::SType,
    };
    use std::str::FromStr;

    const GENERATOR_P2PK_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn signer() -> (SecretKey, [u8; 32], [u8; 33], ErgoTree) {
        let secret_bytes = [1u8; 32];
        let secret = SecretKey::dlog_from_bytes(&secret_bytes).expect("valid scalar");
        let address = secret.get_address_from_public_image();
        let tracker_pubkey = match &address {
            Address::P2Pk(prove_dlog) => prove_dlog
                .h
                .sigma_serialize_bytes()
                .expect("serialize public key")
                .try_into()
                .expect("33-byte public key"),
            _ => panic!("dlog secret must derive P2PK"),
        };
        let p2pk_tree = address.script().expect("derive P2PK tree");
        (secret, secret_bytes, tracker_pubkey, p2pk_tree)
    }

    fn tracker_contract() -> ErgoTree {
        let r4: Expr = ExtractRegisterAs::new(
            GlobalVars::SelfBox.into(),
            NonMandatoryRegisterId::R4 as i8,
            SType::SOption(SType::SGroupElement.into()),
        )
        .expect("R4 extraction")
        .into();
        let r4 = OptionGet::try_build(r4).expect("R4 get").into();
        let proposition: Expr = CreateProveDlog::try_build(r4)
            .expect("proveDlog from R4")
            .into();
        ErgoTree::try_from(proposition).expect("tracker contract tree")
    }

    fn r5_constant(byte: u8) -> Constant {
        let mut bytes = vec![0x64];
        bytes.extend_from_slice(&[byte; 33]);
        bytes.extend_from_slice(&[0x03, 0x20, 0x00]);
        Constant::sigma_parse_bytes(&bytes).expect("AVL constant")
    }

    fn token(id_byte: u8, amount: u64) -> Token {
        Token::from((
            TokenId::from_str(&hex::encode([id_byte; 32])).expect("token id"),
            TokenAmount::try_from(amount).expect("token amount"),
        ))
    }

    fn make_box(
        tree: ErgoTree,
        value: u64,
        tokens: Vec<Token>,
        registers: Vec<Constant>,
        height: u32,
        tx_byte: u8,
        index: u16,
    ) -> ErgoBox {
        ErgoBox::new(
            BoxValue::new(value).expect("box value"),
            tree,
            if tokens.is_empty() {
                None
            } else {
                Some(BoxTokens::try_from(tokens).expect("box tokens"))
            },
            NonMandatoryRegisters::try_from(registers).expect("registers"),
            height,
            TxId::from_str(&hex::encode([tx_byte; 32])).expect("tx id"),
            index,
        )
        .expect("box")
    }

    fn tracker_box() -> (ErgoBox, [u8; 32], [u8; 33], [u8; 32], ErgoTree) {
        let (_secret, secret_bytes, pubkey, p2pk_tree) = signer();
        let point = EcPoint::sigma_parse_bytes(&pubkey).expect("public point");
        let nft = [0x11; 32];
        let tracker = make_box(
            tracker_contract(),
            3_000_000,
            vec![token(0x11, 1), token(0x22, 7)],
            vec![Constant::from(point), r5_constant(0x33)],
            90,
            0x44,
            0,
        );
        (tracker, nft, pubkey, secret_bytes, p2pk_tree)
    }

    fn api_and_raw(box_: &ErgoBox) -> (ErgoBoxApi, String) {
        let value = serde_json::to_value(box_.clone()).expect("box JSON");
        let api = serde_json::from_value(value).expect("API projection");
        let raw = hex::encode(box_.sigma_serialize_bytes().expect("box bytes"));
        (api, raw)
    }

    fn linked_headers() -> Vec<Header> {
        (0..10)
            .map(|index| {
                let id = format!("{:064x}", index + 1);
                let parent_id = if index < 9 {
                    format!("{:064x}", index + 2)
                } else {
                    "00".repeat(32)
                };
                serde_json::from_value(serde_json::json!({
                    "version": 3,
                    "id": id,
                    "parentId": parent_id,
                    "adProofsRoot": "00".repeat(32),
                    "stateRoot": "00".repeat(33),
                    "transactionsRoot": "00".repeat(32),
                    "timestamp": 1_700_000_000_000u64 + index as u64,
                    "nBits": 117586360,
                    "height": 100 - index,
                    "extensionHash": "00".repeat(32),
                    "powSolutions": {
                        "pk": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
                        "n": "0000000000000000"
                    },
                    "votes": "000000",
                    "unparsedBytes": ""
                }))
                .expect("header JSON")
            })
            .collect()
    }

    fn node_info() -> serde_json::Value {
        serde_json::json!({
            "fullHeight": 100,
            "bestFullHeaderId": format!("{:064x}", 1),
            "parameters": {
                "height": 90,
                "blockVersion": 3,
                "storageFeeFactor": 1_250_000,
                "minValuePerByte": 360,
                "maxBlockSize": 1_275_000,
                "maxBlockCost": 8_000_000,
                "tokenAccessCost": 100,
                "inputCost": 2_000,
                "dataInputCost": 100,
                "outputCost": 100
            }
        })
    }

    #[test]
    fn exact_box_binding_rejects_independent_json_and_raw_mutants() {
        let (box_, _, _, _, _) = tracker_box();
        let (api, raw) = api_and_raw(&box_);
        assert_eq!(TrackerBoxUpdater::bind_exact_box(&api, &raw).unwrap(), box_);

        let mut mutants = Vec::new();
        let mut id = api.clone();
        id.box_id = "aa".repeat(32);
        mutants.push(("id", id));
        let mut value = api.clone();
        value.value += 1;
        mutants.push(("value", value));
        let mut tree = api.clone();
        tree.ergo_tree = GENERATOR_P2PK_TREE.to_string();
        mutants.push(("tree", tree));
        let mut asset_id = api.clone();
        asset_id.assets[0].token_id = "bb".repeat(32);
        mutants.push(("asset id", asset_id));
        let mut asset_amount = api.clone();
        asset_amount.assets[0].amount += 1;
        mutants.push(("asset amount", asset_amount));
        let mut asset_order = api.clone();
        asset_order.assets.swap(0, 1);
        mutants.push(("asset order", asset_order));
        let mut register = api.clone();
        register
            .additional_registers
            .insert("R4".to_string(), "0e00".to_string());
        mutants.push(("register", register));
        let mut extra_register = api.clone();
        extra_register
            .additional_registers
            .insert("R9".to_string(), "0101".to_string());
        mutants.push(("register key set", extra_register));
        let mut height = api.clone();
        height.creation_height += 1;
        mutants.push(("height", height));

        for (name, mutant) in mutants {
            assert!(
                TrackerBoxUpdater::bind_exact_box(&mutant, &raw).is_err(),
                "{name} mutant must reject"
            );
        }
        assert!(TrackerBoxUpdater::bind_exact_box(&api, &format!("{raw}00")).is_err());
        assert!(
            TrackerBoxUpdater::bind_exact_box(&api, &"00".repeat(ErgoBox::MAX_BOX_SIZE + 1))
                .is_err()
        );
    }

    #[test]
    fn tracker_authority_binds_secret_pubkey_and_group_element_r4() {
        let (tracker, _, pubkey, secret, _) = tracker_box();
        TrackerBoxUpdater::local_signing_material(&pubkey, Some(&secret), &tracker)
            .expect("matching authority");

        let wrong_secret = [2u8; 32];
        assert!(
            TrackerBoxUpdater::local_signing_material(&pubkey, Some(&wrong_secret), &tracker)
                .is_err()
        );
        assert!(
            TrackerBoxUpdater::local_signing_material(&pubkey, Some(&[0u8; 32]), &tracker).is_err()
        );
        assert!(TrackerBoxUpdater::local_signing_material(&pubkey, None, &tracker).is_err());

        let wrong_type = Constant::from(pubkey.iter().map(|byte| *byte as i8).collect::<Vec<_>>());
        let wrong_r4 = make_box(
            tracker_contract(),
            3_000_000,
            vec![token(0x11, 1)],
            vec![wrong_type, r5_constant(0x33)],
            90,
            0x45,
            0,
        );
        assert!(
            TrackerBoxUpdater::local_signing_material(&pubkey, Some(&secret), &wrong_r4).is_err()
        );

        let (_, _, other_pubkey, _, _) = {
            let other = SecretKey::dlog_from_bytes(&[2u8; 32]).expect("valid scalar");
            let address = other.get_address_from_public_image();
            let other_pubkey: [u8; 33] = match &address {
                Address::P2Pk(p) => p.h.sigma_serialize_bytes().unwrap().try_into().unwrap(),
                _ => unreachable!(),
            };
            (
                other,
                [2u8; 32],
                other_pubkey,
                [0u8; 32],
                address.script().unwrap(),
            )
        };
        assert!(TrackerBoxUpdater::local_signing_material(
            &other_pubkey,
            Some(&[2u8; 32]),
            &tracker
        )
        .is_err());
    }

    #[test]
    fn state_context_requires_ten_linked_headers_and_same_info_pin() {
        TrackerBoxUpdater::validate_signing_context(linked_headers(), node_info())
            .expect("linked state context");

        let mut short = linked_headers();
        short.pop();
        assert!(TrackerBoxUpdater::validate_signing_context(short, node_info()).is_err());
        let mut wrong_parent = linked_headers();
        wrong_parent[0].parent_id = wrong_parent[9].id;
        assert!(TrackerBoxUpdater::validate_signing_context(wrong_parent, node_info()).is_err());
        let mut wrong_height = linked_headers();
        wrong_height[1].height -= 1;
        assert!(TrackerBoxUpdater::validate_signing_context(wrong_height, node_info()).is_err());
        let mut stale_info = node_info();
        stale_info["fullHeight"] = serde_json::json!(99);
        assert!(TrackerBoxUpdater::validate_signing_context(linked_headers(), stale_info).is_err());
        let mut wrong_tip = node_info();
        wrong_tip["bestFullHeaderId"] = serde_json::json!("ff".repeat(32));
        assert!(TrackerBoxUpdater::validate_signing_context(linked_headers(), wrong_tip).is_err());
        let mut incomplete_info = node_info();
        incomplete_info
            .get_mut("parameters")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("maxBlockCost");
        assert!(
            TrackerBoxUpdater::validate_signing_context(linked_headers(), incomplete_info).is_err()
        );
        let mut wrong_version = node_info();
        wrong_version["parameters"]["blockVersion"] = serde_json::json!(4);
        assert!(
            TrackerBoxUpdater::validate_signing_context(linked_headers(), wrong_version).is_err()
        );
        let mut future_parameters = node_info();
        future_parameters["parameters"]["height"] = serde_json::json!(101);
        assert!(
            TrackerBoxUpdater::validate_signing_context(linked_headers(), future_parameters)
                .is_err()
        );
        let mut flat_parameters = node_info();
        let parameters = flat_parameters
            .as_object_mut()
            .unwrap()
            .remove("parameters")
            .unwrap();
        flat_parameters
            .as_object_mut()
            .unwrap()
            .extend(parameters.as_object().unwrap().clone());
        assert!(
            TrackerBoxUpdater::validate_signing_context(linked_headers(), flat_parameters).is_err()
        );
    }

    #[test]
    fn input_closure_rejects_cardinality_order_and_duplicate_ids() {
        let (tracker, _, _, _, p2pk_tree) = tracker_box();
        let fee = make_box(p2pk_tree, 2_000_000, vec![], vec![], 90, 0x55, 0);
        let boxes = vec![tracker.clone(), fee.clone()];
        let ids = vec![tracker.box_id().to_string(), fee.box_id().to_string()];
        TrackerBoxUpdater::validate_input_closure(ids.clone().into_iter(), &boxes).unwrap();
        assert!(
            TrackerBoxUpdater::validate_input_closure(ids[..1].iter().cloned(), &boxes).is_err()
        );
        assert!(
            TrackerBoxUpdater::validate_input_closure(ids.iter().rev().cloned(), &boxes).is_err()
        );
        assert!(TrackerBoxUpdater::validate_input_closure(
            vec![ids[0].clone(), ids[0].clone()].into_iter(),
            &[tracker.clone(), tracker]
        )
        .is_err());
    }

    #[test]
    fn fee_selection_requires_exact_owner_token_free_and_checked_sum() {
        let (tracker, _, _, _, owner_tree) = tracker_box();
        let owner = make_box(owner_tree.clone(), 1_100_000, vec![], vec![], 90, 0x51, 0);
        let other_tree = SecretKey::dlog_from_bytes(&[2u8; 32])
            .expect("other scalar")
            .get_address_from_public_image()
            .script()
            .expect("other P2PK tree");
        let other_owner = make_box(other_tree, 9_000_000, vec![], vec![], 90, 0x52, 0);
        let token_bearing = make_box(
            owner_tree.clone(),
            9_000_000,
            vec![token(0x77, 1)],
            vec![],
            90,
            0x53,
            0,
        );
        let mut advertised = vec![
            api_and_raw(&other_owner).0,
            api_and_raw(&token_bearing).0,
            api_and_raw(&owner).0,
        ];
        let (selected, total) = TrackerBoxUpdater::select_fee_inputs(
            &advertised,
            1_000_000,
            &tracker.box_id().to_string(),
            &owner_tree,
        )
        .expect("one exact owner box");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].box_id, owner.box_id().to_string());
        assert_eq!(total, 1_100_000);

        let owner_tree_hex = hex::encode(owner_tree.sigma_serialize_bytes().unwrap());
        for (index, value) in [u64::MAX - 2, u64::MAX - 1].into_iter().enumerate() {
            advertised.push(ErgoBoxApi {
                box_id: hex::encode([0x80 + index as u8; 32]),
                value,
                ergo_tree: owner_tree_hex.clone(),
                assets: Vec::new(),
                additional_registers: std::collections::HashMap::new(),
                creation_height: 90,
            });
        }
        assert!(matches!(
            TrackerBoxUpdater::select_fee_inputs(
                &advertised[3..],
                u64::MAX,
                &tracker.box_id().to_string(),
                &owner_tree,
            ),
            Err(TrackerBoxUpdaterError::ArithmeticError(_))
        ));
    }

    #[test]
    fn wallet_locally_signs_contract_tracker_and_exact_p2pk_fee_inputs() {
        let (tracker, _, pubkey, secret, p2pk_tree) = tracker_box();
        assert_ne!(
            tracker.ergo_tree, p2pk_tree,
            "tracker is a contract, not P2PK"
        );
        let material =
            TrackerBoxUpdater::local_signing_material(&pubkey, Some(&secret), &tracker).unwrap();
        let local_context =
            TrackerBoxUpdater::validate_signing_context(linked_headers(), node_info()).unwrap();

        for fee_values in [vec![2_000_000], vec![600_000, 600_000]] {
            let fee_boxes = fee_values
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    make_box(
                        p2pk_tree.clone(),
                        value,
                        vec![],
                        vec![],
                        90,
                        0x60 + index as u8,
                        0,
                    )
                })
                .collect::<Vec<_>>();
            let fee_total = fee_boxes
                .iter()
                .map(|box_| *box_.value.as_u64())
                .sum::<u64>();
            let mut boxes = vec![tracker.clone()];
            boxes.extend(fee_boxes);
            let inputs = boxes
                .iter()
                .map(|box_| UnsignedInput::new(box_.box_id(), ContextExtension::empty()))
                .collect();
            let outputs = vec![
                ErgoBoxCandidate {
                    value: tracker.value,
                    ergo_tree: tracker.ergo_tree.clone(),
                    tokens: tracker.tokens.clone(),
                    additional_registers: tracker.additional_registers.clone(),
                    creation_height: local_context.creation_height,
                },
                new_miner_fee_box(
                    BoxValue::new(1_000_000).unwrap(),
                    local_context.creation_height,
                )
                .unwrap(),
                ErgoBoxCandidate {
                    value: BoxValue::new(fee_total - 1_000_000).unwrap(),
                    ergo_tree: p2pk_tree.clone(),
                    tokens: None,
                    additional_registers: NonMandatoryRegisters::empty(),
                    creation_height: local_context.creation_height,
                },
            ];
            let unsigned = UnsignedTransaction::new_from_vec(inputs, Vec::new(), outputs).unwrap();
            TrackerBoxUpdater::validate_output_dust(
                &unsigned,
                &local_context.state_context.parameters,
            )
            .unwrap();
            let signed = TrackerBoxUpdater::sign_locally(
                unsigned,
                boxes,
                &material,
                &local_context.state_context,
            )
            .expect("local contract and P2PK proofs");
            assert!(signed.inputs.iter().all(|input| !input
                .spending_proof
                .proof
                .clone()
                .to_bytes()
                .is_empty()));
            let signed_json = serde_json::to_string(&signed).expect("signed transaction JSON");
            assert!(!signed_json.contains(&hex::encode(secret)));
        }

        let dust_fee = make_box(p2pk_tree.clone(), 1_010_800, vec![], vec![], 90, 0x70, 0);
        let dust_boxes = vec![tracker.clone(), dust_fee];
        let dust_inputs = dust_boxes
            .iter()
            .map(|box_| UnsignedInput::new(box_.box_id(), ContextExtension::empty()))
            .collect();
        let dust_outputs = vec![
            ErgoBoxCandidate {
                value: tracker.value,
                ergo_tree: tracker.ergo_tree.clone(),
                tokens: tracker.tokens.clone(),
                additional_registers: tracker.additional_registers.clone(),
                creation_height: local_context.creation_height,
            },
            new_miner_fee_box(
                BoxValue::new(1_000_000).unwrap(),
                local_context.creation_height,
            )
            .unwrap(),
            ErgoBoxCandidate {
                value: BoxValue::new(10_800).unwrap(),
                ergo_tree: p2pk_tree.clone(),
                tokens: None,
                additional_registers: NonMandatoryRegisters::empty(),
                creation_height: local_context.creation_height,
            },
        ];
        let dust_tx =
            UnsignedTransaction::new_from_vec(dust_inputs, Vec::new(), dust_outputs).unwrap();
        assert!(TrackerBoxUpdater::validate_output_dust(
            &dust_tx,
            &local_context.state_context.parameters
        )
        .is_err());

        let invalid_fee = make_box(p2pk_tree, 2_000_000, vec![], vec![], 90, 0x71, 0);
        let invalid_boxes = vec![tracker.clone(), invalid_fee];
        let invalid_inputs = invalid_boxes
            .iter()
            .map(|box_| UnsignedInput::new(box_.box_id(), ContextExtension::empty()))
            .collect();
        let invalid_outputs = vec![
            ErgoBoxCandidate {
                value: tracker.value,
                ergo_tree: tracker.ergo_tree.clone(),
                tokens: tracker.tokens.clone(),
                additional_registers: tracker.additional_registers.clone(),
                creation_height: local_context.creation_height,
            },
            new_miner_fee_box(
                BoxValue::new(1_000_000).unwrap(),
                local_context.creation_height,
            )
            .unwrap(),
        ];
        let invalid_tx =
            UnsignedTransaction::new_from_vec(invalid_inputs, Vec::new(), invalid_outputs).unwrap();
        assert!(matches!(
            TrackerBoxUpdater::sign_locally(
                invalid_tx,
                invalid_boxes,
                &material,
                &local_context.state_context,
            ),
            Err(TrackerBoxUpdaterError::SigningFailed(message))
                if message.contains("post-sign transaction validation failed")
        ));
    }
}

#[cfg(test)]
mod publication_health_tests {
    use super::{
        AssetApi, ErgoBoxApi, SharedTrackerState, TrackerBoxUpdater, TrackerBoxUpdaterError,
    };
    use ergo_lib::ergotree_ir::{ergo_tree::ErgoTree, serialization::SigmaSerializable};
    use std::collections::HashMap;

    const SIGNED_TRANSACTION_JSON: &str = r#"{
      "id": "9148408c04c2e38a6402a7950d6157730fa7d49e9ab3b9cadec481d7769918e9",
      "inputs": [{
        "boxId": "9126af0675056b80d1fda7af9bf658464dbfa0b128afca7bf7dae18c27fe8456",
        "spendingProof": {"proofBytes": "", "extension": {}}
      }],
      "dataInputs": [],
      "outputs": [{
        "boxId": "b979c439dc698ce5e823b21c722a6e23721af010e4df8c72de0bfd0c3d9ccf6b",
        "value": 74187765000000000,
        "ergoTree": "101004020e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a7017300730110010204020404040004c0fd4f05808c82f5f6030580b8c9e5ae040580f882ad16040204c0944004c0f407040004000580f882ad16d19683030191a38cc7a7019683020193c2b2a57300007473017302830108cdeeac93a38cc7b2a573030001978302019683040193b1a5730493c2a7c2b2a573050093958fa3730673079973089c73097e9a730a9d99a3730b730c0599c1a7c1b2a5730d00938cc7b2a5730e0001a390c1a7730f",
        "assets": [],
        "creationHeight": 284761,
        "additionalRegisters": {},
        "transactionId": "9148408c04c2e38a6402a7950d6157730fa7d49e9ab3b9cadec481d7769918e9",
        "index": 0
      }, {
        "boxId": "e56847ed19b3dc6b72828fcfb992fdf7310828cf291221269b7ffc72fd66706e",
        "value": 67500000000,
        "ergoTree": "100204a00b08cd021dde34603426402615658f1d970cfa7c7bd92ac81a8b16eeebff264d59ce4604ea02d192a39a8cc7a70173007301",
        "assets": [],
        "creationHeight": 284761,
        "additionalRegisters": {},
        "transactionId": "9148408c04c2e38a6402a7950d6157730fa7d49e9ab3b9cadec481d7769918e9",
        "index": 1
      }]
    }"#;

    #[test]
    fn publication_quarantine_is_one_way() {
        let state = SharedTrackerState::new();
        assert!(state.is_publication_healthy());

        state.quarantine_publication();
        assert!(!state.is_publication_healthy());

        state.quarantine_publication();
        assert!(!state.is_publication_healthy());
    }

    #[test]
    fn tracker_fee_selection_rejects_token_bearing_boxes() {
        let token_box = ErgoBoxApi {
            box_id: "11".repeat(32),
            value: 2_000_000,
            ergo_tree: "00".to_string(),
            assets: vec![AssetApi {
                token_id: "22".repeat(32),
                amount: 1,
            }],
            additional_registers: HashMap::new(),
            creation_height: 100,
        };

        let owner_tree = ErgoTree::sigma_parse_bytes(
            &hex::decode(
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            )
            .expect("P2PK hex"),
        )
        .expect("P2PK tree");
        let wallet_boxes = [token_box];
        let (selected, total) =
            TrackerBoxUpdater::select_fee_inputs(&wallet_boxes, 1_000_000, "33", &owner_tree)
                .expect("selection must not overflow");
        assert!(selected.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn updater_restores_a_complete_durable_publication_receipt() {
        let state = SharedTrackerState::new();
        let tx_id = "11".repeat(32);
        let digest = [0x42; 33];
        state.set_pending(digest, tx_id.clone(), 100);

        assert_eq!(
            TrackerBoxUpdater::restored_pending_transaction(&state).unwrap(),
            Some((tx_id, digest))
        );
        assert!(state.is_publication_healthy());
    }

    #[test]
    fn updater_quarantines_an_incomplete_publication_receipt() {
        let state = SharedTrackerState::new();
        state.pending.write().unwrap().tx_id = Some("11".repeat(32));

        assert!(matches!(
            TrackerBoxUpdater::restored_pending_transaction(&state),
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
        ));
        assert!(!state.is_publication_healthy());
    }

    #[test]
    fn non_success_broadcast_response_has_unknown_outcome() {
        let expected = "11".repeat(32);
        assert!(matches!(
            TrackerBoxUpdater::parse_broadcast_response(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                "node failed",
                &expected,
            ),
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
        ));
    }

    #[test]
    fn success_without_transaction_id_has_unknown_outcome() {
        let expected = "11".repeat(32);
        assert!(matches!(
            TrackerBoxUpdater::parse_broadcast_response(reqwest::StatusCode::OK, "{}", &expected,),
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
        ));
    }

    #[test]
    fn empty_or_mismatched_transaction_id_never_releases_publication() {
        let expected = "11".repeat(32);
        for response in ["\"\"".to_string(), format!("\"{}\"", "22".repeat(32))] {
            assert!(matches!(
                TrackerBoxUpdater::parse_broadcast_response(
                    reqwest::StatusCode::OK,
                    &response,
                    &expected,
                ),
                Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
            ));
        }
    }

    #[test]
    fn matching_32_byte_transaction_id_is_accepted() {
        let expected = "11".repeat(32);
        let response = format!("\"{}\"", expected);
        assert_eq!(
            TrackerBoxUpdater::parse_broadcast_response(
                reqwest::StatusCode::OK,
                &response,
                &expected,
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn signed_transaction_id_is_derived_from_the_serialized_body() {
        let signed_tx: serde_json::Value = serde_json::from_str(SIGNED_TRANSACTION_JSON)
            .expect("valid signed transaction fixture");
        assert_eq!(
            TrackerBoxUpdater::signed_transaction_id(&signed_tx).unwrap(),
            "9148408c04c2e38a6402a7950d6157730fa7d49e9ab3b9cadec481d7769918e9"
        );
    }

    #[test]
    fn signed_transaction_rejects_a_forged_embedded_id() {
        let mut signed_tx: serde_json::Value = serde_json::from_str(SIGNED_TRANSACTION_JSON)
            .expect("valid signed transaction fixture");
        signed_tx["id"] = serde_json::Value::String("11".repeat(32));
        assert!(matches!(
            TrackerBoxUpdater::signed_transaction_id(&signed_tx),
            Err(TrackerBoxUpdaterError::SerializationError(_))
        ));
    }

    #[test]
    fn signed_transaction_rejects_a_mutated_body_with_the_old_id() {
        let mut signed_tx: serde_json::Value = serde_json::from_str(SIGNED_TRANSACTION_JSON)
            .expect("valid signed transaction fixture");
        signed_tx["outputs"][0]["value"] = serde_json::json!(74187765000000001u64);
        assert!(matches!(
            TrackerBoxUpdater::signed_transaction_id(&signed_tx),
            Err(TrackerBoxUpdaterError::SerializationError(_))
        ));
    }
}
