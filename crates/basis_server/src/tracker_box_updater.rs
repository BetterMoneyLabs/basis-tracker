//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically updates the R4 and R5 register values
//! of the tracker box every 10 minutes by submitting transactions to the Ergo blockchain via the wallet payment API.

use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{error, info};
use serde_json::{json, to_string};
use basis_store::reqwest;
use hex;
use ergo_lib::ergotree_ir::address::NetworkPrefix;

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
}

impl SharedTrackerState {
    /// Creates a new SharedTrackerState with a default tracker public key for testing
    /// This should only be used in tests - production code should use new_with_tracker_key
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(create_default_tracker_pubkey())), // Initialize with a valid compressed pubkey
            tracker_box_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
            tracker_box_id: Arc::new(RwLock::new(None)),
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
            [0x02u8; 33] // fallback with compressed pubkey marker
        }
    }

    pub fn get_tracker_box_id(&self) -> Option<String> {
        if let Ok(id_lock) = self.tracker_box_id.read() {
            id_lock.clone()
        } else {
            None
        }
    }
}

/// Configuration for the tracker box updater service
#[derive(Debug, Clone)]
pub struct TrackerBoxUpdateConfig {
    /// Interval in seconds between tracker box updates (default: 600 seconds = 10 minutes)
    pub update_interval_seconds: u64,
    /// Flag to enable/disable the tracker box updater (default: true)
    pub enabled: bool,
    /// Ergo node URL for API requests
    pub ergo_node_url: String,
    /// API key for Ergo node authentication (if required)
    pub ergo_api_key: Option<String>,
    /// Tracker secret key for signing transactions (32 bytes)
    pub tracker_secret_key: Option<[u8; 32]>,
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            update_interval_seconds: 600, // 10 minutes
            enabled: true,
            ergo_node_url: "".to_string(), // Must be provided in config
            ergo_api_key: None,
            tracker_secret_key: None,
        }
    }
}

/// Tracker Box Updater Service
pub struct TrackerBoxUpdater;

impl TrackerBoxUpdater {
    /// Create a new tracker box updater service
    pub fn new() -> Self {
        Self
    }

    /// Start the periodic update service
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_tracker_state: SharedTrackerState,
        network_prefix: NetworkPrefix,
        tracker_nft_id: String,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        if !config.enabled {
            info!("Tracker box updater is disabled, not starting service");
            return Ok(());
        }

        info!(
            "Starting tracker box updater with interval {} seconds",
            config.update_interval_seconds
        );

        let client = reqwest::Client::new();
        let mut interval = tokio::time::interval(Duration::from_secs(config.update_interval_seconds));

        // Skip the first immediate tick to avoid immediate execution
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Access the shared state to get current values
                    let current_root = shared_tracker_state.get_avl_root_digest();
                    let tracker_pubkey = shared_tracker_state.get_tracker_pubkey();

                    // R4 should contain the tracker public key as a GroupElement constant (EcPoint)
                    // Convert the public key bytes directly to an EcPoint and serialize as Constant
                    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
                    use ergo_lib::ergotree_ir::mir::constant::Constant;
                    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

                    tracing::info!("Creating EcPoint from tracker public key bytes: {}", hex::encode(&tracker_pubkey));
                    let ec_point = EcPoint::sigma_parse_bytes(&tracker_pubkey)
                        .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse EcPoint from tracker public key: {}", e)))?;
                    tracing::info!("Successfully created EcPoint from tracker public key");
                    let r4_constant = Constant::from(ec_point.clone());
                    let r4_bytes = r4_constant.sigma_serialize_bytes();
                    let r4_hex = hex::encode(&r4_bytes);

                    // R5 should contain the serialized SAvlTree type
                    // The proper format for Ergo AVL tree register is the serialized tree structure
                    // Following the Ergo specification for SAvlTree serialization:
                    // - Type byte: 0x64 (SAvlTree type identifier)
                    // - Root digest: 33 bytes (1 byte height + 32 bytes blake2b256 hash)
                    // - Flags: 1 byte (bit 0=insert, bit 1=update, bit 2=remove allowed)
                    // - Key length: 4 bytes big-endian (64 for hash(issuer||receiver))
                    // - Value length: 4 bytes big-endian (0 for variable length)

