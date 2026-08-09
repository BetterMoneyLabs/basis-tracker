//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes by submitting transactions to the Ergo blockchain via the
//! node's /wallet/transaction/sign and /transactions endpoints.

use ergo_lib::chain::transaction::Transaction;
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
    pub change_address: Option<String>,
    pub tracker_secret_key: Option<[u8; 32]>,
}

impl std::fmt::Debug for TrackerBoxUpdateConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackerBoxUpdateConfig")
            .field("node_url", &self.node_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("update_interval_seconds", &self.update_interval_seconds)
            .field("fee", &self.fee)
            .field("change_address", &self.change_address)
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
            change_address: None,
            tracker_secret_key: None,
        }
    }
}

#[cfg(test)]
mod secret_redaction_tests {
    use super::{TrackerBoxUpdateConfig, TrackerBoxUpdater};
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
        type Writer = BufferWriter;

        fn make_writer(&'a self) -> Self::Writer {
            BufferWriter(Arc::clone(&self.0))
        }
    }

    impl Write for BufferWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("log buffer lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    async fn one_error_response(body: &'static str) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let address = listener.local_addr().expect("loopback address");
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stream.read(&mut buffer).await.expect("read request");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(header_end) =
                    request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }

            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
            request
        });
        (format!("http://{address}"), task)
    }

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

    #[tokio::test(flavor = "current_thread")]
    async fn node_signing_error_and_logs_do_not_echo_secret_bearing_bodies() {
        let tracker_sentinel = "sentinel-tracker-private-key-do-not-log";
        let api_sentinel = "sentinel-updater-api-key-do-not-log";
        let response_sentinel = "sentinel-node-response-do-not-log";
        let (node_url, server) = one_error_response(response_sentinel).await;
        let config = TrackerBoxUpdateConfig {
            node_url,
            api_key: Some(api_sentinel.to_string()),
            ..TrackerBoxUpdateConfig::default()
        };
        let unsigned_tx = serde_json::json!({
            "tx": {"inputs": [], "dataInputs": [], "outputs": []},
            "secrets": {"dlog": [tracker_sentinel]}
        });

        let writer = SharedWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(writer.clone())
            .finish();
        let dispatch = tracing::Dispatch::new(subscriber);
        let guard = tracing::dispatcher::set_default(&dispatch);
        let error = TrackerBoxUpdater::sign_transaction(&config, unsigned_tx)
            .await
            .expect_err("loopback node must reject signing")
            .to_string();
        drop(guard);

        let request = server.await.expect("loopback server task");
        assert!(request
            .windows(tracker_sentinel.len())
            .any(|window| window == tracker_sentinel.as_bytes()));
        assert!(request
            .windows(api_sentinel.len())
            .any(|window| window == api_sentinel.as_bytes()));

        let logs = String::from_utf8(writer.0.lock().expect("log buffer lock").clone())
            .expect("UTF-8 logs");
        for sentinel in [tracker_sentinel, api_sentinel, response_sentinel] {
            assert!(!error.contains(sentinel));
            assert!(!logs.contains(sentinel));
        }
        assert!(logs.contains("Node signing request completed"));
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
    #[error("Broadcast outcome is unknown; tracker publication remains fenced: {0}")]
    BroadcastOutcomeUnknown(String),
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

/// Tracker box updater service
pub struct TrackerBoxUpdater;

struct PreparedTrackerUpdate {
    signed_tx: serde_json::Value,
    tx_id: String,
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

            let submitted_height = Self::get_node_height(&config)
                .await
                .map(|height| height as u64)
                .unwrap_or(tracker_box.creation_height as u64);
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

    /// Get the current blockchain height from the Ergo node.
    async fn get_node_height(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<u32, TrackerBoxUpdaterError> {
        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(e.to_string()))?;
        let url = format!("{}/info", config.node_url.trim_end_matches('/'));

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
                "HTTP {} fetching node height: {}",
                status, body
            )));
        }

        let body: serde_json::Value = response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

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
        info!("Requesting node signature for tracker-box update");

        let client = crate::bounded_http::node_http()
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(e.to_string()))?;
        let url = format!(
            "{}/wallet/transaction/sign",
            config.node_url.trim_end_matches('/')
        );

        let mut request = client.post(&url).json(&unsigned_tx);
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = client
            .execute(request)
            .await
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(e.to_string()))?;

        let status = response.status();
        info!(status = %status, "Node signing request completed");

        if !status.is_success() {
            return Err(TrackerBoxUpdaterError::SigningFailed(format!(
                "HTTP {}",
                status
            )));
        }

        response
            .json()
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(format!("JSON parse error: {}", e)))
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

    /// Submit a tracker box update transaction via /wallet/transaction/sign
    async fn prepare_tracker_update(
        tracker_nft_id: &str,
        config: &TrackerBoxUpdateConfig,
        tracker_box: &ErgoBoxApi,
        tracker_pubkey: &[u8; 33],
        avl_root_digest: &[u8; 33],
    ) -> Result<PreparedTrackerUpdate, TrackerBoxUpdaterError> {
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
        let tx_id = Self::signed_transaction_id(&signed_tx)?;
        Ok(PreparedTrackerUpdate { signed_tx, tx_id })
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
mod publication_health_tests {
    use super::{SharedTrackerState, TrackerBoxUpdater, TrackerBoxUpdaterError};

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
