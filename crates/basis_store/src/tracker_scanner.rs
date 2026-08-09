//! Tracker box scanner for monitoring Basis tracker state commitment boxes
//! This module provides blockchain integration using /blockchain endpoints (no node scans).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::Client;

use crate::{
    ergo_scanner::{node_http, BoundedResponse, IndexedErgoBox, ScanBox},
    persistence::{ScannerMetadataStorage, TrackerStorage},
    TrackerBoxInfo,
};

#[derive(Error, Debug)]
pub enum TrackerScannerError {
    #[error("Tracker scanner error: {0}")]
    Generic(String),
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Node error: {0}")]
    NodeError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Box error: {0}")]
    BoxError(String),
    #[error("Invalid tracker box {0}")]
    InvalidTrackerBox(String),
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("JSON parse error: {0}")]
    JsonError(String),
    #[error("Missing tracker NFT ID configuration")]
    MissingTrackerNftId,
    #[error("Missing required register: {0}")]
    MissingRegister(String),
    #[error("Invalid register data: {0}")]
    InvalidRegisterData(String),
    #[error("Missing tracker NFT in box assets")]
    MissingTrackerNft,
}

/// Configuration for tracker scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerNodeConfig {
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Tracker NFT ID (hex-encoded)
    pub tracker_nft_id: Option<String>,
    /// Ergo node URL
    pub node_url: String,
    /// Scan registration name
    pub scan_name: Option<String>,
    /// API key for Ergo node authentication
    pub api_key: Option<String>,
}

/// Inner state for tracker scanner that requires synchronization
#[derive(Clone)]
pub struct TrackerServerStateInner {
    pub _current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
}

/// Server state for tracker scanner
/// Uses real blockchain integration with proper synchronization
#[derive(Clone)]
pub struct TrackerServerState {
    pub config: TrackerNodeConfig,
    pub inner: Arc<Mutex<TrackerServerStateInner>>,
    pub client: Client,
    pub metadata_storage: ScannerMetadataStorage,
    pub tracker_storage: TrackerStorage,
}

