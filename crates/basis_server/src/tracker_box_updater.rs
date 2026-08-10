//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes. Exact node box bytes and one linked state context are
//! validated locally, signed with ergo-lib, and only the signed transaction is sent to the node.

use basis_store::chain_reconciliation::{
    validate_anchor_still_active, validate_chain_effect, validate_reorg_horizon, validate_rollback,
    ActiveChainProof, JournalBootstrap, ReconciliationError, ReconciliationIntent,
    ReconciliationJournal, ReconciliationJournalBinding, ReconciliationPolicy, RecoveryAction,
    ReorgHorizonDecision, TransactionChainEvidence, ValidatedChainEffect, ValidatedRollback,
    MAX_REORG_MONITOR_DEPTH,
};
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
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

const RECONCILIATION_NETWORK_ID: &str = "ergo-mainnet";

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
    confirmed: Arc<RwLock<ConfirmedState>>,
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
        if !self.is_publication_healthy() {
            return ConfirmedState::default();
        }
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
    #[error("node response or local observation integrity failed: {0}")]
    InvalidNodeResponse(String),
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
    #[error("Invalid confirmed-chain updater configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Confirmed-chain reconciliation failed: {0}")]
    Reconciliation(#[from] ReconciliationError),
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
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        cmd_tx: Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let health = shared_state.clone();
        let result = Self::run(config, shared_state, shutdown_rx, cmd_tx).await;
        if result.is_err() {
            // Every non-graceful updater termination closes the same one-way
            // gate consumed by tracker accounting and redemption reads.
            health.quarantine_publication();
        }
        result
    }

    async fn run(
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
        let source_id =
            ReconciliationPolicy::source_id_for(RECONCILIATION_NETWORK_ID, &config.node_url);
        let policy = ReconciliationPolicy::new(
            config.min_successor_depth,
            config.max_evidence_age_ms,
            reorg_monitor_depth,
            RECONCILIATION_NETWORK_ID,
            source_id,
        );
        let restored_pending = Self::restored_pending_transaction(&shared_state)?;
        let historical_confirmation = shared_state.get_historical_confirmation();
        let confirmation_history_present = shared_state.has_confirmation_history();
        let bootstrap = Self::journal_bootstrap_policy(
            confirmation_history_present,
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
            confirmation_history_present,
            &policy,
        )?;

        info!(
            "Tracker box updater started with {}s interval",
            config.update_interval_seconds
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                shutdown = shutdown_rx.recv() => {
                    match shutdown {
                        Ok(()) => {
                            info!("Tracker box updater received shutdown signal, stopping");
                            return Ok(());
                        }
                        Err(error) => {
                            return Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(format!(
                                "tracker updater shutdown channel failed: {error}"
                            )));
                        }
                    }
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
                    &policy,
                )
                .await
                {
                    // Once an effect is exposed as Confirmed, loss of its sole
                    // reorg monitor is an effect-consumer health failure, not
                    // an availability-only retry. Quarantine before returning.
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
                    match Self::observe_transaction(&config, &client, &intent, &policy).await {
                        Ok(TransactionObservation::Pending) => {
                            info!(tx_id = %intent.tx_id(), "Exact tracker transaction is not yet policy-accepted");
                        }
                        Ok(TransactionObservation::Accepted(effect)) => {
                            journal.record_validated_effect(effect)?;
                        }
                        Err(error) if Self::is_retryable_pending_observation_error(&error) => {
                            warn!(tx_id = %intent.tx_id(), %error, "Confirmed-chain evidence unavailable or invalid; retaining the fence");
                        }
                        Err(error) => {
                            shared_state.quarantine_publication();
                            return Err(error);
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
        _config: &TrackerBoxUpdateConfig,
    ) -> Result<&'static basis_store::ergo_scanner::BoundedHttpClient, TrackerBoxUpdaterError> {
        crate::bounded_http::node_http()
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
        confirmation_history_present: bool,
        policy: &ReconciliationPolicy,
    ) -> Result<(), TrackerBoxUpdaterError> {
        journal
            .validate_tracker_startup_join(
                restored_pending,
                historical_confirmation,
                confirmation_history_present,
                policy,
            )
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
            .map_err(|error| TrackerBoxUpdaterError::InvalidNodeResponse(error.to_string()))?
            .as_millis();
        u64::try_from(millis).map_err(|_| {
            TrackerBoxUpdaterError::InvalidNodeResponse(
                "system time exceeds u64 milliseconds".to_string(),
            )
        })
    }

    fn is_retryable_pending_observation_error(error: &TrackerBoxUpdaterError) -> bool {
        match error {
            TrackerBoxUpdaterError::HttpError(_) => true,
            TrackerBoxUpdaterError::Reconciliation(
                ReconciliationError::IncoherentSnapshot | ReconciliationError::StaleEvidence,
            ) => true,
            _ => false,
        }
    }

    async fn get_node_bytes(
        config: &TrackerBoxUpdateConfig,
        client: &basis_store::ergo_scanner::BoundedHttpClient,
        path_and_query: &str,
        missing_is_pending: bool,
    ) -> Result<Option<Vec<u8>>, TrackerBoxUpdaterError> {
        let url = format!(
            "{}{}",
            config.node_url.trim_end_matches('/'),
            path_and_query
        );
        let mut request = client.get(&url);
        if let Some(api_key) = &config.api_key {
            request = request.header("api_key", api_key);
        }
        let response = tokio::time::timeout(
            Duration::from_secs(config.request_timeout_seconds),
            client.execute(request),
        )
        .await
        .map_err(|_| {
            TrackerBoxUpdaterError::HttpError(
                "confirmed-chain node request exceeded its configured deadline".to_string(),
            )
        })?
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
        Ok(Some(response.bytes().to_vec()))
    }

    async fn fetch_tip(
        config: &TrackerBoxUpdateConfig,
        client: &basis_store::ergo_scanner::BoundedHttpClient,
    ) -> Result<NodeTip, TrackerBoxUpdaterError> {
        let bytes = Self::get_node_bytes(config, client, "/info", false)
            .await?
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("missing /info body".to_string())
            })?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| TrackerBoxUpdaterError::InvalidNodeResponse(error.to_string()))?;
        let height = value
            .get("fullHeight")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("/info lacks fullHeight".to_string())
            })?;
        let ids = ["bestFullHeaderId", "bestHeaderId"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() != 1 {
            return Err(TrackerBoxUpdaterError::InvalidNodeResponse(
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
        client: &basis_store::ergo_scanner::BoundedHttpClient,
        inclusion_height: u64,
    ) -> Result<ActiveChainProof, TrackerBoxUpdaterError> {
        let before = Self::fetch_tip(config, client).await?;
        let to_height = before.height.checked_add(1).ok_or_else(|| {
            TrackerBoxUpdaterError::InvalidNodeResponse("node height overflow".to_string())
        })?;
        let path = format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &path, false)
            .await?
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("missing chain slice".to_string())
            })?;
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
        client: &basis_store::ergo_scanner::BoundedHttpClient,
        inclusion_height: u64,
        selected_through_height: u64,
    ) -> Result<ActiveChainProof, TrackerBoxUpdaterError> {
        let before = Self::fetch_tip(config, client).await?;
        if before.height < selected_through_height {
            return Err(TrackerBoxUpdaterError::Reconciliation(
                ReconciliationError::IncompleteAncestry,
            ));
        }
        let to_height = selected_through_height.checked_add(1).ok_or_else(|| {
            TrackerBoxUpdaterError::InvalidNodeResponse("node height overflow".to_string())
        })?;
        let path = format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &path, false)
            .await?
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("missing chain slice".to_string())
            })?;
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
        client: &basis_store::ergo_scanner::BoundedHttpClient,
        intent: &ReconciliationIntent,
        policy: &ReconciliationPolicy,
    ) -> Result<TransactionObservation, TrackerBoxUpdaterError> {
        let transaction_path = format!("/blockchain/transaction/byId/{}", intent.tx_id());
        let Some(observation_bytes) =
            Self::get_node_bytes(config, client, &transaction_path, true).await?
        else {
            return Ok(TransactionObservation::Pending);
        };
        let observation: serde_json::Value = serde_json::from_slice(&observation_bytes)
            .map_err(|error| TrackerBoxUpdaterError::InvalidNodeResponse(error.to_string()))?;
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
        .map_err(|error| TrackerBoxUpdaterError::InvalidNodeResponse(error.to_string()))?;

        let before = Self::fetch_tip(config, client).await?;
        if before
            .height
            .checked_sub(inclusion_height)
            .is_none_or(|depth| depth < policy.min_successor_depth())
        {
            return Ok(TransactionObservation::Pending);
        }
        let to_height = before.height.checked_add(1).ok_or_else(|| {
            TrackerBoxUpdaterError::InvalidNodeResponse("node height overflow".to_string())
        })?;
        let chain_path =
            format!("/blocks/chainSlice?fromHeight={inclusion_height}&toHeight={to_height}");
        let chain_slice = Self::get_node_bytes(config, client, &chain_path, false)
            .await?
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("missing chain slice".to_string())
            })?;
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
                    TrackerBoxUpdaterError::InvalidNodeResponse("empty chain slice".to_string())
                })?;
        let block_id = first_header
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                TrackerBoxUpdaterError::InvalidNodeResponse("first header lacks id".to_string())
            })?;
        let full_block =
            Self::get_node_bytes(config, client, &format!("/blocks/{block_id}"), false)
                .await?
                .ok_or_else(|| {
                    TrackerBoxUpdaterError::InvalidNodeResponse("missing full block".to_string())
                })?;
        let predecessor = Self::get_node_bytes(
            config,
            client,
            &format!("/blockchain/box/byId/{}", intent.predecessor().box_id()),
            false,
        )
        .await?
        .ok_or_else(|| {
            TrackerBoxUpdaterError::InvalidNodeResponse("missing predecessor".to_string())
        })?;
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
            policy.clone(),
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
        client: &basis_store::ergo_scanner::BoundedHttpClient,
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        shared_state: &SharedTrackerState,
        journal: &ReconciliationJournal,
        effect: &ValidatedChainEffect,
        policy: &ReconciliationPolicy,
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
        crate::tracker_request(tx, |response_tx| crate::TrackerCommand::BeginPublication {
            tracker_nft_id,
            observed_root,
            box_id,
            height,
            response_tx,
        })
        .await
        .ok()?
        .ok()
    }

    async fn abort_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        lease: crate::PublicationLease,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        matches!(
            crate::tracker_request(tx, |response_tx| {
                crate::TrackerCommand::AbortPublication { lease, response_tx }
            })
            .await,
            Ok(Ok(()))
        )
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
        matches!(
            crate::tracker_request(tx, |response_tx| {
                crate::TrackerCommand::RecordPublicationAttempt {
                    lease,
                    tx_id,
                    submitted_height,
                    response_tx,
                }
            })
            .await,
            Ok(Ok(_))
        )
    }

    async fn confirm_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        effect: ValidatedChainEffect,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        matches!(
            crate::tracker_request(tx, |response_tx| {
                crate::TrackerCommand::ConfirmPublication {
                    effect,
                    response_tx,
                }
            })
            .await,
            Ok(Ok(_))
        )
    }

    async fn rollback_publication(
        cmd_tx: &Option<tokio::sync::mpsc::Sender<crate::TrackerCommand>>,
        rollback: ValidatedRollback,
    ) -> bool {
        let Some(tx) = cmd_tx else {
            return false;
        };
        matches!(
            crate::tracker_request(tx, |response_tx| {
                crate::TrackerCommand::RollbackPublication {
                    rollback,
                    response_tx,
                }
            })
            .await,
            Ok(Ok(_))
        )
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
        client: &basis_store::ergo_scanner::BoundedHttpClient,
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
                transaction_id: hex::encode([0x40 + index as u8; 32]),
                index: index as u16,
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
        AssetApi, ErgoBoxApi, SharedTrackerState, TrackerBoxUpdateConfig, TrackerBoxUpdater,
        TrackerBoxUpdaterError,
    };
    use basis_store::{
        chain_reconciliation::{
            JournalBootstrap, ReconciliationError, ReconciliationJournal,
            ReconciliationJournalBinding,
        },
        ConfirmedProjectionAnchor,
    };
    use ergo_lib::ergotree_ir::{ergo_tree::ErgoTree, serialization::SigmaSerializable};
    use std::collections::HashMap;
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
        state.set_confirmed(
            [0x42; 33],
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32),
            100,
            6,
        );
        assert_eq!(state.get_confirmed().digest, Some([0x42; 33]));

        state.quarantine_publication();
        assert!(!state.is_publication_healthy());
        let hidden = state.get_confirmed();
        assert!(hidden.digest.is_none());
        assert!(hidden.tx_id.is_none());
        assert!(hidden.box_id.is_none());

        state.quarantine_publication();
        assert!(!state.is_publication_healthy());
    }

    #[test]
    fn only_pending_transport_or_coherent_snapshot_races_are_retryable() {
        assert!(TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::HttpError("node unavailable".to_string())
        ));
        assert!(TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::Reconciliation(ReconciliationError::StaleEvidence)
        ));
        assert!(!TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::InvalidNodeResponse("malformed /info".to_string())
        ));
        assert!(!TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::Reconciliation(ReconciliationError::TransactionRootMismatch)
        ));
        assert!(!TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::Reconciliation(ReconciliationError::OutcomeUnknown(
                "journal persist".to_string(),
            ))
        ));
        assert!(!TrackerBoxUpdater::is_retryable_pending_observation_error(
            &TrackerBoxUpdaterError::BroadcastOutcomeUnknown("actor rejected".to_string())
        ));
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
            transaction_id: "44".repeat(32),
            index: 0,
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
            TrackerBoxUpdater::start(config, state.clone(), shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::InvalidConfiguration(_))
        ));
        assert!(!state.is_publication_healthy());
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
    async fn orphan_confirmation_rows_reject_idle_journal_before_node_io() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_url = format!("http://{}", listener.local_addr().unwrap());
        let state = SharedTrackerState::new();
        state.set_tracker_nft_id("11".repeat(32));
        // BNS1 row fragments exist, but their global BCP1 projection receipt
        // was lost. This must not be mistaken for a fresh Idle generation.
        state.set_confirmation_history_present(true);
        let parent = tempfile::tempdir().unwrap();
        let journal_path = parent.path().join("journal");
        {
            let _empty = ReconciliationJournal::open(
                &journal_path,
                ReconciliationJournalBinding::tracker_v1([0x11; 32]),
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
            TrackerBoxUpdater::start(config, state.clone(), shutdown_rx, None).await,
            Err(TrackerBoxUpdaterError::BroadcastOutcomeUnknown(_))
        ));
        assert!(!state.is_publication_healthy());
        assert_eq!(std::fs::read(manifest_path).unwrap(), manifest_before);
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
    async fn reconciliation_probe_rejects_a_node_body_over_the_process_cap() {
        let body = "x".repeat(basis_store::ergo_scanner::NODE_HTTP_MAX_BODY_BYTES + 1);
        let (node_url, server) = one_response("200 OK", body, 0).await;
        let config = TrackerBoxUpdateConfig {
            node_url,
            ..TrackerBoxUpdateConfig::default()
        };

        assert!(matches!(
            TrackerBoxUpdater::probe_transaction_observation(&config, &"11".repeat(32)).await,
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
