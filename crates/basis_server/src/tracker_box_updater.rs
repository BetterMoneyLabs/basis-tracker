//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes by submitting transactions to the Ergo blockchain via the
//! node's /wallet/transaction/sign and /transactions endpoints.

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
    pub avl_root_digest: Arc<RwLock<[u8; 33]>>,
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
    pub tracker_box_id: Arc<RwLock<Option<String>>>,
    pub tracker_nft_id: Arc<RwLock<Option<String>>>,
    pub confirmed: Arc<RwLock<ConfirmedState>>,
    pub pending: Arc<RwLock<PendingState>>,
}

impl SharedTrackerState {
    /// Creates a new SharedTrackerState with a default tracker public key for testing.
    /// This should only be used in tests - production code should use new_with_tracker_key.
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])),
            tracker_pubkey: Arc::new(RwLock::new(create_default_tracker_pubkey())),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
            confirmed: Arc::new(RwLock::new(ConfirmedState::default())),
            pending: Arc::new(RwLock::new(PendingState::default())),
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])),
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
            confirmed: Arc::new(RwLock::new(ConfirmedState::default())),
            pending: Arc::new(RwLock::new(PendingState::default())),
        }
    }

    pub fn set_avl_root_digest(&self, digest: [u8; 33]) {
        if let Ok(mut root_lock) = self.avl_root_digest.write() {
            *root_lock = digest;
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

    pub fn get_avl_root_digest(&self) -> [u8; 33] {
        if let Ok(root_lock) = self.avl_root_digest.read() {
            *root_lock
        } else {
            [0u8; 33]
        }
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
#[derive(Debug, Clone)]
pub struct TrackerBoxUpdateConfig {
    pub node_url: String,
    pub api_key: Option<String>,
    pub update_interval_seconds: u64,
    pub fee: u64,
    pub change_address: Option<String>,
    pub tracker_secret_key: Option<[u8; 32]>,
    /// Minimum on-chain depth (including the inclusion block) before a
    /// tracker box update is treated as confirmed and pending notes are
    /// promoted to `Confirmed`.
    pub min_confirmation_depth: u64,
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9053".to_string(),
            api_key: None,
            update_interval_seconds: 600,
            fee: 1_000_000,
            change_address: None,
            tracker_secret_key: None,
            min_confirmation_depth: 2,
        }
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
    #[error("Failed to sign transaction: {0}")]
    SigningFailed(String),
    #[error("Failed to broadcast transaction: {0}")]
    BroadcastFailed(String),
}

/// Ergo box as returned by the blockchain API
#[derive(Debug, serde::Deserialize, serde::Serialize)]
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

/// Compute the on-chain confirmation depth of a transaction included at
/// `inclusion_height` given the current chain tip `current_height`. Both heights
/// are inclusive, so a transaction in the current tip has depth 1. A height of 0
/// means the inclusion height is unknown and depth is reported as 0.
pub fn confirmation_depth(current_height: u64, inclusion_height: u64) -> u64 {
    if inclusion_height == 0 {
        0
    } else {
        current_height.saturating_sub(inclusion_height) + 1
    }
}

/// Tracker box updater service
pub struct TrackerBoxUpdater;

impl TrackerBoxUpdater {
    /// Start the tracker box updater service as an async background task
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_state: SharedTrackerState,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        cmd_tx: Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let mut ticker = interval(Duration::from_secs(config.update_interval_seconds));

        let mut last_submitted_digest: Option<[u8; 33]> = None;
        let mut pending_tx: Option<(String, [u8; 33])> = None;

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

            if let Some((ref tx_id, expected_digest)) = pending_tx {
                match Self::check_transaction_confirmation(&config, tx_id).await {
                    Ok(true) => {
                        // Look up the confirming box to record its height/id.
                        let (box_id, height) =
                            match Self::fetch_tracker_box_summary(&config, tx_id).await {
                                Ok(summary) => summary,
                                Err(e) => {
                                    error!(
                                        "Failed to fetch confirming box for {}: {}. Will retry.",
                                        tx_id, e
                                    );
                                    continue;
                                }
                            };

                        // Require a minimum on-chain depth (presence in a block
                        // alone is not confirmation) before promoting pending
                        // notes to Confirmed.
                        if config.min_confirmation_depth > 1 {
                            match Self::get_node_height(&config).await {
                                Ok(current) => {
                                    let depth = confirmation_depth(current as u64, height);
                                    if depth < config.min_confirmation_depth {
                                        info!(
                                            "Transaction {} included at height {} (depth {}/{}), waiting for more confirmations...",
                                            tx_id, height, depth, config.min_confirmation_depth
                                        );
                                        continue;
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to fetch node height for depth check of {}: {}. Will retry.",
                                        tx_id, e
                                    );
                                    continue;
                                }
                            }
                        }

                        info!("Transaction {} confirmed on chain. Update complete.", tx_id);
                        last_submitted_digest = Some(expected_digest);

                        shared_state.set_confirmed(expected_digest, box_id.clone(), height);
                        shared_state.clear_pending();
                        pending_tx = None;

                        if let Some(ref tx) = cmd_tx {
                            let (rtx, rrx) = tokio::sync::oneshot::channel();
                            let _ = tx
                                .send(crate::TrackerCommand::ConfirmPendingNotes {
                                    box_id,
                                    height,
                                    response_tx: rtx,
                                })
                                .await;
                            let _ = rrx.await;
                        }
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

            let current_digest = shared_state.get_avl_root_digest();
            let tracker_pubkey = shared_state.get_tracker_pubkey();

            if current_digest == [0u8; 33] {
                info!("AVL root digest not initialized yet, skipping update");
                continue;
            }

            let tracker_box = match Self::find_tracker_box(&config, &tracker_nft_id).await {
                Ok(box_data) => box_data,
                Err(e) => {
                    error!("Failed to find tracker box: {}", e);
                    continue;
                }
            };

            // Refresh the confirmed box id in shared state from the live node.
            shared_state.set_tracker_box_id(tracker_box.box_id.clone());

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

                        // Reconcile per-note confirmation records with the
                        // observed on-chain digest (handles restarts where the
                        // local tree already matches the on-chain commitment).
                        if let Some(ref tx) = cmd_tx {
                            let (rtx, rrx) = tokio::sync::oneshot::channel();
                            let _ = tx
                                .send(crate::TrackerCommand::ReconcileWithConfirmedDigest {
                                    digest: onchain_digest_arr,
                                    box_id: tracker_box.box_id.clone(),
                                    height: tracker_box.creation_height as u64,
                                    response_tx: rtx,
                                })
                                .await;
                            let _ = rrx.await;
                        }

                        if onchain_digest == current_digest.as_slice() {
                            info!("On-chain tracker box already has current AVL root digest");
                            last_submitted_digest = Some(current_digest);
                            continue;
                        }
                    }
                }
            }

            // If a previous submission for a different digest is still pending
            // and never confirmed, skip submitting a new one (the confirmation
            // check at the top of the loop handles the in-flight tx).
            if let Some(last) = last_submitted_digest {
                if last == current_digest {
                    info!("AVL root digest unchanged, skipping redundant update");
                    continue;
                }
            }

            match Self::submit_tracker_update(
                &tracker_nft_id,
                &config,
                &tracker_box,
                &tracker_pubkey,
                &current_digest,
            )
            .await
            {
                Ok(tx_id) => {
                    info!(
                        "Tracker box update submitted. Transaction ID: {}, Box ID: {}. Waiting for confirmation...",
                        tx_id, tracker_box.box_id
                    );

                    let submitted_height = Self::get_node_height(&config).await.unwrap_or(0) as u64;
                    shared_state.set_pending(current_digest, tx_id.clone(), submitted_height);
                    pending_tx = Some((tx_id.clone(), current_digest));

                    if let Some(ref tx) = cmd_tx {
                        let (rtx, rrx) = tokio::sync::oneshot::channel();
                        let _ = tx
                            .send(crate::TrackerCommand::MarkNotesPending {
                                digest: current_digest,
                                tx_id,
                                submitted_height,
                                response_tx: rtx,
                            })
                            .await;
                        let _ = rrx.await;
                    }
                }
                Err(e) => {
                    error!("Failed to submit tracker box update: {}", e);
                }
            }
        }
    }

    /// Fetch a minimal summary (box_id, creation_height) for a tracker box by
    /// spending transaction id. Falls back to the supplied tx id on error.
    async fn fetch_tracker_box_summary(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<(String, u64), TrackerBoxUpdaterError> {
        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/blockchain/transaction/byId/{}",
            config.node_url.trim_end_matches('/'),
            tx_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = basis_store::http::read_json_capped(response)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e))
                })
            })?;

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
        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/blockchain/box/unspent/byTokenId/{}?limit=5",
            config.node_url.trim_end_matches('/'),
            tracker_nft_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let boxes: Vec<ErgoBoxApi> = basis_store::http::read_json_capped(response)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e))
                })
            })?;

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
        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1",
            config.node_url.trim_end_matches('/')
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching wallet boxes: {}",
                status, body
            )));
        }

        let entries: Vec<WalletBoxEntry> = basis_store::http::read_json_capped(response)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e))
                })
            })?;

        Ok(entries.into_iter().map(|e| e.box_details).collect())
    }

    /// Select wallet boxes covering the required fee, excluding the tracker box itself.
    /// Prefers boxes without tokens; falls back to token-bearing boxes and preserves their tokens
    /// in the change output.
    fn select_fee_inputs(
        wallet_boxes: &[ErgoBoxApi],
        required: u64,
        tracker_box_id: &str,
    ) -> (Vec<String>, u64) {
        let candidates: Vec<&ErgoBoxApi> = wallet_boxes
            .iter()
            .filter(|b| b.box_id != tracker_box_id)
            .collect();

        // Try token-free boxes first.
        let mut token_free: Vec<&ErgoBoxApi> = candidates
            .iter()
            .filter(|b| b.assets.is_empty())
            .copied()
            .collect();
        token_free.sort_by_key(|b| b.value);

        if let Some(box_) = token_free.iter().find(|b| b.value >= required) {
            return (vec![box_.box_id.clone()], box_.value);
        }

        let mut selected = Vec::new();
        let mut total = 0u64;
        for box_ in token_free {
            total += box_.value;
            selected.push(box_.box_id.clone());
            if total >= required {
                return (selected, total);
            }
        }

        // Fall back to token-bearing boxes if necessary.
        let mut token_boxes: Vec<&ErgoBoxApi> = candidates
            .iter()
            .filter(|b| !b.assets.is_empty())
            .copied()
            .collect();
        token_boxes.sort_by_key(|b| b.value);

        if let Some(box_) = token_boxes.iter().find(|b| b.value >= required) {
            return (vec![box_.box_id.clone()], box_.value);
        }

        let mut selected = Vec::new();
        let mut total = 0u64;
        for box_ in token_boxes {
            total += box_.value;
            selected.push(box_.box_id.clone());
            if total >= required {
                return (selected, total);
            }
        }

        (selected, total)
    }

    /// Fetch the hex-encoded serialized bytes of a box from the Ergo node.
    async fn get_box_binary(
        config: &TrackerBoxUpdateConfig,
        box_id: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/utxo/byIdBinary/{}",
            config.node_url.trim_end_matches('/'),
            box_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching box binary {}: {}",
                status, box_id, body
            )));
        }

        let binary: BoxBinaryResponse = basis_store::http::read_json_capped(response)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e))
                })
            })?;

        Ok(binary.bytes)
    }

    /// Get the current blockchain height from the Ergo node.
    async fn get_node_height(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<u32, TrackerBoxUpdaterError> {
        let client = basis_store::http::bounded_client();
        let url = format!("{}/info", config.node_url.trim_end_matches('/'));

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "HTTP {} fetching node height: {}",
                status, body
            )));
        }

        let body: serde_json::Value = basis_store::http::read_json_capped(response)
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e))
                })
            })?;

        body["fullHeight"]
            .as_u64()
            .map(|h| h as u32)
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("Missing fullHeight".to_string()))
    }

    /// Sign an unsigned transaction using the Ergo node's /wallet/transaction/sign endpoint.
    async fn sign_transaction(
        config: &TrackerBoxUpdateConfig,
        unsigned_tx: serde_json::Value,
    ) -> Result<serde_json::Value, TrackerBoxUpdaterError> {
        info!("Signing unsigned transaction: {}", unsigned_tx);

        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/wallet/transaction/sign",
            config.node_url.trim_end_matches('/')
        );

        let mut request = client.post(&url).json(&unsigned_tx);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(e.to_string()))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        info!(
            "/wallet/transaction/sign response: status={}, body={}",
            status, body_text
        );

        if !status.is_success() {
            return Err(TrackerBoxUpdaterError::SigningFailed(format!(
                "HTTP {}: {}",
                status, body_text
            )));
        }

        serde_json::from_str(&body_text)
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(format!("JSON parse error: {}", e)))
    }

    /// Broadcast a signed transaction to the Ergo node's /transactions endpoint.
    async fn broadcast_transaction(
        config: &TrackerBoxUpdateConfig,
        signed_tx: &serde_json::Value,
    ) -> Result<String, TrackerBoxUpdaterError> {
        info!("Broadcasting signed transaction: {}", signed_tx);

        let client = basis_store::http::bounded_client();
        let url = format!("{}/transactions", config.node_url.trim_end_matches('/'));

        let mut request = client.post(&url).json(signed_tx);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::BroadcastFailed(e.to_string()))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        info!(
            "/transactions broadcast response: status={}, body={}",
            status, body_text
        );

        if !status.is_success() {
            return Err(TrackerBoxUpdaterError::BroadcastFailed(format!(
                "HTTP {}: {}",
                status, body_text
            )));
        }

        let body: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            TrackerBoxUpdaterError::BroadcastFailed(format!("JSON parse error: {}", e))
        })?;

        // The Ergo node's /transactions endpoint returns the tx id as a plain
        // JSON string (e.g. "abc..."), not an object. Handle both forms.
        match body {
            serde_json::Value::String(tx_id) => Ok(tx_id),
            _ => body["id"]
                .as_str()
                .or_else(|| body["txId"].as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::BroadcastFailed("Missing tx id".to_string())
                }),
        }
    }

    /// Submit a tracker box update transaction via /wallet/transaction/sign
    async fn submit_tracker_update(
        tracker_nft_id: &str,
        config: &TrackerBoxUpdateConfig,
        tracker_box: &ErgoBoxApi,
        tracker_pubkey: &[u8; 33],
        avl_root_digest: &[u8; 33],
    ) -> Result<String, TrackerBoxUpdaterError> {
        let mut r4_bytes = vec![0x07u8];
        r4_bytes.extend_from_slice(tracker_pubkey);
        let r4_value = hex::encode(&r4_bytes);

        let mut r5_bytes = vec![0x64u8];
        r5_bytes.extend_from_slice(avl_root_digest);
        r5_bytes.push(0x03u8); // insert + update allowed (insertOrUpdate contract)
        r5_bytes.extend_from_slice(&vlq_encode(32));
        r5_bytes.extend_from_slice(&vlq_encode(0));
        let r5_value = hex::encode(&r5_bytes);

        let mut output_registers = tracker_box.additional_registers.clone();
        output_registers.insert("R4".to_string(), r4_value);
        output_registers.insert("R5".to_string(), r5_value);

        let change_address = match &config.change_address {
            Some(addr) if !addr.is_empty() => addr.clone(),
            _ => derive_change_address(tracker_pubkey)?,
        };

        let current_height = Self::get_node_height(config).await?;
        let wallet_boxes = Self::get_wallet_boxes(config).await?;

        let (fee_input_ids, fee_input_total) =
            Self::select_fee_inputs(&wallet_boxes, config.fee, &tracker_box.box_id);

        if fee_input_ids.is_empty() {
            return Err(TrackerBoxUpdaterError::NoFeeInputs);
        }

        if fee_input_total < config.fee {
            return Err(TrackerBoxUpdaterError::InsufficientFeeInputs {
                available: fee_input_total,
                required: config.fee,
            });
        }

        let mut inputs = vec![serde_json::json!({
            "boxId": tracker_box.box_id,
            "extension": serde_json::json!({})
        })];
        let mut inputs_raw = vec![Self::get_box_binary(config, &tracker_box.box_id).await?];

        for fee_box_id in &fee_input_ids {
            inputs.push(serde_json::json!({
                "boxId": fee_box_id,
                "extension": serde_json::json!({})
            }));
            inputs_raw.push(Self::get_box_binary(config, fee_box_id).await?);
        }

        let change_amount = fee_input_total.saturating_sub(config.fee);

        // Preserve any tokens from the fee inputs in the change output so they are not burned.
        let change_assets: Vec<serde_json::Value> = fee_input_ids
            .iter()
            .flat_map(|id| {
                wallet_boxes
                    .iter()
                    .find(|b| &b.box_id == id)
                    .map(|b| {
                        b.assets.iter().map(|a| {
                            serde_json::json!({
                                "tokenId": a.token_id,
                                "amount": a.amount
                            })
                        })
                    })
                    .into_iter()
                    .flatten()
            })
            .collect();

        // Ensure the tracker NFT is always the first token in the output tracker box,
        // followed by any other tokens preserved from the input tracker box.
        let mut output_assets = Vec::new();
        let mut other_assets = Vec::new();
        for asset in &tracker_box.assets {
            if asset.token_id == tracker_nft_id {
                output_assets.push(asset.clone());
            } else {
                other_assets.push(asset.clone());
            }
        }
        output_assets.extend(other_assets);

        let mut outputs = vec![
            serde_json::json!({
                "value": tracker_box.value,
                "ergoTree": tracker_box.ergo_tree,
                "creationHeight": current_height,
                "assets": output_assets,
                "additionalRegisters": output_registers
            }),
            serde_json::json!({
                "value": config.fee,
                "ergoTree": "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304",
                "creationHeight": current_height,
                "assets": [],
                "additionalRegisters": {}
            }),
        ];

        if change_amount > 0 {
            outputs.push(serde_json::json!({
                "value": change_amount,
                "ergoTree": change_address_to_ergo_tree(&change_address)?,
                "creationHeight": current_height,
                "assets": change_assets,
                "additionalRegisters": {}
            }));
        }

        let secrets = config
            .tracker_secret_key
            .as_ref()
            .map(|sk| vec![hex::encode(sk)])
            .unwrap_or_default();

        let unsigned_tx = serde_json::json!({
            "tx": {
                "inputs": inputs,
                "dataInputs": [],
                "outputs": outputs
            },
            "inputsRaw": inputs_raw,
            "dataInputsRaw": [],
            "secrets": {
                "dlog": secrets
            }
        });

        let signed_tx = Self::sign_transaction(config, unsigned_tx).await?;
        let tx_id = Self::broadcast_transaction(config, &signed_tx).await?;

        Ok(tx_id)
    }

    /// Check if a transaction has been confirmed on-chain by querying the blockchain API
    pub async fn check_transaction_confirmation(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<bool, TrackerBoxUpdaterError> {
        let client = basis_store::http::bounded_client();
        let url = format!(
            "{}/blockchain/transaction/byId/{}",
            config.node_url.trim_end_matches('/'),
            tx_id
        );

        let mut request = client.get(&url);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;

        match response.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(TrackerBoxUpdaterError::HttpError(format!(
                    "HTTP {} checking transaction: {}",
                    status, body
                )))
            }
        }
    }
}