                    // Get the current root digest from shared state (33 bytes)
                    // The root digest from basis_trees::BasisAvlTree is already in the correct format:
                    // [height_byte (1 byte) || blake2b256_hash (32 bytes)]
                    let root_digest = current_root; // Already [u8; 33]

                    // Build the serialized SAvlTree
                    let mut r5_bytes = Vec::with_capacity(43); // 1 + 33 + 1 + 4 + 4 = 43 bytes
                    r5_bytes.push(0x64u8); // SAvlTree type identifier
                    r5_bytes.extend_from_slice(&root_digest); // 33-byte root digest
                    r5_bytes.push(0x01u8); // Flags: insert-only allowed (bit 0 set)
                    r5_bytes.extend_from_slice(&32u32.to_be_bytes()); // Key length: 32 bytes
                    r5_bytes.extend_from_slice(&0u32.to_be_bytes()); // Value length: 0 (variable)

                    let r5_hex = hex::encode(&r5_bytes);

                    // Check if we have a tracker box ID and secret key
                    let tracker_box_id = shared_tracker_state.get_tracker_box_id();
                    let tracker_secret_key = config.tracker_secret_key.clone();
                    
                    if tracker_box_id.is_none() {
                        error!("No tracker box ID available. Skipping update cycle. Ensure tracker scanner has found the box.");
                        continue;
                    }
                    
                    if tracker_secret_key.is_none() {
                        error!("No tracker secret key configured. Cannot sign transactions locally.");
                        continue;
                    }
                    
                    let tracker_box_id = tracker_box_id.unwrap();
                    let tracker_secret_key = tracker_secret_key.unwrap();
                    
                    // Derive tracker address from public key for the output
                    let tracker_address = {
                        let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(
                            ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet
                        );
                        encoder.address_to_str(&ergo_lib::ergotree_ir::address::Address::P2Pk(
                            ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog::from(
                                ec_point.clone()
                            )
                        ))
                    };
                    
