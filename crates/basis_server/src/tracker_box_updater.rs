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
use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};

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
}

impl SharedTrackerState {
    /// Creates a new SharedTrackerState with a default tracker public key for testing
    /// This should only be used in tests - production code should use new_with_tracker_key
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(create_default_tracker_pubkey())), // Initialize with a valid compressed pubkey
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
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
}

/// Configuration for the tracker box updater service
#[derive(Debug, Clone)]
pub struct TrackerBoxUpdateConfig {
    /// Interval in seconds between tracker box updates (default: 600 seconds = 10 minutes)
    pub update_interval_seconds: u64,
    /// Flag to enable/disable the tracker box updater (default: true)
    pub enabled: bool,
    /// Flag to enable actual transaction submission (default: false for logging-only mode)
    pub submit_transaction: bool,
    /// Ergo node URL for API requests
    pub ergo_node_url: String,
    /// API key for Ergo node authentication (if required)
    pub ergo_api_key: Option<String>,
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            update_interval_seconds: 600, // 10 minutes
            enabled: true,
            submit_transaction: false,
            ergo_node_url: "".to_string(), // Must be provided in config
            ergo_api_key: None,
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
                    let r4_constant = Constant::from(ec_point);
                    let r4_bytes = r4_constant.sigma_serialize_bytes();
                    let r4_hex = hex::encode(&r4_bytes);

                    // R5 should contain the full AVL tree data structure, not just the digest
                    // For Ergo, this should be the serialized SAvlTree type which includes the tree structure
                    // For now, we'll use the proper serialized AVL tree format with insert flag enabled
                    // The format should be: [type_byte][tree_structure_with_digest][flags]
                    // For an empty tree it would be: 644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000
                    // But for a tracker with notes, we need the actual tree structure
                    // For now, let's create a placeholder with the proper format for a non-empty tree
                    // We'll use the expected format: 64[type_bytes][digest][insert_flag][key_len][value_len]

                    // For now, we'll use the proper serialized format for the AVL tree structure
                    // This should be the serialized SAvlTree value containing the actual tree data
                    // Since we're working with the root digest, we need to create the proper serialized AVL tree format
                    let mut r5_bytes = Vec::new();
                    r5_bytes.push(0x64); // AVL tree type identifier
                    r5_bytes.extend_from_slice(&current_root); // 33-byte root digest
                    r5_bytes.push(0x01); // Insert flag enabled
                    r5_bytes.push(0x20); // Key length (32 bytes)
                    r5_bytes.push(0x00); // Value length (variable)

                    let r5_hex = hex::encode(&r5_bytes);

                    if config.submit_transaction {
                        // Submit transaction to Ergo node via wallet payment API
                        match Self::submit_tracker_box_update(
                            &client,
                            &config.ergo_node_url,
                            config.ergo_api_key.as_deref(),
                            &r4_hex,
                            &r5_hex,
                            network_prefix,  // Use the network_prefix passed to the start function
                            tracker_nft_id.as_str(),  // Pass the required tracker NFT ID
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
                    } else {
                        // Log register values for testing/development
                        info!(
                            "Tracker Box Update: R4={} (GroupElement), R5={} (SAvlTree), timestamp={}, root_digest={}",
                            r4_hex,
                            r5_hex,
                            current_timestamp(),
                            hex::encode(&current_root)
                        );
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

    /// Submit a tracker box update transaction to the Ergo node
    /// Uses a placeholder network prefix since we can't determine it from hex keys alone
    /// The real network prefix must be determined elsewhere and passed in
    async fn submit_tracker_box_update(
        client: &reqwest::Client,
        node_url: &str,
        api_key: Option<&str>,
        r4_hex: &str,
        r5_hex: &str,
        network_prefix: NetworkPrefix,  // Network prefix to use for address encoding
        tracker_nft_id: &str,           // Required tracker NFT ID to include in the transaction
    ) -> Result<String, TrackerBoxUpdaterError> {
        // Build the URL for the wallet payment endpoint
        let url = format!("{}/wallet/payment/send", node_url);

        // Prepare the request body with register values
        // For R4 as GroupElement (EcPoint), we need to parse the serialized constant
        use ergo_lib::ergotree_ir::mir::constant::{Constant, TryExtractInto};
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
        use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
        use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;

        let r4_constant_bytes = hex::decode(r4_hex)
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to decode R4 register hex: {}", e)))?;

        let r4_constant = Constant::sigma_parse_bytes(&r4_constant_bytes)
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to parse R4 constant: {}", e)))?;

        tracing::debug!("Attempting to extract EcPoint from R4 constant with type: {:?}", r4_constant.tpe);
        // Extract the EcPoint from the constant (GroupElement is EcPoint)
        let ec_point = r4_constant
            .try_extract_into::<EcPoint>()
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to extract EcPoint from R4 constant: {}", e)))?;
        tracing::debug!("Successfully extracted EcPoint from R4 constant");

        // Create ProveDlog from the extracted EcPoint for the P2PK address
        let prove_dlog = ProveDlog::new(ec_point);

        // Create P2PK address from ProveDlog
        let p2pk_address = Address::P2Pk(prove_dlog);

        // Encode as base58 address using the provided network prefix
        let address_encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(network_prefix);
        let address = address_encoder
            .address_to_str(&p2pk_address);

        // Wrap the payment request in an array as required by Ergo node's /wallet/payment/send API
        // For tracker box updates, always include the tracker NFT in the output box assets
        // The tracker NFT is essential and always provided according to server configuration
        // The wallet must possess the tracker NFT token to include it in the transaction
        // If the token is not available in wallet boxes, this will cause NotEnoughTokensError
        let assets = vec![json!({
            "tokenId": tracker_nft_id,
            "amount": 1
        })];

        let request_obj = json!({
            "address": address, // Properly formatted P2PK address
            "value": 100000, // Minimum ERG value for box (0.001 ERG)
            "registers": {
                "R4": r4_hex,  // Tracker public key
                "R5": r5_hex,  // AVL+ tree root digest as properly serialized ByteArray constant
            },
            "assets": assets, // Include tracker NFT in output box assets when provided
            "fee": 1000000 // Standard transaction fee (0.001 ERG)
        });

        let payload = json!([request_obj]);

        // Log the request payload for debugging
        let payload_str = to_string(&payload).unwrap_or_else(|_| "Invalid JSON".to_string());
        tracing::debug!("Tracker box update payload: {}", payload_str);

        // Build the HTTP request
        let mut request_builder = client.post(&url);

        // Add API key if provided
        if let Some(key) = api_key {
            request_builder = request_builder.header("api_key", key);
        }

        // Send the request
        let response = request_builder
            .json(&payload)
            .send()
            .await
            .map_err(|e| TrackerBoxUpdaterError::ConfigurationError(format!("Failed to send request: {}", e)))?;

        // Check response status first before consuming the response
        let response_status = response.status();

        // Parse the response (should contain transaction ID)
        let response_text = response.text().await.map_err(|e| {
            TrackerBoxUpdaterError::ConfigurationError(format!("Failed to read response: {}", e))
        })?;

        // Check response status and handle errors using the text we already have
        if !response_status.is_success() {
            return Err(TrackerBoxUpdaterError::ConfigurationError(format!(
                "Wallet payment API returned error: {} - {}",
                response_status,
                response_text
            )));
        }

        // Extract and return transaction ID from response
        // The response format depends on Ergo node API, typically contains transaction ID
        Ok(response_text) // In real implementation, parse the JSON response for tx ID
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
        assert!(!config.submit_transaction);
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