/// Derive a P2PK change address from a compressed tracker public key.
fn derive_change_address(tracker_pubkey: &[u8; 33]) -> Result<String, TrackerBoxUpdaterError> {
    use ergo_lib::ergo_chain_types::EcPoint;
    use ergo_lib::ergotree_ir::chain::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

    let ec_point = EcPoint::sigma_parse_bytes(tracker_pubkey).map_err(|e| {
        TrackerBoxUpdaterError::SerializationError(format!("Invalid tracker pubkey: {}", e))
    })?;
    let prove_dlog = ProveDlog::new(ec_point);
    let address = Address::P2Pk(prove_dlog);
    let encoder =
        ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

/// Convert a P2PK or P2S address string to its hex-encoded ergoTree bytes.
fn change_address_to_ergo_tree(address_str: &str) -> Result<String, TrackerBoxUpdaterError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let address = encoder.parse_address_from_str(address_str).map_err(|e| {
        TrackerBoxUpdaterError::SerializationError(format!(
            "Invalid address '{}': {}",
            address_str, e
        ))
    })?;
    let tree = address.script().map_err(|e| {
        TrackerBoxUpdaterError::SerializationError(format!(
            "Failed to get script for address '{}': {}",
            address_str, e
        ))
    })?;
    Ok(hex::encode(tree.sigma_serialize_bytes().map_err(|e| {
        TrackerBoxUpdaterError::SerializationError(format!("Failed to serialize ergoTree: {:?}", e))
    })?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirmation_depth_current_tip() {
        // Transaction in the current tip has depth 1.
        assert_eq!(confirmation_depth(100, 100), 1);
    }

    #[test]
    fn test_confirmation_depth_one_block_back() {
        assert_eq!(confirmation_depth(101, 100), 2);
    }

    #[test]
    fn test_confirmation_depth_unknown_height() {
        // An inclusion height of 0 is treated as unknown.
        assert_eq!(confirmation_depth(100, 0), 0);
    }

    #[test]
    fn test_confirmation_depth_does_not_underflow() {
        // If current height somehow lags inclusion height, depth is at least 1.
        assert_eq!(confirmation_depth(100, 200), 1);
    }

    #[test]
    fn test_tracker_box_update_config_default_depth() {
        let config = TrackerBoxUpdateConfig::default();
        assert_eq!(config.min_confirmation_depth, 2);
    }
}