impl TrackerServerState {
    /// Create HTTP request builder with API key header if configured
    fn request_builder(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::RequestBuilder, TrackerScannerError> {
        debug!("Tracker request method: {}, URL: {}", method, url);
        let client =
            node_http().map_err(|error| TrackerScannerError::HttpError(error.to_string()))?;
        let mut builder = client.request(method, url);

        if let Some(api_key) = &self.config.api_key {
            builder = builder.header("api_key", api_key);
        }

        Ok(builder)
    }

    async fn execute_request(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<BoundedResponse, TrackerScannerError> {
        let client =
            node_http().map_err(|error| TrackerScannerError::HttpError(error.to_string()))?;
        client
            .execute(request)
            .await
            .map_err(|error| TrackerScannerError::HttpError(error.to_string()))
    }

    /// Get unspent tracker boxes via `GET /blockchain/box/unspent/byTokenId/{trackerNftId}`.
    pub async fn get_unspent_tracker_boxes(&self) -> Result<Vec<ScanBox>, TrackerScannerError> {
        let tracker_nft_id = self
            .config
            .tracker_nft_id
            .as_ref()
            .ok_or(TrackerScannerError::MissingTrackerNftId)?;

        let url = format!(
            "{}/blockchain/box/unspent/byTokenId/{}?limit=5&includeUnconfirmed=false",
            self.config.node_url, tracker_nft_id
        );

        debug!("Fetching unspent tracker boxes from: {}", url);

        let request = self.request_builder(reqwest::Method::GET, &url)?;
        let response = self.execute_request(request).await.map_err(|e| {
            TrackerScannerError::HttpError(format!("Failed to fetch tracker boxes: {}", e))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(TrackerScannerError::NodeError(format!(
                "Failed to get unspent tracker boxes with status {}",
                status
            )));
        }

        let indexed_boxes: Vec<IndexedErgoBox> = response.json().map_err(|e| {
            error!("Failed to parse tracker boxes JSON: {}", e);
            TrackerScannerError::JsonError(format!("Failed to parse tracker boxes: {}", e))
        })?;

        let boxes: Vec<ScanBox> = indexed_boxes.into_iter().map(Into::into).collect();
        info!("Retrieved {} unspent tracker boxes", boxes.len());

        Ok(boxes)
    }

    /// Parse a ScanBox into TrackerBoxInfo
    pub fn parse_tracker_box(
        &self,
        scan_box: &ScanBox,
    ) -> Result<TrackerBoxInfo, TrackerScannerError> {
        let tracker_nft_id = self
            .config
            .tracker_nft_id
            .as_ref()
            .ok_or(TrackerScannerError::MissingTrackerNftId)?;

        // Validate that the box contains the tracker NFT
        let has_tracker_nft = scan_box
            .assets
            .iter()
            .any(|asset| asset.token_id == *tracker_nft_id && asset.amount >= 1);

        if !has_tracker_nft {
            return Err(TrackerScannerError::MissingTrackerNft);
        }

        // Extract data from registers
        let tracker_pubkey_raw = scan_box
            .additional_registers
            .get("R4")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R4".to_string()))?
            .clone();

        // Strip the 0x07 prefix if present (GroupElement type identifier from Ergo registers)
        let tracker_pubkey =
            if tracker_pubkey_raw.starts_with("07") && tracker_pubkey_raw.len() >= 68 {
                // Extract the actual 33-byte public key (66 hex chars) after the 0x07 prefix
                tracker_pubkey_raw[2..].to_string()
            } else {
                // Use as-is if no prefix or wrong length
                tracker_pubkey_raw
            };

        let state_commitment = scan_box
            .additional_registers
            .get("R5")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R5".to_string()))?
            .clone();

        // R6 contains the tracker NFT ID as a Coll[Byte] constant (serialized as
        // 0e20 + 64 hex chars). Parse it out and validate against the configured
        // tracker NFT ID.
        let r6_tracker_nft_id = scan_box
            .additional_registers
            .get("R6")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R6".to_string()))?
            .clone();

        let tracker_nft_hex = if r6_tracker_nft_id.len() >= 4 {
            &r6_tracker_nft_id[4..]
        } else {
            r6_tracker_nft_id.as_str()
        };
        let tracker_nft_id_from_r6 = hex::decode(tracker_nft_hex).map_err(|_| {
            TrackerScannerError::InvalidRegisterData(format!(
                "Invalid R6 tracker NFT ID hex: {}",
                r6_tracker_nft_id
            ))
        })?;
        if tracker_nft_id_from_r6.len() != 32 {
            return Err(TrackerScannerError::InvalidRegisterData(format!(
                "Invalid R6 tracker NFT ID length: expected 32 bytes, got {}",
                tracker_nft_id_from_r6.len()
            )));
        }
        let tracker_nft_id_from_r6 = hex::encode(&tracker_nft_id_from_r6);

        // Use the configured tracker NFT ID only if it matches R6; otherwise the
        // on-chain value is authoritative.
        if tracker_nft_id != &tracker_nft_id_from_r6 {
            warn!(
                "Tracker box R6 NFT ID ({}) does not match configured tracker NFT ID ({}). Using on-chain value.",
                tracker_nft_id_from_r6, tracker_nft_id
            );
        }

        // The box creation height is the best available height indicator.
        let last_verified_height = scan_box.creation_height;

        // Validate register data (basic sanity checks)
        if tracker_pubkey.len() != 66 {
            // 33 bytes hex encoded (after stripping 0x07 prefix if present)
            return Err(TrackerScannerError::InvalidRegisterData(format!(
                "Invalid tracker pubkey length: {} (expected 66 hex chars)",
                tracker_pubkey.len()
            )));
        }

        // The R5 register contains the serialized SAvlTree which can vary in length
        // It should start with 0x64 (SAvlTree type identifier) followed by the tree data
        if !state_commitment.starts_with("64") {
            return Err(TrackerScannerError::InvalidRegisterData(
                format!("Invalid state commitment format: does not start with SAvlTree type identifier (64)")
            ));
        }

        // Optionally, we can add a reasonable length check to prevent extremely large values
        if state_commitment.len() < 66 {
            // At least 64 + 2 chars for the type identifier
            return Err(TrackerScannerError::InvalidRegisterData(format!(
                "Invalid state commitment length: {} (too short)",
                state_commitment.len()
            )));
        }

        Ok(TrackerBoxInfo {
            box_id: scan_box.box_id.clone(),
            tracker_pubkey,
            state_commitment,
            last_verified_height,
            value: scan_box.value,
            creation_height: scan_box.creation_height,
            tracker_nft_id: tracker_nft_id_from_r6,
        })
    }

    /// Process all unspent tracker boxes
    pub async fn process_tracker_boxes(&self) -> Result<Vec<TrackerBoxInfo>, TrackerScannerError> {
        let unspent_boxes = self.get_unspent_tracker_boxes().await?;
        let total_boxes = unspent_boxes.len();
        let mut processed_boxes = Vec::new();

        for scan_box in &unspent_boxes {
            match self.parse_tracker_box(scan_box) {
                Ok(tracker_box) => {
                    // Store the parsed box
                    self.tracker_storage
                        .store_tracker_box(&tracker_box)
                        .map_err(|e| {
                            TrackerScannerError::StoreError(format!(
                                "Failed to store tracker box: {:?}",
                                e
                            ))
                        })?;

                    processed_boxes.push(tracker_box);

                    debug!("Successfully processed tracker box: {}", scan_box.box_id);
                }
                Err(e) => {
                    warn!("Failed to parse tracker box {}: {}", scan_box.box_id, e);
                    // Continue processing other boxes
                }
            }
        }

        info!(
            "Processed {} tracker boxes ({} successful)",
            total_boxes,
            processed_boxes.len()
        );

        Ok(processed_boxes)
    }

    /// Get the latest tracker box ID from the blockchain query.
    /// Returns the box ID of the most recent tracker box (highest creation height)
    pub async fn get_latest_tracker_box_id(&self) -> Result<Option<String>, TrackerScannerError> {
        let unspent_boxes = self.get_unspent_tracker_boxes().await?;

        if unspent_boxes.is_empty() {
            return Ok(None);
        }

        // Find the box with the highest creation height
        let latest_box = unspent_boxes
            .into_iter()
            .max_by_key(|box_| box_.creation_height);

        Ok(latest_box.map(|box_| box_.box_id))
    }

    /// Update tracker state with processed boxes
    pub async fn update_tracker_state(
        &self,
        tracker_boxes: &[TrackerBoxInfo],
    ) -> Result<(), TrackerScannerError> {
        if tracker_boxes.is_empty() {
            debug!("No tracker boxes to update state");
            return Ok(());
        }

        // For now, we'll just log the boxes
        // In a real implementation, this would update the tracker state manager
        // with cross-verification logic
        for tracker_box in tracker_boxes {
            debug!(
                "Tracker box: id={}, pubkey={}, commitment={}, height={}",
                &tracker_box.box_id[..16], // First 16 chars of box ID
                &tracker_box.tracker_pubkey[..16], // First 16 chars of pubkey
                &tracker_box.state_commitment[..16], // First 16 chars of commitment
                tracker_box.last_verified_height
            );
        }

        debug!("Updated tracker state with {} boxes", tracker_boxes.len());

        Ok(())
    }

    /// Get last scanned height
    pub async fn last_scanned_height(&self) -> u64 {
        let inner = self.inner.lock().await;
        inner.last_scanned_height
    }

    /// Get current blockchain height from cache or Ergo node
    /// Uses cached value if less than 10 minutes old, otherwise fetches from node
    pub async fn get_current_height(&self) -> Result<u64, TrackerScannerError> {
        const CACHE_TTL_MS: u64 = 600_000; // 10 minutes in milliseconds

        // Check if we have a cached height
        match self.metadata_storage.get_blockchain_height() {
            Ok(Some((cached_height, cached_timestamp))) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                if now.saturating_sub(cached_timestamp) < CACHE_TTL_MS {
                    debug!("Using cached blockchain height: {}", cached_height);
                    return Ok(cached_height);
                }

                debug!("Cached blockchain height expired, fetching from node");
            }
            Ok(None) => {
                debug!("No cached blockchain height found, fetching from node");
            }
            Err(e) => {
                warn!("Failed to read cached blockchain height: {:?}", e);
            }
        }

        // Fetch from node
        let url = format!("{}/info", self.config.node_url);

        let request = self.request_builder(reqwest::Method::GET, &url)?;
        let response = self
            .execute_request(request)
            .await
            .map_err(|e| TrackerScannerError::HttpError(format!("Failed to get height: {}", e)))?;

        if !response.status().is_success() {
            return Err(TrackerScannerError::NodeError(format!(
                "Failed to get height: {}",
                response.status()
            )));
        }

        let info: serde_json::Value = response.json().map_err(|e| {
            TrackerScannerError::JsonError(format!("Failed to parse height: {}", e))
        })?;

        let height = info["fullHeight"].as_u64().ok_or_else(|| {
            TrackerScannerError::JsonError("Missing fullHeight in response".to_string())
        })?;

        // Store in cache with current timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Err(e) = self.metadata_storage.store_blockchain_height(height, now) {
            warn!("Failed to cache blockchain height: {:?}", e);
        }

        Ok(height)
    }

    /// Start the tracker scanner (single scan)
    pub async fn start_tracker_scanner(&self) -> Result<(), TrackerScannerError> {
        info!("Starting tracker scanner...");

        // Mark scanner as active
        {
            let mut inner = self.inner.lock().await;
            inner.scan_active = true;
        }

        // Process tracker boxes directly, no scan registration required
        match self.process_tracker_boxes().await {
            Ok(tracker_boxes) => {
                if let Err(e) = self.update_tracker_state(&tracker_boxes).await {
                    error!("Failed to update tracker state: {}", e);
                } else {
                    info!("Tracker scanner completed successfully");
                }
            }
            Err(e) => {
                error!("Failed to process tracker boxes: {}", e);
            }
        }

        Ok(())
    }

    /// Stop the tracker scanner
    pub async fn stop_tracker_scanner(&self) -> Result<(), TrackerScannerError> {
        info!("Stopping tracker scanner...");

        {
            let mut inner = self.inner.lock().await;
            inner.scan_active = false;
        }

        info!("Tracker scanner stopped successfully");
        Ok(())
    }
}

/// Create a new tracker server state with default configuration
pub fn create_tracker_server_state(
    config: TrackerNodeConfig,
    metadata_storage: ScannerMetadataStorage,
    tracker_storage: TrackerStorage,
) -> TrackerServerState {
    let inner = TrackerServerStateInner {
        _current_height: 0,
        last_scanned_height: config.start_height.unwrap_or(0),
        scan_active: false,
    };

    TrackerServerState {
        config,
        inner: Arc::new(Mutex::new(inner)),
        client: Client::new(),
        metadata_storage,
        tracker_storage,
    }
}
