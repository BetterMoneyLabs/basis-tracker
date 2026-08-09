//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes by submitting transactions to the Ergo blockchain via the
//! node's /wallet/transaction/sign and /transactions endpoints.

use basis_store::chain_reconciliation::{
    validate_anchor_still_active, validate_chain_effect, validate_reorg_horizon, validate_rollback,
    ActiveChainProof, JournalBootstrap, ReconciliationError, ReconciliationIntent,
    ReconciliationJournal, ReconciliationJournalBinding, ReconciliationPolicy, RecoveryAction,
    ReorgHorizonDecision, TransactionChainEvidence, ValidatedChainEffect, ValidatedRollback,
    MAX_REORG_MONITOR_DEPTH,
};
use ergo_lib::chain::transaction::Transaction;
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
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
    /// Transaction which created the accepted tracker successor.
    pub tx_id: Option<String>,
    /// Active-chain block containing `tx_id`.
    pub block_id: Option<String>,
    /// Policy-accepted successor depth.
    pub successor_depth: Option<u64>,
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
    historical_confirmation: Arc<RwLock<Option<basis_store::ConfirmedProjectionAnchor>>>,
    confirmation_history_present: Arc<RwLock<bool>>,
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
            historical_confirmation: Arc::new(RwLock::new(None)),
            confirmation_history_present: Arc::new(RwLock::new(false)),
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
            historical_confirmation: Arc::new(RwLock::new(None)),
            confirmation_history_present: Arc::new(RwLock::new(false)),
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
    pub fn set_confirmed(
        &self,
        digest: [u8; 33],
        tx_id: String,
        box_id: String,
        block_id: String,
        height: u64,
        successor_depth: u64,
    ) {
        if let Ok(mut confirmed) = self.confirmed.write() {
            confirmed.digest = Some(digest);
            confirmed.tx_id = Some(tx_id);
            confirmed.box_id = Some(box_id);
            confirmed.block_id = Some(block_id);
            confirmed.height = Some(height);
            confirmed.successor_depth = Some(successor_depth);
        }
    }

    pub fn clear_confirmed(&self) {
        if let Ok(mut confirmed) = self.confirmed.write() {
            *confirmed = ConfirmedState::default();
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

    /// Seed or update the data-only BNS1 projection used by the startup join.
    /// This does not authorize confirmation; only a journal-private validated
    /// effect can do that.
    pub fn set_historical_confirmation(
        &self,
        anchor: Option<basis_store::ConfirmedProjectionAnchor>,
    ) {
        if anchor.is_some() {
            self.set_confirmation_history_present(true);
        }
        if let Ok(mut stored) = self.historical_confirmation.write() {
            *stored = anchor;
        }
    }

    fn get_historical_confirmation(&self) -> Option<basis_store::ConfirmedProjectionAnchor> {
        self.historical_confirmation
            .read()
            .map(|stored| stored.clone())
            .unwrap_or(None)
    }

    pub fn set_confirmation_history_present(&self, present: bool) {
        if let Ok(mut stored) = self.confirmation_history_present.write() {
            *stored = present;
        }
    }

    fn has_confirmation_history(&self) -> bool {
        self.confirmation_history_present
            .read()
            .map(|present| *present)
            .unwrap_or(true)
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
    /// Application policy expressed as successor depth (tip inclusion = 0).
    pub min_successor_depth: u64,
    /// Maximum age of the coherent evidence bundle used for acceptance.
    pub max_evidence_age_ms: u64,
    /// Explicit, maintainer-ratified successor depth after which an accepted
    /// anchor is durably retired from active reorg polling.
    pub reorg_monitor_depth: Option<u64>,
    /// Deadline applied to each node request.
    pub request_timeout_seconds: u64,
    /// Dedicated single-writer confirmed-chain journal.
    pub reconciliation_journal_path: PathBuf,
    /// One-shot permission to bind a new empty journal to a fresh BNS1
    /// generation. Never set this for an existing state directory.
    pub allow_fresh_reconciliation_journal: bool,
    /// V2 remains disabled until its complete runtime activation is approved.
    pub allow_v2_reconciliation: bool,
}

impl std::fmt::Debug for TrackerBoxUpdateConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackerBoxUpdateConfig")
            .field("node_url", &self.node_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("update_interval_seconds", &self.update_interval_seconds)
            .field("fee", &self.fee)
            .field("min_successor_depth", &self.min_successor_depth)
            .field("max_evidence_age_ms", &self.max_evidence_age_ms)
            .field("reorg_monitor_depth", &self.reorg_monitor_depth)
            .field(
                "allow_fresh_reconciliation_journal",
                &self.allow_fresh_reconciliation_journal,
            )
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
            min_successor_depth: 6,
            max_evidence_age_ms: 60_000,
            reorg_monitor_depth: None,
            request_timeout_seconds: 15,
            reconciliation_journal_path: PathBuf::from("data/confirmed-chain"),
            allow_fresh_reconciliation_journal: false,
            allow_v2_reconciliation: false,
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
    #[error("Invalid confirmed-chain updater configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Confirmed-chain reconciliation failed: {0}")]
    Reconciliation(#[from] ReconciliationError),
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
    pub transaction_id: String,
    pub index: u16,
}

/// Asset in an Ergo box
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
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
    signed_bytes: Vec<u8>,
    tx_id: String,
    intent: ReconciliationIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeTip {
    id: String,
    height: u64,
}

enum TransactionObservation {
    Pending,
    Accepted(ValidatedChainEffect),
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

    fn journal_bootstrap_policy(
        has_confirmation_history: bool,
        has_pending_publication: bool,
        fresh_approved: bool,
    ) -> JournalBootstrap {
        if has_confirmation_history || has_pending_publication || !fresh_approved {
            JournalBootstrap::ExistingRequired
        } else {
            JournalBootstrap::FreshAllowed
        }
    }

    /// Start the tracker box updater service as an async background task
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_state: SharedTrackerState,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        cmd_tx: Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        if config.allow_v2_reconciliation {
            return Err(TrackerBoxUpdaterError::InvalidConfiguration(
                "v2 reconciliation has no activated complete reserve/claim manifest".to_string(),
            ));
        }
        let tracker_nft_id = match shared_state.get_tracker_nft_id() {
            Some(id) => id,
            None => {
                info!("Tracker NFT is not configured; confirmed-chain publisher remains disabled");
                let _ = shutdown_rx.recv().await;
                return Ok(());
            }
        };
        let tracker_nft_bytes: [u8; 32] = hex::decode(&tracker_nft_id)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidConfiguration(
                    "configured tracker NFT is not exactly 32 bytes".to_string(),
                )
            })?;
        let reorg_monitor_depth = config.reorg_monitor_depth.ok_or_else(|| {
            TrackerBoxUpdaterError::InvalidConfiguration(
                "confirmed-chain reorg_monitor_depth requires explicit maintainer approval"
                    .to_string(),
            )
        })?;
        if config.update_interval_seconds == 0
            || config.request_timeout_seconds == 0
            || config.max_evidence_age_ms == 0
            || reorg_monitor_depth < config.min_successor_depth
            || reorg_monitor_depth > MAX_REORG_MONITOR_DEPTH
        {
            return Err(TrackerBoxUpdaterError::InvalidConfiguration(
                "interval, request timeout, evidence lifetime and finality horizon are invalid"
                    .to_string(),
            ));
        }
        let client = Self::node_client(&config)?;
        let policy = ReconciliationPolicy::new(
            config.min_successor_depth,
            config.max_evidence_age_ms,
            reorg_monitor_depth,
        );
        let restored_pending = Self::restored_pending_transaction(&shared_state)?;
        let historical_confirmation = shared_state.get_historical_confirmation();
        let bootstrap = Self::journal_bootstrap_policy(
            shared_state.has_confirmation_history(),
            restored_pending.is_some(),
            config.allow_fresh_reconciliation_journal,
        );
        let journal = ReconciliationJournal::open(
            &config.reconciliation_journal_path,
            ReconciliationJournalBinding::tracker_v1(tracker_nft_bytes),
            bootstrap,
        )?;
        let mut ticker = interval(Duration::from_secs(config.update_interval_seconds));
        Self::validate_startup_join(
            &journal,
            restored_pending.as_ref(),
            historical_confirmation.as_ref(),
        )?;

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

            let mut recovery_action = journal.recovery_action()?;
            // Consume decisions already sealed in the durable journal before
            // any node I/O. A node outage cannot postpone a known demotion or
            // an acceptance-ready idempotent local apply.
            match recovery_action.clone() {
                RecoveryAction::ApplyAccepted(effect) => {
                    if let Err(error) =
                        Self::apply_validated_publication(&cmd_tx, &shared_state, &journal, &effect)
                            .await
                    {
                        shared_state.quarantine_publication();
                        return Err(error);
                    }
                    continue;
                }
                RecoveryAction::ApplyRollback(rollback) => {
                    if let Err(error) =
                        Self::apply_validated_rollback(&cmd_tx, &shared_state, &journal, &rollback)
                            .await
                    {
                        shared_state.quarantine_publication();
                        return Err(error);
                    }
                    continue;
                }
                RecoveryAction::RestoreRetired(effect) => {
                    if let Err(error) =
                        Self::restore_retired_publication(&cmd_tx, &shared_state, &journal, &effect)
                            .await
                    {
                        shared_state.quarantine_publication();
                        return Err(error);
                    }
                    // A retired anchor is final under the explicit bounded
                    // policy. Restore its local projection once, then proceed
                    // as idle without any further chain polling.
                    recovery_action = RecoveryAction::Idle;
                }
                _ => {}
            }

            // An older accepted anchor remains economically relevant while a
            // newer successor is pending. Revalidate it independently so a
            // deep reorg cannot be hidden behind a perpetually pending tx id.
            let accepted_to_revalidate = match &recovery_action {
                RecoveryAction::RevalidateAccepted(effect) => Some(effect.clone()),
                _ => journal.accepted_effect()?,
            };
            if let Some(accepted) = accepted_to_revalidate {
                if let Err(error) = Self::revalidate_accepted_anchor(
                    &config,
                    &client,
                    &cmd_tx,
                    &shared_state,
                    &journal,
                    &accepted,
                    policy,
                )
                .await
                {
                    if Self::is_retryable_chain_observation_error(&error) {
                        warn!(%error, tx_id = %accepted.tx_id(), "Unable to revalidate accepted chain anchor; retaining fail-closed state");
                        continue;
                    }
                    shared_state.quarantine_publication();
                    return Err(error);
                }
            }

            match recovery_action {
                RecoveryAction::SubmitPrepared(intent) => {
                    Self::ensure_pending_matches(&shared_state, &intent)?;
                    journal.arm_submission(intent.intent_id())?;
                    match Self::broadcast_transaction(
                        &config,
                        &client,
                        intent.signed_transaction_json(),
                        intent.tx_id(),
                    )
                    .await
                    {
                        Ok(tx_id) => info!(%tx_id, "Prepared tracker transaction submitted"),
                        Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(error)) => {
                            warn!(tx_id = %intent.tx_id(), %error, "Submission outcome unknown; querying the exact transaction only");
                        }
                        Err(error) => return Err(error),
                    }
                    continue;
                }
                RecoveryAction::QueryExactTransaction(intent) => {
                    Self::ensure_pending_matches(&shared_state, &intent)?;
                    match Self::observe_transaction(&config, &client, &intent, policy).await {
                        Ok(TransactionObservation::Pending) => {
                            info!(tx_id = %intent.tx_id(), "Exact tracker transaction is not yet policy-accepted");
                        }
                        Ok(TransactionObservation::Accepted(effect)) => {
                            journal.record_validated_effect(effect)?;
                        }
                        Err(error) => {
                            warn!(tx_id = %intent.tx_id(), %error, "Confirmed-chain evidence unavailable or invalid; retaining the fence");
                        }
                    }
                    continue;
                }
                RecoveryAction::ApplyAccepted(_)
                | RecoveryAction::ApplyRollback(_)
                | RecoveryAction::RestoreRetired(_) => {
                    return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                        "durable local recovery action escaped pre-I/O dispatch".to_string(),
                    ));
                }
                RecoveryAction::RevalidateAccepted(_) => {}
                RecoveryAction::Idle => {
                    if shared_state.get_pending().tx_id.is_some() {
                        shared_state.quarantine_publication();
                        return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                            "actor has a durable pending publication but the signed-intent journal is idle"
                                .to_string(),
                        ));
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

            // This is only a construction input. Unspent-box presence never
            // promotes accounting or confirmation state.
            shared_state.set_tracker_box_id(tracker_box.box_id.clone());

            let onchain_digest = Self::tracker_root_from_box(&tracker_box)?;
            let tracker_nft_bytes: [u8; 32] = hex::decode(&tracker_nft_id)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::InvalidConfiguration(
                        "configured tracker NFT is not exactly 32 bytes".to_string(),
                    )
                })?;
            let publication_lease = Self::begin_publication(
                &cmd_tx,
                tracker_nft_bytes,
                onchain_digest,
                tracker_box.box_id.clone(),
                tracker_box.creation_height as u64,
            )
            .await
            .ok_or_else(|| {
                TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                    "tracker actor refused the publication fence".to_string(),
                )
            })?;
            let current_digest = publication_lease.digest;

            if onchain_digest == current_digest {
                info!("Tracker successor already commits the current local AVL root");
                if !Self::abort_publication(&cmd_tx, publication_lease).await {
                    shared_state.quarantine_publication();
                    return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                        "tracker actor did not release a no-op publication fence".to_string(),
                    ));
                }
                continue;
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

            let submitted_height = Self::fetch_tip(&config, &client).await?.height;
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
            // The actor receipt is durable before the signed intent is
            // journaled. A crash in this narrow window is detected as an
            // outcome-unknown mismatch at startup and remains fenced.
            journal.record_prepared(prepared.intent.clone())?;
            shared_state.set_pending(current_digest, prepared.tx_id.clone(), submitted_height);
            journal.arm_submission(prepared.intent.intent_id())?;
            match Self::broadcast_transaction(
                &config,
                &client,
                &prepared.signed_bytes,
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

    fn node_client(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<reqwest::Client, TrackerBoxUpdaterError> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))
    }

    #[doc(hidden)]
    pub async fn probe_transaction_observation(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let client = Self::node_client(config)?;
        let path = format!("/blockchain/transaction/byId/{tx_id}");
        let _ = Self::get_node_bytes(config, &client, &path, true).await?;
        Ok(())
    }

    fn validate_startup_join(
        journal: &ReconciliationJournal,
        restored_pending: Option<&(String, [u8; 33])>,
        historical_confirmation: Option<&basis_store::ConfirmedProjectionAnchor>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        journal
            .validate_tracker_startup_join(restored_pending, historical_confirmation)
            .map_err(|error| {
                TrackerBoxUpdaterError::BroadcastOutcomeUnknown(format!(
                    "tracker startup reconciliation join failed: {error}"
                ))
            })
    }

    fn ensure_pending_matches(
        shared_state: &SharedTrackerState,
        intent: &ReconciliationIntent,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let pending = shared_state.get_pending();
        let root = intent.tracker_root().ok_or_else(|| {
            TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "journaled transaction is not a tracker publication".to_string(),
            )
        })?;
        if pending
            .tx_id
            .as_deref()
            .is_some_and(|tx_id| tx_id.eq_ignore_ascii_case(intent.tx_id()))
            && pending.digest == Some(root)
            && pending.submitted_height.is_some()
        {
            Ok(())
        } else {
            shared_state.quarantine_publication();
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "actor receipt no longer matches the exact journaled transaction".to_string(),
            ))
        }
    }

    fn unix_time_ms() -> Result<u64, TrackerBoxUpdaterError> {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))?
            .as_millis();
        u64::try_from(millis).map_err(|_| {
            TrackerBoxUpdaterError::HttpError("system time exceeds u64 milliseconds".to_string())
        })
    }

    fn is_retryable_chain_observation_error(error: &TrackerBoxUpdaterError) -> bool {
        match error {
            TrackerBoxUpdaterError::HttpError(_) => true,
            TrackerBoxUpdaterError::Reconciliation(error) => !matches!(
                error,
                ReconciliationError::TicketInProgress
                    | ReconciliationError::DuplicateTransactionConflict
                    | ReconciliationError::NoTicket
                    | ReconciliationError::IntentMismatch
                    | ReconciliationError::InvalidPhase
                    | ReconciliationError::Journal(_)
                    | ReconciliationError::JournalBindingRequired
                    | ReconciliationError::JournalBindingMismatch
                    | ReconciliationError::AccountingProjectionMismatch
                    | ReconciliationError::OutcomeUnknown(_)
            ),
            _ => false,
        }
    }

    async fn get_node_bytes(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        path_and_query: &str,
        missing_is_pending: bool,
    ) -> Result<Option<Vec<u8>>, TrackerBoxUpdaterError> {
        let url = format!(
            "{}{}",
            config.node_url.trim_end_matches('/'),
            path_and_query
        );
        let mut request = client.get(url);
        if let Some(api_key) = &config.api_key {
            request = request.header("api_key", api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))?;
        if missing_is_pending && response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(TrackerBoxUpdaterError::HttpError(format!(
                "node returned HTTP {} for {}",
                response.status(),
                path_and_query.split('?').next().unwrap_or("request")
            )));
        }
        response
            .bytes()
            .await
            .map(|bytes| Some(bytes.to_vec()))
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))
    }

    async fn fetch_tip(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
    ) -> Result<NodeTip, TrackerBoxUpdaterError> {
        let bytes = Self::get_node_bytes(config, client, "/info", false)
            .await?
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("missing /info body".to_string()))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))?;
        let height = value
            .get("fullHeight")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::HttpError("/info lacks fullHeight".to_string())
            })?;
        let ids = ["bestFullHeaderId", "bestHeaderId"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() != 1 {
            return Err(TrackerBoxUpdaterError::HttpError(
                "/info does not expose one coherent full-chain tip id".to_string(),
            ));
        }
        Ok(NodeTip {
            id: ids.into_iter().next().unwrap_or_default(),
            height,
        })
    }

    async fn collect_selected_chain(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        inclusion_height: u64,
    ) -> Result<ActiveChainProof, TrackerBoxUpdaterError> {
        let before = Self::fetch_tip(config, client).await?;
        let to_height = before
            .height
            .checked_add(1)
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("node height overflow".to_string()))?;
        let path = format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &path, false)
            .await?
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("missing chain slice".to_string()))?;
        let after = Self::fetch_tip(config, client).await?;
        Ok(ActiveChainProof::from_node_responses(
            before.id,
            before.height,
            after.id,
            after.height,
            inclusion_height,
            &chain_slice,
            Self::unix_time_ms()?,
        )?)
    }

    async fn collect_selected_window(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        inclusion_height: u64,
        selected_through_height: u64,
    ) -> Result<ActiveChainProof, TrackerBoxUpdaterError> {
        let before = Self::fetch_tip(config, client).await?;
        if before.height < selected_through_height {
            return Err(TrackerBoxUpdaterError::Reconciliation(
                ReconciliationError::IncompleteAncestry,
            ));
        }
        let to_height = selected_through_height
            .checked_add(1)
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("node height overflow".to_string()))?;
        let path = format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &path, false)
            .await?
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("missing chain slice".to_string()))?;
        let after = Self::fetch_tip(config, client).await?;
        Ok(ActiveChainProof::from_bounded_node_responses(
            before.id,
            before.height,
            after.id,
            after.height,
            inclusion_height,
            selected_through_height,
            &chain_slice,
            Self::unix_time_ms()?,
        )?)
    }

    async fn observe_transaction(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        intent: &ReconciliationIntent,
        policy: ReconciliationPolicy,
    ) -> Result<TransactionObservation, TrackerBoxUpdaterError> {
        let transaction_path = format!("/blockchain/transaction/byId/{}", intent.tx_id());
        let Some(observation_bytes) =
            Self::get_node_bytes(config, client, &transaction_path, true).await?
        else {
            return Ok(TransactionObservation::Pending);
        };
        let observation: serde_json::Value = serde_json::from_slice(&observation_bytes)
            .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))?;
        let Some(inclusion_height) = observation
            .get("inclusionHeight")
            .and_then(serde_json::Value::as_u64)
        else {
            return Ok(TransactionObservation::Pending);
        };
        let transaction_json = serde_json::to_vec(&serde_json::json!({
            "id": observation.get("id").cloned().unwrap_or(serde_json::Value::Null),
            "inputs": observation.get("inputs").cloned().unwrap_or(serde_json::Value::Null),
            "dataInputs": observation.get("dataInputs").cloned().unwrap_or_else(|| serde_json::json!([])),
            "outputs": observation.get("outputs").cloned().unwrap_or(serde_json::Value::Null),
        }))
        .map_err(|error| TrackerBoxUpdaterError::HttpError(error.to_string()))?;

        let before = Self::fetch_tip(config, client).await?;
        if before
            .height
            .checked_sub(inclusion_height)
            .is_none_or(|depth| depth < policy.min_successor_depth())
        {
            return Ok(TransactionObservation::Pending);
        }
        let to_height = before
            .height
            .checked_add(1)
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("node height overflow".to_string()))?;
        let chain_path =
            format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &chain_path, false)
            .await?
            .ok_or_else(|| TrackerBoxUpdaterError::HttpError("missing chain slice".to_string()))?;
        let first_header: serde_json::Value =
            serde_json::from_slice::<serde_json::Value>(&chain_slice)
                .ok()
                .and_then(|value| {
                    value
                        .as_array()
                        .and_then(|headers| headers.first())
                        .cloned()
                })
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::HttpError("empty chain slice".to_string())
                })?;
        let block_id = first_header
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::HttpError("first header lacks id".to_string())
            })?;
        let full_block =
            Self::get_node_bytes(config, client, &format!("/blocks/{block_id}"), false)
                .await?
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::HttpError("missing full block".to_string())
                })?;
        let predecessor = Self::get_node_bytes(
            config,
            client,
            &format!("/blockchain/box/byId/{}", intent.predecessor().box_id()),
            false,
        )
        .await?
        .ok_or_else(|| TrackerBoxUpdaterError::HttpError("missing predecessor".to_string()))?;
        let after = Self::fetch_tip(config, client).await?;
        let chain = ActiveChainProof::from_node_responses(
            before.id,
            before.height,
            after.id,
            after.height,
            inclusion_height,
            &chain_slice,
            Self::unix_time_ms()?,
        )?;
        let selected_block_id = chain.first_block_id().to_string();
        let evidence = TransactionChainEvidence::from_node_snapshot(
            transaction_json,
            full_block,
            selected_block_id,
            inclusion_height,
            vec![predecessor],
            chain,
        )?;
        Ok(TransactionObservation::Accepted(validate_chain_effect(
            intent,
            &evidence,
            policy,
            Self::unix_time_ms()?,
        )?))
    }

    async fn apply_validated_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        shared_state: &SharedTrackerState,
        journal: &ReconciliationJournal,
        effect: &ValidatedChainEffect,
    ) -> Result<(), TrackerBoxUpdaterError> {
        if !Self::confirm_publication(cmd_tx, effect.clone()).await {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "tracker actor rejected a policy-validated private ticket".to_string(),
            ));
        }
        journal.mark_applied(effect)?;
        Self::set_shared_confirmed(shared_state, effect)?;
        shared_state.clear_pending();
        Ok(())
    }

    async fn apply_validated_rollback(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        shared_state: &SharedTrackerState,
        journal: &ReconciliationJournal,
        rollback: &ValidatedRollback,
    ) -> Result<(), TrackerBoxUpdaterError> {
        if !Self::rollback_publication(cmd_tx, rollback.clone()).await {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "tracker actor rejected a validated reorg rollback".to_string(),
            ));
        }
        journal.mark_rollback_applied(rollback)?;
        shared_state.clear_confirmed();
        shared_state.set_historical_confirmation(None);
        shared_state.set_confirmation_history_present(false);
        if journal.pending_intent()?.is_none() {
            shared_state.clear_pending();
        }
        Ok(())
    }

    async fn restore_retired_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        shared_state: &SharedTrackerState,
        journal: &ReconciliationJournal,
        effect: &ValidatedChainEffect,
    ) -> Result<(), TrackerBoxUpdaterError> {
        if journal.pending_intent()?.is_some() {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "retired anchor restore collided with a pending ticket".to_string(),
            ));
        }
        if Self::shared_matches_effect(shared_state, effect)? {
            return Ok(());
        }
        if !Self::confirm_publication(cmd_tx, effect.clone()).await {
            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "retired publication could not be replayed exactly".to_string(),
            ));
        }
        Self::set_shared_confirmed(shared_state, effect)
    }

    async fn revalidate_accepted_anchor(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        shared_state: &SharedTrackerState,
        journal: &ReconciliationJournal,
        effect: &ValidatedChainEffect,
        policy: ReconciliationPolicy,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let tip = Self::fetch_tip(config, client).await?;
        let observed_depth = tip
            .height
            .checked_sub(effect.inclusion_height())
            .ok_or_else(|| {
                TrackerBoxUpdaterError::Reconciliation(ReconciliationError::DepthMismatch)
            })?;
        if observed_depth >= policy.reorg_monitor_depth() {
            let selected_through = effect
                .inclusion_height()
                .checked_add(policy.reorg_monitor_depth())
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::Reconciliation(ReconciliationError::DepthMismatch)
                })?;
            let selected_window = Self::collect_selected_window(
                config,
                client,
                effect.inclusion_height(),
                selected_through,
            )
            .await?;
            return match validate_reorg_horizon(
                effect,
                &selected_window,
                policy,
                Self::unix_time_ms()?,
            )? {
                ReorgHorizonDecision::Retire(retirement) => {
                    journal.retire_accepted(&retirement)?;
                    if journal.pending_intent()?.is_none() {
                        Self::restore_retired_publication(cmd_tx, shared_state, journal, effect)
                            .await?;
                    }
                    Ok(())
                }
                ReorgHorizonDecision::Rollback(rollback) => {
                    journal.record_rollback(rollback.clone())?;
                    Self::apply_validated_rollback(cmd_tx, shared_state, journal, &rollback).await
                }
            };
        }
        let selected_chain =
            Self::collect_selected_chain(config, client, effect.inclusion_height()).await?;
        let now = Self::unix_time_ms()?;
        if validate_anchor_still_active(effect, &selected_chain, policy, now).is_ok() {
            if journal.pending_intent()?.is_some() {
                // The actor is fenced by a newer transaction. Its persisted
                // records retain the older provenance, but remain Pending and
                // therefore non-redeemable until the newer outcome resolves.
                return Ok(());
            }
            if !Self::shared_matches_effect(shared_state, effect)? {
                // Restores restart-demoted local projections only through the
                // same sealed accepted ticket.
                if !Self::confirm_publication(cmd_tx, effect.clone()).await {
                    return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                        "accepted publication could not be replayed exactly".to_string(),
                    ));
                }
                Self::set_shared_confirmed(shared_state, effect)?;
            }
            return Ok(());
        }
        let rollback = validate_rollback(effect, &selected_chain, policy, now)?;
        journal.record_rollback(rollback.clone())?;
        Self::apply_validated_rollback(cmd_tx, shared_state, journal, &rollback).await
    }

    fn shared_matches_effect(
        shared_state: &SharedTrackerState,
        effect: &ValidatedChainEffect,
    ) -> Result<bool, TrackerBoxUpdaterError> {
        let root = effect.tracker_root().ok_or_else(|| {
            TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "accepted ticket has no tracker root".to_string(),
            )
        })?;
        let shared = shared_state.get_confirmed();
        Ok(shared.digest == Some(root)
            && shared
                .tx_id
                .as_deref()
                .is_some_and(|tx_id| tx_id.eq_ignore_ascii_case(effect.tx_id()))
            && shared.block_id.as_deref() == Some(effect.block_id())
            && shared.box_id.as_deref() == Some(effect.successor_box_id())
            && shared.height == Some(effect.inclusion_height())
            && shared.successor_depth == Some(effect.successor_depth()))
    }

    fn set_shared_confirmed(
        shared_state: &SharedTrackerState,
        effect: &ValidatedChainEffect,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let root = effect.tracker_root().ok_or_else(|| {
            TrackerBoxUpdaterError::BroadcastOutcomeUnknown(
                "accepted ticket has no tracker root".to_string(),
            )
        })?;
        shared_state.set_confirmed(
            root,
            effect.tx_id().to_string(),
            effect.successor_box_id().to_string(),
            effect.block_id().to_string(),
            effect.inclusion_height(),
            effect.successor_depth(),
        );
        shared_state.set_historical_confirmation(Some(
            basis_store::ConfirmedProjectionAnchor::from_parts(
                effect.tx_id().to_string(),
                effect.successor_box_id().to_string(),
                effect.block_id().to_string(),
                effect.inclusion_height(),
                effect.successor_depth(),
                effect.intent_id().to_string(),
                root,
            ),
        ));
        Ok(())
    }

    fn tracker_root_from_box(box_data: &ErgoBoxApi) -> Result<[u8; 33], TrackerBoxUpdaterError> {
        let encoded = box_data.additional_registers.get("R5").ok_or_else(|| {
            TrackerBoxUpdaterError::SerializationError(
                "tracker box is missing its R5 AVL commitment".to_string(),
            )
        })?;
        let bytes = hex::decode(encoded)
            .map_err(|error| TrackerBoxUpdaterError::SerializationError(error.to_string()))?;
        if bytes.len() != 37 || bytes[0] != 0x64 || bytes[34..] != [0x03, 0x20, 0x00] {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "tracker R5 is not the exact 33-byte insert/update AVL ABI".to_string(),
            ));
        }
        let mut root = [0u8; 33];
        root.copy_from_slice(&bytes[1..34]);
        Ok(root)
    }

    async fn begin_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        tracker_nft_id: [u8; 32],
        observed_root: [u8; 33],
        box_id: String,
        height: u64,
    ) -> Option<crate::PublicationLease> {
        let tx = cmd_tx.as_ref()?;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        tx.send(crate::TrackerCommand::BeginPublication {
            tracker_nft_id,
            observed_root,
            box_id,
            height,
            response_tx,
        })
        .await
        .ok()?;
        response_rx.await.ok()?.ok()
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
        effect: ValidatedChainEffect,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        if tx
            .send(crate::TrackerCommand::ConfirmPublication {
                effect,
                response_tx,
            })
            .await
            .is_err()
        {
            return false;
        }
        matches!(response_rx.await, Ok(Ok(_)))
    }

    async fn rollback_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        rollback: ValidatedRollback,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        if tx
            .send(crate::TrackerCommand::RollbackPublication {
                rollback,
                response_tx,
            })
            .await
            .is_err()
        {
            return false;
        }
        matches!(response_rx.await, Ok(Ok(_)))
    }

    /// Find the tracker box on chain using the tracker NFT ID
    async fn find_tracker_box(
        config: &TrackerBoxUpdateConfig,
        tracker_nft_id: &str,
    ) -> Result<ErgoBoxApi, TrackerBoxUpdaterError> {
        let client = Self::node_client(config)?;
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

        let boxes: Vec<ErgoBoxApi> = response
            .json()
            .await
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
        let client = Self::node_client(config)?;
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

        let entries: Vec<WalletBoxEntry> = response
            .json()
            .await
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
        let client = Self::node_client(config)?;
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

        let binary: BoxBinaryResponse = response
            .json()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        Ok(binary.bytes)
    }

    /// Get the current blockchain height from the Ergo node.
    async fn get_node_height(
        config: &TrackerBoxUpdateConfig,
    ) -> Result<u32, TrackerBoxUpdaterError> {
        let client = Self::node_client(config)?;
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

        let body: serde_json::Value = response
            .json()
            .await
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

        let client = Self::node_client(config)?;
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
        info!(status = %status, "Node signing request completed");

        if !status.is_success() {
            return Err(TrackerBoxUpdaterError::SigningFailed(format!(
                "HTTP {}",
                status
            )));
        }

        serde_json::from_str(&body_text)
            .map_err(|e| TrackerBoxUpdaterError::SigningFailed(format!("JSON parse error: {}", e)))
    }

    /// Broadcast a signed transaction to the Ergo node's /transactions endpoint.
    async fn broadcast_transaction(
        config: &TrackerBoxUpdateConfig,
        client: &reqwest::Client,
        signed_bytes: &[u8],
        expected_tx_id: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        info!("Broadcasting signed tracker-box update transaction");

        let url = format!("{}/transactions", config.node_url.trim_end_matches('/'));

        let mut request = client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(signed_bytes.to_vec());
        if let Some(ref api_key) = config.api_key {
            request = request.header("api_key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::BroadcastOutcomeUnknown(e.to_string()))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        info!(status = %status, "Transaction broadcast request completed");

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
        if tracker_box.additional_registers.get("R4") != Some(&r4_value) {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "tracker predecessor R4 does not match the configured receiver".to_string(),
            ));
        }

        let mut r5_bytes = vec![0x64u8];
        r5_bytes.extend_from_slice(avl_root_digest);
        r5_bytes.push(0x03u8); // insert + update allowed (insertOrUpdate contract)
        r5_bytes.extend_from_slice(&vlq_encode(32));
        r5_bytes.extend_from_slice(&vlq_encode(0));
        let r5_value = hex::encode(&r5_bytes);

        let mut output_registers = tracker_box.additional_registers.clone();
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

        let nft_occurrences = tracker_box
            .assets
            .iter()
            .filter(|asset| asset.token_id == tracker_nft_id)
            .count();
        if nft_occurrences != 1
            || !tracker_box
                .assets
                .first()
                .is_some_and(|asset| asset.token_id == tracker_nft_id && asset.amount == 1)
        {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "tracker NFT is not a unique singleton at token index zero".to_string(),
            ));
        }
        let output_assets = tracker_box.assets.clone();

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
        let signed_bytes = serde_json::to_vec(&signed_tx).map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "cannot serialize signed transaction: {error}"
            ))
        })?;
        let predecessor_box_json = serde_json::to_vec(tracker_box).map_err(|error| {
            TrackerBoxUpdaterError::SerializationError(format!(
                "cannot serialize tracker predecessor: {error}"
            ))
        })?;
        let intent = ReconciliationIntent::tracker_publication(
            signed_bytes.clone(),
            predecessor_box_json,
            *avl_root_digest,
        )?;
        if !intent.tx_id().eq_ignore_ascii_case(&tx_id) {
            return Err(TrackerBoxUpdaterError::SerializationError(
                "signed intent transaction id changed during construction".to_string(),
            ));
        }
        Ok(PreparedTrackerUpdate {
            signed_bytes,
            tx_id,
            intent,
        })
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
    use super::{
        ErgoBoxApi, SharedTrackerState, TrackerBoxUpdateConfig, TrackerBoxUpdater,
        TrackerBoxUpdaterError,
    };
    use basis_store::{
        chain_reconciliation::{
            JournalBootstrap, ReconciliationError, ReconciliationJournal,
            ReconciliationJournalBinding,
        },
        ConfirmedProjectionAnchor,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::Duration;

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

    async fn one_response(
        status: &'static str,
        body: String,
        delay_ms: u64,
    ) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stream.read(&mut buffer).await.unwrap_or(0);
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
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
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            request
        });
        (format!("http://{address}"), task)
    }

    fn request_body(request: &[u8]) -> &[u8] {
        request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|offset| &request[offset + 4..])
            .unwrap_or_default()
    }

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
    fn only_chain_observation_failures_are_retryable_after_anchor_revalidation() {
        assert!(TrackerBoxUpdater::is_retryable_chain_observation_error(
            &TrackerBoxUpdaterError::HttpError("node unavailable".to_string())
        ));
        assert!(TrackerBoxUpdater::is_retryable_chain_observation_error(
            &TrackerBoxUpdaterError::Reconciliation(ReconciliationError::StaleEvidence)
        ));
        assert!(!TrackerBoxUpdater::is_retryable_chain_observation_error(
            &TrackerBoxUpdaterError::Reconciliation(ReconciliationError::OutcomeUnknown(
                "journal persist".to_string(),
            ))
        ));
        assert!(!TrackerBoxUpdater::is_retryable_chain_observation_error(
            &TrackerBoxUpdaterError::BroadcastOutcomeUnknown("actor rejected".to_string())
        ));
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

    #[tokio::test(flavor = "current_thread")]
    async fn transaction_404_is_pending_evidence_not_confirmation() {
        let (node_url, server) = one_response("404 Not Found", "{}".to_string(), 0).await;
        let config = TrackerBoxUpdateConfig {
            node_url,
            ..TrackerBoxUpdateConfig::default()
        };
        let client = TrackerBoxUpdater::node_client(&config).unwrap();
        let state = SharedTrackerState::new();
        let pending_root = [0x42; 33];
        let pending_tx = "11".repeat(32);
        state.set_pending(pending_root, pending_tx.clone(), 100);
        let result = TrackerBoxUpdater::get_node_bytes(
            &config,
            &client,
            &format!("/blockchain/transaction/byId/{}", "11".repeat(32)),
            true,
        )
        .await
        .unwrap();
        assert!(result.is_none());
        let still_fenced = state.get_pending();
        assert_eq!(still_fenced.tx_id, Some(pending_tx));
        assert_eq!(still_fenced.digest, Some(pending_root));
        assert_eq!(still_fenced.submitted_height, Some(100));
        let _ = server.await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn absent_reorg_horizon_refuses_before_any_node_io() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_url = format!("http://{}", listener.local_addr().unwrap());
        let state = SharedTrackerState::new();
        state.set_tracker_nft_id("11".repeat(32));
        let temp = tempfile::tempdir().unwrap();
        let config = TrackerBoxUpdateConfig {
            node_url,
            reconciliation_journal_path: temp.path().join("journal"),
            reorg_monitor_depth: None,
            ..TrackerBoxUpdateConfig::default()
        };
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        assert!(matches!(
            TrackerBoxUpdater::start(config, state, shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::InvalidConfiguration(_))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn fresh_journal_bootstrap_is_allowed_only_for_history_free_approved_state() {
        assert_eq!(
            TrackerBoxUpdater::journal_bootstrap_policy(false, false, true),
            JournalBootstrap::FreshAllowed
        );
        for (history, pending, approved) in [
            (true, false, true),
            (false, true, true),
            (false, false, false),
            (true, true, false),
        ] {
            assert_eq!(
                TrackerBoxUpdater::journal_bootstrap_policy(history, pending, approved),
                JournalBootstrap::ExistingRequired
            );
        }
    }

    fn historical_confirmation() -> ConfirmedProjectionAnchor {
        ConfirmedProjectionAnchor::from_parts(
            "33".repeat(32),
            "22".repeat(32),
            "44".repeat(32),
            100,
            6,
            "55".repeat(32),
            [0x66; 33],
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn historical_bns1_rejects_a_missing_journal_without_node_io_or_writes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_url = format!("http://{}", listener.local_addr().unwrap());
        let state = SharedTrackerState::new();
        state.set_tracker_nft_id("11".repeat(32));
        state.set_historical_confirmation(Some(historical_confirmation()));
        let parent = tempfile::tempdir().unwrap();
        let journal_path = parent.path().join("missing-journal");
        let config = TrackerBoxUpdateConfig {
            node_url,
            reorg_monitor_depth: Some(12),
            allow_fresh_reconciliation_journal: true,
            reconciliation_journal_path: journal_path.clone(),
            ..TrackerBoxUpdateConfig::default()
        };
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        assert!(matches!(
            TrackerBoxUpdater::start(config, state, shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::Reconciliation(
                ReconciliationError::JournalBindingRequired
            ))
        ));
        assert!(!journal_path.exists());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn historical_bns1_rejects_manifest_only_journal_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_url = format!("http://{}", listener.local_addr().unwrap());
        let state = SharedTrackerState::new();
        state.set_tracker_nft_id("11".repeat(32));
        state.set_historical_confirmation(Some(historical_confirmation()));
        let parent = tempfile::tempdir().unwrap();
        let journal_path = parent.path().join("journal");
        {
            let empty = ReconciliationJournal::open(
                &journal_path,
                ReconciliationJournalBinding::tracker_v1([0x11; 32]),
                JournalBootstrap::FreshAllowed,
            )
            .unwrap();
            assert!(matches!(
                empty.recovery_action().unwrap(),
                basis_store::chain_reconciliation::RecoveryAction::Idle
            ));
        }
        let manifest_path = journal_path.join("confirmed-chain.manifest");
        let manifest_before = std::fs::read(&manifest_path).unwrap();
        let config = TrackerBoxUpdateConfig {
            node_url,
            reorg_monitor_depth: Some(12),
            allow_fresh_reconciliation_journal: true,
            reconciliation_journal_path: journal_path.clone(),
            ..TrackerBoxUpdateConfig::default()
        };
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        assert!(matches!(
            TrackerBoxUpdater::start(config, state, shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
        ));
        assert_eq!(std::fs::read(manifest_path).unwrap(), manifest_before);
        let reopened = ReconciliationJournal::open(
            journal_path,
            ReconciliationJournalBinding::tracker_v1([0x11; 32]),
            JournalBootstrap::ExistingRequired,
        )
        .unwrap();
        assert!(matches!(
            reopened.recovery_action().unwrap(),
            basis_store::chain_reconciliation::RecoveryAction::Idle
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn historical_bns1_rejects_wrong_journal_binding_without_rewrite() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_url = format!("http://{}", listener.local_addr().unwrap());
        let state = SharedTrackerState::new();
        state.set_tracker_nft_id("11".repeat(32));
        state.set_historical_confirmation(Some(historical_confirmation()));
        let parent = tempfile::tempdir().unwrap();
        let journal_path = parent.path().join("journal");
        {
            let _wrong = ReconciliationJournal::open(
                &journal_path,
                ReconciliationJournalBinding::tracker_v1([0x22; 32]),
                JournalBootstrap::FreshAllowed,
            )
            .unwrap();
        }
        let manifest_path = journal_path.join("confirmed-chain.manifest");
        let manifest_before = std::fs::read(&manifest_path).unwrap();
        let config = TrackerBoxUpdateConfig {
            node_url,
            reorg_monitor_depth: Some(12),
            allow_fresh_reconciliation_journal: true,
            reconciliation_journal_path: journal_path,
            ..TrackerBoxUpdateConfig::default()
        };
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        assert!(matches!(
            TrackerBoxUpdater::start(config, state, shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::Reconciliation(
                ReconciliationError::JournalBindingMismatch
            ))
        ));
        assert_eq!(std::fs::read(manifest_path).unwrap(), manifest_before);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_timeout_never_becomes_confirmation() {
        let (node_url, server) = one_response("200 OK", "{}".to_string(), 1_500).await;
        let config = TrackerBoxUpdateConfig {
            node_url,
            request_timeout_seconds: 1,
            ..TrackerBoxUpdateConfig::default()
        };
        let client = TrackerBoxUpdater::node_client(&config).unwrap();
        assert!(matches!(
            TrackerBoxUpdater::get_node_bytes(&config, &client, "/info", false).await,
            Err(TrackerBoxUpdaterError::HttpError(_))
        ));
        let _ = server.await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn broadcast_posts_the_exact_journaled_signed_bytes() {
        let signed_bytes = SIGNED_TRANSACTION_JSON.as_bytes().to_vec();
        let expected_tx_id = "9148408c04c2e38a6402a7950d6157730fa7d49e9ab3b9cadec481d7769918e9";
        let (node_url, server) = one_response("200 OK", format!("\"{expected_tx_id}\""), 0).await;
        let config = TrackerBoxUpdateConfig {
            node_url,
            ..TrackerBoxUpdateConfig::default()
        };
        let client = TrackerBoxUpdater::node_client(&config).unwrap();
        assert_eq!(
            TrackerBoxUpdater::broadcast_transaction(
                &config,
                &client,
                &signed_bytes,
                expected_tx_id,
            )
            .await
            .unwrap(),
            expected_tx_id
        );
        let request = server.await.unwrap();
        assert_eq!(request_body(&request), signed_bytes);
    }

    #[test]
    fn tracker_r5_shape_is_exact_not_prefix_only() {
        let mut r5 = vec![0x64];
        r5.extend_from_slice(&[0x42; 33]);
        r5.extend_from_slice(&[0x03, 0x20, 0x00]);
        let mut box_json = serde_json::json!({
            "boxId": "11".repeat(32), "value": 1_000_000u64,
            "ergoTree": "00", "assets": [],
            "additionalRegisters": {"R5": hex::encode(&r5)},
            "creationHeight": 1, "transactionId": "22".repeat(32), "index": 0
        });
        let parsed: ErgoBoxApi = serde_json::from_value(box_json.clone()).unwrap();
        assert_eq!(
            TrackerBoxUpdater::tracker_root_from_box(&parsed).unwrap(),
            [0x42; 33]
        );
        r5[34] = 0x01;
        box_json["additionalRegisters"]["R5"] = serde_json::json!(hex::encode(r5));
        let malformed: ErgoBoxApi = serde_json::from_value(box_json).unwrap();
        assert!(TrackerBoxUpdater::tracker_root_from_box(&malformed).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v2_activation_is_explicitly_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let config = TrackerBoxUpdateConfig {
            allow_v2_reconciliation: true,
            reconciliation_journal_path: temp.path().join("journal"),
            ..TrackerBoxUpdateConfig::default()
        };
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        assert!(matches!(
            TrackerBoxUpdater::start(config, SharedTrackerState::new(), shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::InvalidConfiguration(_))
        ));
    }
}