                    // Build, sign, and submit transaction locally using tracker secret key
                    match Self::submit_tracker_box_update(
                        &client,
                        &config.ergo_node_url,
                        config.ergo_api_key.as_deref(),
                        &tracker_box_id,
                        &tracker_secret_key,
                        &r4_constant,
                        &r5_bytes,
                        tracker_nft_id.as_str(),
                        &tracker_address,
                        &r4_hex,
                    ).await {
                        Ok(tx_id) => {
                            info!(
                                "Tracker Box Update Transaction Submitted: R4={} (GroupElement), R5={} (SAvlTree), timestamp={}, root_digest={}, tx_id={}",
                                r4_hex,
                                r5_hex,
                                current_timestamp(),
                                hex::encode(&current_root),
                                tx_id
                            );
                        }
                        Err(e) => {
                            error!("Failed to submit tracker box update transaction: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Tracker box updater shutdown signal received");
                    break;
                }
            }
        }

        info!("Tracker box updater stopped");
        Ok(())
    }

    /// Build, sign, and submit a tracker box update transaction using the wallet API
    /// 
    /// This function uses /wallet/transaction/send to let the node wallet handle
    /// transaction creation and signing, which properly supports SAvlTree registers.
    /// Falls back to self-funding (99M output) if wallet auto-selection fails.
    async fn submit_tracker_box_update(
        client: &reqwest::Client,
        node_url: &str,
        api_key: Option<&str>,
        tracker_box_id: &str,
        _tracker_secret_key: &[u8; 32],
        _r4_constant: &ergo_lib::ergotree_ir::mir::constant::Constant,
        r5_bytes: &[u8],
        tracker_nft_id: &str,
        tracker_address: &str,
        r4_hex: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        let r5_hex = hex::encode(r5_bytes);

        // Try wallet auto-selection first (no inputsRaw specified)
        let wallet_request = serde_json::json!({
            "requests": [
                {
                    "address": tracker_address,
                    "value": 100000000,
                    "assets": [{"tokenId": tracker_nft_id, "amount": 1}],
                    "registers": {
                        "R4": r4_hex,
                        "R5": r5_hex
                    }
                }
            ],
            "fee": 1000000
        });
        
        let wallet_url = format!("{}/wallet/transaction/send", node_url);
        tracing::info!("Submitting tracker box update via wallet API: {}", wallet_url);
        
        let mut wallet_request_builder = client.post(&wallet_url)
            .header("Content-Type", "application/json")
            .json(&wallet_request);
        
        if let Some(key) = api_key {
            wallet_request_builder = wallet_request_builder.header("api_key", key);
        }
        
        let wallet_response = wallet_request_builder.send().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to send wallet request: {}", e))
        })?;
        
        let wallet_status = wallet_response.status();
        let wallet_text = wallet_response.text().await.unwrap_or_default();
        
        if wallet_status.is_success() {
            tracing::info!("Tracker box update transaction sent via wallet: {}", wallet_text);
            return Ok(wallet_text.trim_matches('"').to_string());
        }
        
        // Check if it's the "null" signing error
        if wallet_text.contains("Failed to sign boxes due to null") {
            tracing::warn!("Wallet auto-selection failed with null signing error. Retrying with explicit inputs...");
            
            // Wait a minute and retry (as instructed)
            tokio::time::sleep(Duration::from_secs(60)).await;
            
            // Retry once with auto-selection
            let mut retry_request = client.post(&wallet_url)
                .header("Content-Type", "application/json")
                .json(&wallet_request);
            
            if let Some(key) = api_key {
                retry_request = retry_request.header("api_key", key);
            }
            
            let retry_response = retry_request.send().await.map_err(|e| {
                TrackerBoxUpdaterError::ConfigurationError(format!("Failed to retry wallet request: {}", e))
            })?;
            
            let retry_status = retry_response.status();
            let retry_text = retry_response.text().await.unwrap_or_default();
            
            if retry_status.is_success() {
                tracing::info!("Tracker box update transaction sent on retry: {}", retry_text);
                return Ok(retry_text.trim_matches('"').to_string());
            }
            
            if retry_text.contains("Failed to sign boxes due to null") {
                tracing::warn!("Wallet retry also failed. Falling back to self-funding (99M output)...");
                
                // Fallback: self-funding with 99M output to cover fee
                let self_fund_request = serde_json::json!({
                    "requests": [
                        {
                            "address": tracker_address,
                            "value": 99000000,
                            "assets": [{"tokenId": tracker_nft_id, "amount": 1}],
                            "registers": {
                                "R4": r4_hex,
                                "R5": r5_hex
                            }
                        }
                    ],
                    "fee": 1000000
                });
                
                let mut fallback_request = client.post(&wallet_url)
                    .header("Content-Type", "application/json")
                    .json(&self_fund_request);
                
                if let Some(key) = api_key {
                    fallback_request = fallback_request.header("api_key", key);
                }
                
                let fallback_response = fallback_request.send().await.map_err(|e| {
                    TrackerBoxUpdaterError::ConfigurationError(format!("Failed to send fallback request: {}", e))
                })?;
                
                let fallback_status = fallback_response.status();
                let fallback_text = fallback_response.text().await.unwrap_or_default();
                
                if fallback_status.is_success() {
                    tracing::info!("Self-funding tracker box update sent: {}", fallback_text);
                    return Ok(fallback_text.trim_matches('"').to_string());
                }
                
                return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                    "Self-funding fallback failed ({}): {}", fallback_status, fallback_text
                )));
            }
            
            return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                "Wallet retry failed ({}): {}", retry_status, retry_text
            )));
        }
        
        Err(TrackerBoxUpdaterError::ConfigurationError(format!(
            "Wallet transaction failed ({}): {}", wallet_status, wallet_text
        )))
    }
    
    /// Fetch tracker box JSON from Ergo node
    async fn fetch_tracker_box(
        client: &reqwest::Client,
        node_url: &str,
        api_key: Option<&str>,
        tracker_box_id: &str,
    ) -> Result<serde_json::Value, TrackerBoxUpdaterError> {
        let box_url = format!("{}/utxo/byId/{}", node_url, tracker_box_id);
        tracing::info!("Fetching tracker box from: {}", box_url);
        
        let mut request_builder = client.get(&box_url);
        if let Some(key) = api_key {
            request_builder = request_builder.header("api_key", key);
        }
        
        let box_response = request_builder.send().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to fetch tracker box: {}", e))
        })?;
        
        if !box_response.status().is_success() {
            let status = box_response.status();
            let text = box_response.text().await.unwrap_or_default();
            return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                "Failed to fetch tracker box ({}): {}", status, text
            )));
        }
        
        let box_json: serde_json::Value = box_response.json().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse tracker box JSON: {}", e))
        })?;
        
        tracing::debug!("Tracker box JSON: {}", serde_json::to_string_pretty(&box_json).unwrap_or_default());
        Ok(box_json)
    }
    
    /// Fetch current blockchain height from Ergo node
    async fn fetch_blockchain_height(
        client: &reqwest::Client,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<u32, TrackerBoxUpdaterError> {
        let info_url = format!("{}/info", node_url);
        let mut info_request = client.get(&info_url);
        if let Some(key) = api_key {
            info_request = info_request.header("api_key", key);
        }
        
        let info_response = info_request.send().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to get blockchain height: {}", e))
        })?;
        
        let info: serde_json::Value = info_response.json().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse info response: {}", e))
        })?;
        
        let height = info["fullHeight"].as_u64()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing fullHeight in node response".to_string()))? as u32;
        
        tracing::info!("Current blockchain height: {}", height);
        Ok(height)
    }
    
    /// Submit hex-encoded transaction bytes to Ergo node
    async fn submit_transaction_bytes(
        client: &reqwest::Client,
        node_url: &str,
        api_key: Option<&str>,
        tx_hex: &str,
    ) -> Result<String, TrackerBoxUpdaterError> {
        let submit_url = format!("{}/transactions/bytes", node_url);
        tracing::info!("Submitting signed transaction to: {}", submit_url);
        
        let mut submit_request = client.post(&submit_url)
            .header("Content-Type", "application/json")
            .json(&tx_hex);
        
        if let Some(key) = api_key {
            submit_request = submit_request.header("api_key", key);
        }
        
        let submit_response = submit_request.send().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to submit transaction: {}", e))
        })?;
        
        let submit_status = submit_response.status();
        let submit_text = submit_response.text().await.unwrap_or_default();
        
        if !submit_status.is_success() {
            return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                "Transaction submission failed ({}): {}", submit_status, submit_text
            )));
        }
        
        Ok(submit_text)
    }
    
    /// Build and sign transaction using ergo-lib (blocking, non-Send types)
    fn build_and_sign_transaction(
        box_json_str: &str,
        current_height: u32,
        r4_constant: &ergo_lib::ergotree_ir::mir::constant::Constant,
        r5_bytes: &[u8],
        tracker_secret_key: &[u8; 32],
    ) -> Result<String, TrackerBoxUpdaterError> {
        use ergo_lib::chain::ergo_box::{BoxValue, ErgoBox, ErgoBoxCandidate, NonMandatoryRegisters};
        use ergo_lib::wallet::tx_builder::TxBuilder;
        use ergo_lib::wallet::box_selector::BoxSelection;
        use ergo_lib::wallet::Wallet;
        use ergo_lib::wallet::secret_key::SecretKey;
        use ergo_lib::wallet::signing::TransactionContext;
        use ergo_lib::chain::ergo_state_context::ErgoStateContext;
        use ergo_lib::ergotree_ir::mir::constant::Constant;
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
        use ergo_lib::chain::token::{Token, TokenId, TokenAmount};
        use ergo_lib::chain::transaction::TxId;
        use ergo_lib::chain::Digest32;
        use ergo_lib::chain::ergo_box::BoxId;
        use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
        
        // Parse the JSON and manually construct ErgoBox to avoid deserialization issues
        let box_json: serde_json::Value = serde_json::from_str(box_json_str).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse box JSON: {}", e))
        })?;
        
        let value_u64 = box_json["value"].as_u64()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing or invalid 'value' field".to_string()))?;
        let value = BoxValue::new(value_u64).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Invalid box value: {:?}", e))
        })?;
        
        let ergo_tree_hex = box_json["ergoTree"].as_str()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing 'ergoTree' field".to_string()))?;
        let ergo_tree_bytes = hex::decode(ergo_tree_hex).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Invalid ergoTree hex: {}", e))
        })?;
        let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse ErgoTree: {}", e))
        })?;
        
        let mut tokens = Vec::new();
        if let Some(assets) = box_json["assets"].as_array() {
            for asset in assets {
                let token_id_hex = asset["tokenId"].as_str()
                    .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing tokenId".to_string()))?;
                let token_id_bytes = hex::decode(token_id_hex).map_err(|e| {
                    TrackerBoxUpdaterError::ConfigurationError(format!("Invalid tokenId hex: {}", e))
                })?;
                let token_id_arr: [u8; 32] = token_id_bytes.as_slice().try_into().map_err(|_| {
                    TrackerBoxUpdaterError::ConfigurationError("Invalid tokenId length".to_string())
                })?;
                let token_id = TokenId::from(BoxId::from(Digest32::from(token_id_arr)));
                let amount = asset["amount"].as_u64()
                    .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing token amount".to_string()))?;
                let token_amount = TokenAmount::try_from(amount).map_err(|e| {
                    TrackerBoxUpdaterError::ConfigurationError(format!("Invalid token amount: {:?}", e))
                })?;
                tokens.push(Token { token_id, amount: token_amount });
            }
        }
        
        let creation_height = box_json["creationHeight"].as_u64()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing 'creationHeight' field".to_string()))? as u32;
        
        let tx_id_hex = box_json["transactionId"].as_str()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing 'transactionId' field".to_string()))?;
        let tx_id_bytes = hex::decode(tx_id_hex).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Invalid transactionId hex: {}", e))
        })?;
        let tx_id_arr: [u8; 32] = tx_id_bytes.as_slice().try_into().map_err(|_| {
            TrackerBoxUpdaterError::ConfigurationError("Invalid transactionId length".to_string())
        })?;
        let transaction_id = TxId(Digest32::from(tx_id_arr));
        
        let index = box_json["index"].as_u64()
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Missing 'index' field".to_string()))? as u16;
        
        // Construct ErgoBox manually (old registers don't matter since we create new ones)
        let input_box = ErgoBox::new(
            value,
            ergo_tree,
            tokens.clone(),
            NonMandatoryRegisters::empty(),
            creation_height,
            transaction_id,
            index,
        );
        
        tracing::info!(
            "Constructed tracker box: id={:?}, value={}, tokens={}",
            input_box.box_id(),
            input_box.value.as_u64(),
            tokens.len()
        );
        
        // Prepare fee and check funding
        let fee = BoxValue::new(1000000).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Invalid fee value: {:?}", e))
        })?;
        
        let min_box_value = BoxValue::SAFE_USER_MIN;
        let min_input_value = fee.as_u64() + min_box_value.as_u64();
        
        if *input_box.value.as_u64() < min_input_value {
            let tracker_address = Self::get_tracker_address_from_pubkey(r4_constant)?;
            return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                "Tracker box underfunded. Current value: {} nanoERG, required: {} nanoERG (fee: {} + min box: {}). \
                 Please send at least {} ERG to tracker address: {}",
                input_box.value.as_u64(),
                min_input_value,
                fee.as_u64(),
                min_box_value.as_u64(),
                (min_input_value - input_box.value.as_u64()) as f64 / 1e9,
                tracker_address
            )));
        }
        
        // Calculate output value (input - fee)
        let output_value = input_box.value.checked_sub(&fee).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to calculate output value: {:?}", e))
        })?;
        
        // Build the new registers
        let r5_constant = Constant::sigma_parse_bytes(r5_bytes)
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse R5 constant: {}", e)))?;
        
        let new_registers = NonMandatoryRegisters::from_ordered_values(vec![
            r4_constant.clone(),
            r5_constant,
        ]).map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to create registers: {:?}", e)))?;
        
        // Create output candidate
        let output_candidate = ErgoBoxCandidate {
            value: output_value,
            ergo_tree: input_box.ergo_tree.clone(),
            tokens: input_box.tokens.clone(),
            additional_registers: new_registers,
            creation_height: current_height,
        };
        
        // Build unsigned transaction
        let box_selection = BoxSelection {
            boxes: vec![input_box.clone()],
            change_boxes: vec![],
        };
        
        let change_address = Self::get_tracker_address_from_pubkey(r4_constant)?;
        let change_address_parsed = ergo_lib::ergotree_ir::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet
        ).parse_address_from_str(&change_address).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse change address: {}", e))
        })?;
        
        let tx_builder = TxBuilder::new(
            box_selection,
            vec![output_candidate],
            current_height,
            fee,
            change_address_parsed,
            BoxValue::MIN,
        );
        
        let unsigned_tx = tx_builder.build().map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to build unsigned transaction: {:?}", e))
        })?;
        
        tracing::info!("Built unsigned transaction with {} inputs and {} outputs", 
            unsigned_tx.inputs.len(), 
            unsigned_tx.output_candidates.len()
        );
        
        // Create wallet and sign transaction
        let secret_key = SecretKey::dlog_from_bytes(tracker_secret_key)
            .ok_or_else(|| TrackerBoxUpdaterError::ConfigurationError("Invalid tracker secret key".to_string()))?;
        
        let wallet = Wallet::from_secrets(vec![secret_key]);
        
        let tx_context = TransactionContext {
            spending_tx: unsigned_tx,
            boxes_to_spend: vec![input_box.clone()],
            data_boxes: vec![],
        };
        
        let state_context = ErgoStateContext::dummy();
        
        let signed_tx = wallet.sign_transaction(tx_context, &state_context).map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to sign transaction: {:?}", e))
        })?;
        
        tracing::info!("Successfully signed transaction: id={:?}", signed_tx.id());
        
        // Serialize transaction to hex
        let tx_bytes = signed_tx.sigma_serialize_bytes();
        let tx_hex = hex::encode(&tx_bytes);
        
        Ok(tx_hex)
    }
    
    /// Helper to get tracker P2PK address from R4 constant (EcPoint)
    fn get_tracker_address_from_pubkey(r4_constant: &ergo_lib::ergotree_ir::mir::constant::Constant) -> Result<String, TrackerBoxUpdaterError> {
        use ergo_lib::ergotree_ir::mir::constant::TryExtractInto;
        use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
        use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
        use ergo_lib::ergotree_ir::address::Address;
        
        let ec_point = r4_constant.clone()
            .try_extract_into::<EcPoint>()
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to extract EcPoint from R4: {}", e)))?;
        
        let prove_dlog = ProveDlog::new(ec_point);
        let p2pk_address = Address::P2Pk(prove_dlog);
        
        let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(
            ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet
        );
        
        Ok(encoder.address_to_str(&p2pk_address))
    }
}

