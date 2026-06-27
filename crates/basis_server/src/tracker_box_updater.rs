//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes by submitting transactions to the Ergo blockchain via the wallet payment API.

use std::sync::{Arc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

/// Create a default tracker public key that looks realistic (compressed format with proper prefix)
fn create_default_tracker_pubkey() -> [u8; 33] {
    // Use a realistic example of a compressed secp256k1 public key
    // First byte is 0x02 or 0x03 (compressed format marker)
    // Followed by 32 bytes representing x-coordinate of a point on the curve
    // Using a pattern similar to one found in the codebase
    [
        0x02, 0xda, 0xda, 0x81, 0x1a, 0x88, 0x8c, 0xd0, 0xdc, 0x7a,
        0x0a, 0x41, 0x73, 0x9a, 0x3a, 0xd9, 0xb0, 0xf4, 0x27, 0x74,
        0x1f, 0xe6, 0xca, 0x19, 0x70, 0x0c, 0xf1, 0xa5, 0x12, 0x00,
        0xc9, 0x6b, 0xf7
    ]
}

/// Shared state for the tracker box updater
#[derive(Debug, Clone)]
pub struct SharedTrackerState {
    pub avl_root_digest: Arc<RwLock<[u8; 33]>>,
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
    pub tracker_box_id: Arc<RwLock<Option<String>>>,
    pub tracker_nft_id: Arc<RwLock<Option<String>>>,
}

impl SharedTrackerState {
    /// Creates a new SharedTrackerState with a default tracker public key for testing
    /// This should only be used in tests - production code should use new_with_tracker_key
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(create_default_tracker_pubkey())), // Initialize with a valid compressed pubkey
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
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
            [0u8; 33] // fallback
        }
    }

    pub fn get_tracker_pubkey(&self) -> [u8; 33] {
        if let Ok(pubkey_lock) = self.tracker_pubkey.read() {
            *pubkey_lock
        } else {
            [0u8; 33] // fallback
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
}

/// Configuration for the tracker box updater
#[derive(Debug, Clone)]
pub struct TrackerBoxUpdateConfig {
    pub node_url: String,
    pub api_key: Option<String>,
    pub update_interval_seconds: u64,
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9053".to_string(),
            api_key: None,
            update_interval_seconds: 600, // 10 minutes
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
    #[error("No tracker public key configured")]
    NoTrackerPubkey,
    #[error("Transaction not found on chain: {0}")]
    TransactionNotFound(String),
    #[error("Transaction failed on chain: {0}")]
    TransactionFailedOnChain(String),
    #[error("State unchanged - no update needed")]
    StateUnchanged,
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Ergo box as returned by the blockchain API
#[derive(Debug, serde::Deserialize)]
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
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetApi {
    pub token_id: String,
    pub amount: u64,
}

/// Payment request for the wallet API
#[derive(Debug, serde::Serialize)]
pub struct PaymentRequest {
    pub address: String,
    pub value: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Vec<PaymentAsset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registers: Option<std::collections::HashMap<String, String>>,
}

/// Asset in a payment request
#[derive(Debug, serde::Serialize)]
pub struct PaymentAsset {
    pub token_id: String,
    pub amount: i64,
}

/// Tracker box updater service
pub struct TrackerBoxUpdater;

impl TrackerBoxUpdater {
    /// Start the tracker box updater service as an async background task
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_state: SharedTrackerState,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        let mut ticker = interval(Duration::from_secs(config.update_interval_seconds));
        
        // Track the last submitted digest to avoid redundant transactions
        let mut last_submitted_digest: Option<[u8; 33]> = None;
        // Track pending transaction that needs confirmation
        let mut pending_tx: Option<(String, [u8; 33])> = None;

        info!(
            "Tracker box updater started with {}s interval",
            config.update_interval_seconds
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Continue with update logic below
                }
                _ = shutdown_rx.recv() => {
                    info!("Tracker box updater received shutdown signal, stopping");
                    return Ok(());
                }
            }

            // First, check if we have a pending transaction that needs confirmation
            if let Some((ref tx_id, expected_digest)) = pending_tx {
                match Self::check_transaction_confirmation(&config, tx_id).await {
                    Ok(true) => {
                        info!(
                            "Transaction {} confirmed on chain. Update complete.",
                            tx_id
                        );
                        last_submitted_digest = Some(expected_digest);
                        pending_tx = None;
                        // Continue to next cycle - will check if further updates needed
                    }
                    Ok(false) => {
                        info!(
                            "Transaction {} still pending, waiting for next cycle...",
                            tx_id
                        );
                        // Skip this cycle, keep waiting for confirmation
                        continue;
                    }
                    Err(e) => {
                        error!(
                            "Failed to check transaction {} status: {}. Will retry.",
                            tx_id, e
                        );
                        // Keep pending_tx and retry next cycle
                        continue;
                    }
                }
            }

            // Check if we have a tracker NFT ID
            let tracker_nft_id = match shared_state.get_tracker_nft_id() {
                Some(id) => id,
                None => {
                    warn!("No tracker NFT ID configured, skipping update cycle");
                    continue;
                }
            };

            // Get current AVL root digest from shared state
            let current_digest = shared_state.get_avl_root_digest();
            let tracker_pubkey = shared_state.get_tracker_pubkey();

            // Skip if digest is all zeros (not initialized yet)
            if current_digest == [0u8; 33] {
                info!("AVL root digest not initialized yet, skipping update");
                continue;
            }

            // Skip if digest hasn't changed since last confirmed submission
            if let Some(last) = last_submitted_digest {
                if last == current_digest {
                    info!("AVL root digest unchanged, skipping redundant update");
                    continue;
                }
            }

            // Find the tracker box on chain
            let tracker_box = match Self::find_tracker_box(&config, &tracker_nft_id).await {
                Ok(box_data) => box_data,
                Err(e) => {
                    error!("Failed to find tracker box: {}", e);
                    continue;
                }
            };

            // Check if R5 (AVL root digest) already matches current state
            if let Some(r5_value) = tracker_box.additional_registers.get("R5") {
                // R5 is a serialized SAvlTree value (base16-encoded)
                // Format: 0x64 + 33-byte digest + flags + key_length + value_length
                // We need to extract the digest portion (bytes 1-33 after the type prefix)
                if let Ok(r5_bytes) = hex::decode(r5_value) {
                    if r5_bytes.len() >= 34 {
                        let onchain_digest = &r5_bytes[1..34]; // Skip type byte (0x64)
                        if onchain_digest == current_digest.as_slice() {
                            info!("On-chain tracker box already has current AVL root digest");
                            last_submitted_digest = Some(current_digest);
                            continue;
                        }
                    }
                }
            }

            // Build and submit the update transaction
            match Self::submit_tracker_update(&config, &tracker_box, &tracker_pubkey, &current_digest).await {
                Ok(tx_id) => {
                    info!(
                        "Tracker box update submitted. Transaction ID: {}, Box ID: {}. Waiting for confirmation...",
                        tx_id, tracker_box.box_id
                    );
                    // Set pending transaction - don't update last_submitted_digest until confirmed
                    pending_tx = Some((tx_id, current_digest));
                }
                Err(e) => {
                    error!("Failed to submit tracker box update: {}", e);
                }
            }
        }
    }

    /// Find the tracker box on chain using the tracker NFT ID
    /// 
    /// Note: There should be at most one tracker box at any time since the tracker NFT
    /// is unique (non-fungible token with amount=1). If multiple boxes are found, this
    /// indicates an inconsistent state (e.g., during a reorg or race condition).
    async fn find_tracker_box(
        config: &TrackerBoxUpdateConfig,
        tracker_nft_id: &str,
    ) -> Result<ErgoBoxApi, TrackerBoxUpdaterError> {
        let client = reqwest::Client::new();
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

        // There should be at most one tracker box since the tracker NFT is unique
        // (non-fungible token with amount=1). If multiple are found, this indicates
        // an inconsistent state - log a warning but use the first one.
        if boxes.len() > 1 {
            warn!(
                "Found {} tracker boxes for NFT {} - expected at most 1. \
                 This indicates an inconsistent state (possible reorg or race condition). \
                 Using the first box (box_id={}).",
                boxes.len(),
                tracker_nft_id,
                boxes[0].box_id
            );
        }

        Ok(boxes.into_iter().next().unwrap())
    }

    /// Submit a tracker box update transaction via the wallet payment API
    async fn submit_tracker_update(
        config: &TrackerBoxUpdateConfig,
        tracker_box: &ErgoBoxApi,
        tracker_pubkey: &[u8; 33],
        avl_root_digest: &[u8; 33],
    ) -> Result<String, TrackerBoxUpdaterError> {
        // Build R4 register: GroupElement (compressed pubkey)
        // Format: 0x07 + 33-byte compressed pubkey
        let mut r4_bytes = vec![0x07u8];
        r4_bytes.extend_from_slice(tracker_pubkey);
        let r4_value = hex::encode(&r4_bytes);

        // Build R5 register: SAvlTree
        // Format: 0x64 + 33-byte digest + 0x01 (flags) + 4-byte key length (32) + 4-byte value length (0)
        let mut r5_bytes = vec![0x64u8];
        r5_bytes.extend_from_slice(avl_root_digest);
        r5_bytes.push(0x01u8); // flags: insert-only allowed
        r5_bytes.extend_from_slice(&32u32.to_be_bytes()); // key length: 32 bytes
        r5_bytes.extend_from_slice(&0u32.to_be_bytes()); // value length: 0 (variable)
        let r5_value = hex::encode(&r5_bytes);

        // Build registers map
        let mut registers = std::collections::HashMap::new();
        registers.insert("R4".to_string(), r4_value);
        registers.insert("R5".to_string(), r5_value);

            // Build assets list - preserve the tracker NFT token
            let assets: Vec<PaymentAsset> = tracker_box
                .assets
                .iter()
                .map(|asset| PaymentAsset {
                    token_id: asset.token_id.clone(),
                    amount: asset.amount as i64,
                })
                .collect();

        // Build the payment request
        // Convert ergoTree to P2S address for the wallet payment API
        // The PaymentRequest expects an address, not raw ergoTree bytes
        let p2s_address = match ergo_lib::ergotree_ir::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet,
        )
        .parse_address_from_str(&tracker_box.ergo_tree)
        {
            // Case 1: Already a P2S address string
            Ok(addr) => {
                let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(
                    ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet,
                );
                encoder.address_to_str(&addr)
            }
            Err(_) => {
                // Case 2: Hex-encoded ergoTree - need to decode and convert to P2S
                let tree_bytes = hex::decode(&tracker_box.ergo_tree)
                    .map_err(|e| TrackerBoxUpdaterError::SerializationError(
                        format!("Failed to decode ergoTree hex: {}", e)
                    ))?;

                let ergo_tree = ergo_lib::ergotree_ir::ergo_tree::ErgoTree::sigma_parse_bytes(&tree_bytes)
                    .map_err(|e| TrackerBoxUpdaterError::SerializationError(
                        format!("Failed to parse ergoTree bytes: {}", e)
                    ))?;

                let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(
                    ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet,
                );

                // Create P2S address from the ErgoTree
                let address = ergo_lib::ergotree_ir::address::Address::P2S(
                    ergo_tree.sigma_serialize_bytes()
                );

                let p2s_address = encoder.address_to_str(&address);
                if p2s_address.is_empty() {
                    return Err(TrackerBoxUpdaterError::SerializationError(
                        "Failed to encode P2S address: empty result".to_string()
                    ));
                }
                p2s_address
            }
        };

        let payment = PaymentRequest {
            address: p2s_address,
            value: tracker_box.value as i64,
            assets: if assets.is_empty() { None } else { Some(assets) },
            registers: Some(registers),
        };

        // Submit via wallet payment API
        let client = reqwest::Client::new();
        let url = format!(
            "{}/wallet/payment/send",
            config.node_url.trim_end_matches('/')
        );

        let mut request = client.post(&url).json(&vec![payment]);
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
                return Err(TrackerBoxUpdaterError::TransactionFailedOnChain(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        // The response is a transaction ID
        let tx_id: String = response
            .json()
            .await
            .map_err(|e| TrackerBoxUpdaterError::HttpError(format!("JSON parse error: {}", e)))?;

        Ok(tx_id)
    }

    /// Check if a transaction has been confirmed on-chain by querying the blockchain API
    /// Returns true if transaction is found (confirmed), false if not yet confirmed
    pub async fn check_transaction_confirmation(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<bool, TrackerBoxUpdaterError> {
        let client = reqwest::Client::new();
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
            200 => {
                // Transaction found on chain - confirmed
                Ok(true)
            }
            404 => {
                // Transaction not yet found on chain - still pending
                Ok(false)
            }
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