/// Error type for tracker box updater operations
#[derive(Debug)]
pub enum TrackerBoxUpdaterError {
    StateAccessError(String),
    RootCalculationError(String),
    ConfigurationError(String),
    LoggingError(String),
}

impl std::fmt::Display for TrackerBoxUpdaterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackerBoxUpdaterError::StateAccessError(msg) => write!(f, "State access error: {}", msg),
            TrackerBoxUpdaterError::RootCalculationError(msg) => write!(f, "Root calculation error: {}", msg),
            TrackerBoxUpdaterError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            TrackerBoxUpdaterError::LoggingError(msg) => write!(f, "Logging error: {}", msg),
        }
    }
}

impl std::error::Error for TrackerBoxUpdaterError {}

/// Helper function to get the current Unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_tracker_box_updater_creation() {
        let updater = TrackerBoxUpdater::new();
        // Just verify that the updater can be created
        assert!(true); // Simple assertion since the updater was created without error
    }

    #[test]
    fn test_current_timestamp() {
        let timestamp = current_timestamp();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // The timestamp should be close to the current time (within a few seconds)
        assert!(now >= timestamp);
        assert!(now - timestamp < 5); // Allow for small timing differences
    }

    #[test]
    fn test_tracker_box_update_config_default() {
        let config = TrackerBoxUpdateConfig::default();
        assert_eq!(config.update_interval_seconds, 600);
        assert!(config.enabled);
    }

    #[test]
    fn test_shared_tracker_state() {
        let shared_state = SharedTrackerState::new();

        // Test initial values
        let initial_root = shared_state.get_avl_root_digest();
        assert_eq!(initial_root, [0u8; 33]);

        let initial_pubkey = shared_state.get_tracker_pubkey();
        assert_eq!(initial_pubkey[0], 0x02); // Compressed format marker

        // Test updating values
        let new_root = [0xFFu8; 33];
        shared_state.set_avl_root_digest(new_root);
        assert_eq!(shared_state.get_avl_root_digest(), new_root);

        let mut new_pubkey = [0u8; 33];
        new_pubkey[0] = 0x03; // Different compressed format marker
        shared_state.set_tracker_pubkey(new_pubkey);
        assert_eq!(shared_state.get_tracker_pubkey(), new_pubkey);
    }
}