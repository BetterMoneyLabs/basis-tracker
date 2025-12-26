//! Tracker box scanner for monitoring Basis tracker state commitment boxes
//! This module provides blockchain integration using /scan API with containsAsset rule

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::Client;

use crate::{
    ergo_scanner::ScanBox,
    persistence::{ScannerMetadataStorage, TrackerStorage},
    TrackerBoxInfo, TrackerStateManager,
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
struct TrackerServerStateInner {
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
    pub scan_id: Option<i32>,
    pub last_scan_verification: Option<std::time::SystemTime>,
}

/// Server state for tracker scanner
/// Uses real blockchain integration with proper synchronization
pub struct TrackerServerState {
    pub config: TrackerNodeConfig,
    pub inner: Arc<Mutex<TrackerServerStateInner>>,
    pub client: Client,
    pub tracker_state: TrackerStateManager,
    pub metadata_storage: ScannerMetadataStorage,
    pub tracker_storage: TrackerStorage,
}

impl Clone for TrackerServerState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            inner: Arc::clone(&self.inner),
            client: self.client.clone(),
            tracker_state: TrackerStateManager::new(), // Create new instance since it doesn't implement Clone
            metadata_storage: self.metadata_storage.clone(),
            tracker_storage: self.tracker_storage.clone(),
        }
    }
}

impl TrackerServerState {
    /// Create HTTP request builder with API key header if configured
    fn request_builder(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        debug!("Tracker request method: {}, URL: {}", method, url);
        let mut builder = self.client.request(method, url);
        
        if let Some(api_key) = &self.config.api_key {
            builder = builder.header("api_key", api_key);
        }
        
        builder
    }

    /// Register scan for tracker boxes using containsAsset rule
    pub async fn register_tracker_scan(&self) -> Result<i32, TrackerScannerError> {
        let tracker_nft_id = self.config.tracker_nft_id.as_ref()
            .ok_or(TrackerScannerError::MissingTrackerNftId)?;

        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        // Check if scan ID already exists in database
        debug!(
            "Checking for existing scan ID in database for scan name: '{}'",
            scan_name
        );
        match self.metadata_storage.get_scan_id(scan_name) {
            Ok(Some(stored_scan_id)) => {
                info!("Found existing scan ID in database: {}", stored_scan_id);

                // Verify the scan still exists on the node (only every 4 hours)
                let should_verify = self.should_verify_scan().await;
                if should_verify {
                    debug!("Verifying scan ID {} exists on Ergo node", stored_scan_id);
                    match self.verify_scan_registration().await {
                        Ok(true) => {
                            info!("Using existing scan ID: {}", stored_scan_id);
                            // Update verification timestamp
                            self.update_scan_verification_time().await;
                            // Update inner state with the validated scan ID
                            {
                                let mut inner = self.inner.lock().await;
                                inner.scan_id = Some(stored_scan_id);
                                inner.scan_active = true;
                            }
                            return Ok(stored_scan_id);
                        }
                        Ok(false) => {
                            warn!(
                                "Stored scan ID {} no longer exists on node, re-registering",
                                stored_scan_id
                            );
                            self.metadata_storage
                                .remove_scan_id(scan_name)
                                .map_err(|e| {
                                    TrackerScannerError::StoreError(format!(
                                        "Failed to remove invalid scan ID: {:?}",
                                        e
                                    ))
                                })?;
                            info!(
                                "Removed invalid scan ID {} from database, proceeding to registration",
                                stored_scan_id
                            );
                        }
                        Err(e) => {
                            // If verification fails due to scan list endpoint being unavailable (400/404),
                            // or JSON parsing errors, assume the scan still exists and continue using it
                            if e.to_string().contains("400") || e.to_string().contains("404") || e.to_string().contains("Failed to parse scan list") {
                                warn!("Scan list endpoint unavailable or JSON parsing failed ({}), assuming scan ID {} still exists", e, stored_scan_id);
                                // Update verification timestamp
                                self.update_scan_verification_time().await;
                                // Update inner state with the existing scan ID
                                {
                                    let mut inner = self.inner.lock().await;
                                    inner.scan_id = Some(stored_scan_id);
                                    inner.scan_active = true;
                                }
                                return Ok(stored_scan_id);
                            } else {
                                error!(
                                    "Failed to verify existing scan ID {}: {}",
                                    stored_scan_id, e
                                );
                                warn!("Unable to verify scan ID, forcing re-registration");
                                self.metadata_storage
                                    .remove_scan_id(scan_name)
                                    .map_err(|e| {
                                        TrackerScannerError::StoreError(format!(
                                            "Failed to remove scan ID: {:?}",
                                            e
                                        ))
                                    })?;
                                info!("Forcing scan registration due to verification failure");
                            }
                        }
                    }
                } else {
                    debug!("Skipping scan ID verification (last verified less than 4 hours ago)");
                    // Update inner state with the existing scan ID without verification
                    {
                        let mut inner = self.inner.lock().await;
                        inner.scan_id = Some(stored_scan_id);
                        inner.scan_active = true;
                    }
                    return Ok(stored_scan_id);
                }
            }
            Ok(None) => {
                info!("No existing scan ID found in database for scan name: '{}', proceeding with new registration", scan_name);
            }
            Err(e) => {
                error!("Failed to get scan ID from database: {:?}", e);
                info!("Database error, proceeding with new registration");
            }
        }

        // Register scan using containsAsset predicate with tracker NFT ID
        let scan_payload = serde_json::json!({
            "scanName": scan_name,
            "walletInteraction": "shared",
            "trackingRule": {
                "predicate": "containsAsset",
                "assetId": tracker_nft_id
            },
            "removeOffchain": true
        });

        // Log scan registration (INFO level) and JSON payload (DEBUG level)
        info!("Registering new tracker scan with name: {}", scan_name);
        debug!("Tracker scan registration JSON payload: {}", scan_payload);

        let url = format!("{}/scan/register", self.config.node_url);

        // Create request builder and log request details
        let request_builder = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&scan_payload);

        // Log exact HTTP request details
        info!("Sending HTTP POST request to Ergo node: {}", url);
        info!("Request headers: API key present: {}", self.config.api_key.is_some());
        info!("Request body (JSON): {}", scan_payload);
        debug!("Sending scan registration request to: {}", url);
        debug!(
            "Request headers include: {}",
            if self.config.api_key.is_some() {
                "API key"
            } else {
                "NO API key"
            }
        );

        let response = request_builder.send().await.map_err(|e| {
            error!("HTTP request failed: {}", e);
            error!(
                "Request details - URL: {}, Method: POST, Headers: API key present: {}",
                url,
                self.config.api_key.is_some()
            );
            TrackerScannerError::HttpError(format!("Failed to register scan: {}", e))
        })?;

        // Log response details
        let status = response.status();
        debug!("Response status: {}", status);
        debug!("Response headers: {:?}", response.headers());

        if !status.is_success() {
            // Try to read response body for more details
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            error!(
                "Scan registration failed with status: {}. Response body: {}",
                status, response_text
            );
            error!("Full request details:");
            error!("  URL: {}", url);
            error!("  Method: POST");
            error!("  API key present: {}", self.config.api_key.is_some());
            error!("  Payload: {}", scan_payload);
            error!("  Response status: {}", status);
            error!("  Response body: {}", response_text);
            return Err(TrackerScannerError::NodeError(format!(
                "Scan registration failed with status: {}. Response: {}",
                status, response_text
            )));
        }

        let result: serde_json::Value = response.json().await.map_err(|e| {
            error!("Failed to parse scan registration response JSON: {}", e);
            TrackerScannerError::JsonError(format!("Failed to parse scan registration response: {}", e))
        })?;

        debug!(
            "Scan registration successful response: {}",
            serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Unable to format JSON".to_string())
        );

        let scan_id = result["scanId"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| {
                error!(
                    "Failed to get scan ID from registration response. Response was: {}",
                    result
                );
                TrackerScannerError::Generic(
                    "Failed to get scan ID from registration response".to_string(),
                )
            })?;

        // Store scan ID in database and update inner state
        self.metadata_storage
            .store_scan_id(scan_name, scan_id)
            .map_err(|e| {
                TrackerScannerError::StoreError(format!("Failed to store scan ID: {:?}", e))
            })?;

        // Update inner state with the new scan ID
        {
            let mut inner = self.inner.lock().await;
            inner.scan_id = Some(scan_id);
            inner.scan_active = true;
        }

        info!("Registered and stored tracker scan with ID: {}", scan_id);

        Ok(scan_id)
    }

    /// Get unspent tracker boxes from the registered scan
    pub async fn get_unspent_tracker_boxes(&self) -> Result<Vec<ScanBox>, TrackerScannerError> {
        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        let scan_id = self.metadata_storage.get_scan_id(scan_name)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to get scan ID: {:?}", e)))?;

        let scan_id = scan_id.ok_or_else(|| TrackerScannerError::Generic("Scan not registered".to_string()))?;

        let url = format!("{}/scan/unspentBoxes/{}", self.config.node_url, scan_id);
        
        debug!("Fetching unspent tracker boxes for scan ID: {}", scan_id);
        
        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| TrackerScannerError::HttpError(format!("Failed to fetch boxes: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(TrackerScannerError::NodeError(format!(
                "Failed to get unspent boxes with status {}: {}",
                status,
                error_text
            )));
        }

        let boxes: Vec<ScanBox> = response
            .json()
            .await
            .map_err(|e| TrackerScannerError::JsonError(format!("Failed to parse boxes: {}", e)))?;

        info!("Retrieved {} unspent tracker boxes", boxes.len());
        
        Ok(boxes)
    }

    /// Parse a ScanBox into TrackerBoxInfo
    pub fn parse_tracker_box(&self, scan_box: &ScanBox) -> Result<TrackerBoxInfo, TrackerScannerError> {
        let tracker_nft_id = self.config.tracker_nft_id.as_ref()
            .ok_or(TrackerScannerError::MissingTrackerNftId)?;

        // Validate that the box contains the tracker NFT
        let has_tracker_nft = scan_box.assets.iter()
            .any(|asset| asset.token_id == *tracker_nft_id && asset.amount >= 1);

        if !has_tracker_nft {
            return Err(TrackerScannerError::MissingTrackerNft);
        }

        // Extract data from registers
        let tracker_pubkey = scan_box.additional_registers.get("R4")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R4".to_string()))?
            .clone();

        let state_commitment = scan_box.additional_registers.get("R5")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R5".to_string()))?
            .clone();

        let last_verified_height_str = scan_box.additional_registers.get("R6")
            .ok_or_else(|| TrackerScannerError::MissingRegister("R6".to_string()))?;

        let last_verified_height = last_verified_height_str.parse::<u64>()
            .map_err(|e| TrackerScannerError::InvalidRegisterData(format!("Invalid R6 register: {}", e)))?;

        // Validate register data (basic sanity checks)
        if tracker_pubkey.len() != 66 { // 33 bytes hex encoded
            return Err(TrackerScannerError::InvalidRegisterData(
                format!("Invalid tracker pubkey length: {} (expected 66 hex chars)", tracker_pubkey.len())
            ));
        }

        if state_commitment.len() != 64 { // 32 bytes hex encoded
            return Err(TrackerScannerError::InvalidRegisterData(
                format!("Invalid state commitment length: {} (expected 64 hex chars)", state_commitment.len())
            ));
        }

        Ok(TrackerBoxInfo {
            box_id: scan_box.box_id.clone(),
            tracker_pubkey,
            state_commitment,
            last_verified_height,
            value: scan_box.value,
            creation_height: scan_box.creation_height,
            tracker_nft_id: tracker_nft_id.clone(),
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
                    self.tracker_storage.store_tracker_box(&tracker_box)
                        .map_err(|e| TrackerScannerError::StoreError(format!("Failed to store tracker box: {:?}", e)))?;
                    
                    processed_boxes.push(tracker_box);
                    
                    debug!("Successfully processed tracker box: {}", scan_box.box_id);
                }
                Err(e) => {
                    warn!("Failed to parse tracker box {}: {}", scan_box.box_id, e);
                    // Continue processing other boxes
                }
            }
        }

        info!("Processed {} tracker boxes ({} successful)", total_boxes, processed_boxes.len());
        
        Ok(processed_boxes)
    }

    /// Update tracker state with processed boxes
    pub async fn update_tracker_state(&self, tracker_boxes: &[TrackerBoxInfo]) -> Result<(), TrackerScannerError> {
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

    /// Deregister tracker scan
    pub async fn deregister_tracker_scan(&self) -> Result<(), TrackerScannerError> {
        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        let scan_id = self.metadata_storage.get_scan_id(scan_name)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to get scan ID: {:?}", e)))?;

        if let Some(scan_id) = scan_id {
            let url = format!("{}/scan/deregister", self.config.node_url);
            let deregister_payload = serde_json::json!({
                "scanId": scan_id
            });

            info!("Deregistering tracker scan with ID: {}", scan_id);

            let response = self
                .request_builder(reqwest::Method::POST, &url)
                .json(&deregister_payload)
                .send()
                .await
                .map_err(|e| TrackerScannerError::HttpError(format!("Failed to send request: {}", e)))?;

            if !response.status().is_success() {
                let error_text = response.text().await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                warn!("Failed to deregister scan {}: {}", scan_id, error_text);
            } else {
                info!("Successfully deregistered tracker scan with ID: {}", scan_id);
            }

            // Remove scan ID from storage
            self.metadata_storage.remove_scan_id(scan_name)
                .map_err(|e| TrackerScannerError::StoreError(format!("Failed to remove scan ID: {:?}", e)))?;

            // Update inner state
            let mut inner = self.inner.lock().await;
            inner.scan_id = None;
            inner.scan_active = false;
        }

        Ok(())
    }

    /// Get last scanned height
    pub async fn last_scanned_height(&self) -> u64 {
        let inner = self.inner.lock().await;
        inner.last_scanned_height
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, TrackerScannerError> {
        let url = format!("{}/info", self.config.node_url);

        let response = self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| TrackerScannerError::HttpError(format!("Failed to get height: {}", e)))?;

        if !response.status().is_success() {
            return Err(TrackerScannerError::NodeError(format!(
                "Failed to get height: {}",
                response.status()
            )));
        }

        let info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TrackerScannerError::JsonError(format!("Failed to parse height: {}", e)))?;

        let height = info["fullHeight"]
            .as_u64()
            .ok_or_else(|| TrackerScannerError::JsonError("Missing fullHeight in response".to_string()))?;

        Ok(height)
    }

    /// Check if scan verification is needed (every 4 hours)
    async fn should_verify_scan(&self) -> bool {
        let inner = self.inner.lock().await;
        match inner.last_scan_verification {
            Some(last_verification) => {
                let now = std::time::SystemTime::now();
                let duration_since_last = now.duration_since(last_verification).unwrap_or_default();
                duration_since_last >= std::time::Duration::from_secs(4 * 60 * 60) // 4 hours
            }
            None => true, // Never verified before
        }
    }

    /// Update the last scan verification timestamp
    async fn update_scan_verification_time(&self) {
        let mut inner = self.inner.lock().await;
        inner.last_scan_verification = Some(std::time::SystemTime::now());
    }

    /// Verify scan registration is still active
    pub async fn verify_scan_registration(&self) -> Result<bool, TrackerScannerError> {
        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        let stored_scan_id = self.metadata_storage.get_scan_id(scan_name)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to get scan ID: {:?}", e)))?;

        if let Some(scan_id) = stored_scan_id {
            let url = format!("{}/scan/listAll", self.config.node_url);
            debug!("Verifying scan exists - URL: {}", url);
            debug!("Looking for scan ID: {}", scan_id);
            info!("Sending HTTP GET request to Ergo node: {}", url);
            info!("Looking for scan ID: {}", scan_id);

            let response = self
                .request_builder(reqwest::Method::GET, &url)
                .send()
                .await;

            let response = match response {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Failed to send scan list request: {}", e);
                    // If we can't even connect, assume the scan exists to avoid re-registration
                    warn!(
                        "Network error connecting to scan list endpoint, assuming scan ID {} exists",
                        scan_id
                    );
                    return Ok(true);
                }
            };

            let status = response.status();
            debug!("Scan list response status: {}", status);

            // If scan list endpoint is not available (400/404), assume scan exists
            // This handles nodes that don't support scan listing
            if status == 400 || status == 404 {
                info!(
                    "Scan list endpoint not available (status: {}), assuming scan ID {} exists",
                    status, scan_id
                );
                return Ok(true);
            }

            if !status.is_success() {
                // Try to read response body for more details
                let response_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unable to read response body".to_string());
                // Log as warning instead of error since we're handling this gracefully
                warn!(
                    "Scan list request failed with status: {}. Response body: {}",
                    status, response_text
                );
                // For any other non-success status, assume scan exists to prevent re-registration
                warn!("Scan list request failed (status: {}), assuming scan ID {} exists to prevent re-registration", status, scan_id);
                return Ok(true);
            }

            let scans: serde_json::Value = response.json().await.map_err(|e| {
                error!("Failed to parse scan list JSON: {}", e);
                TrackerScannerError::JsonError(format!("Failed to parse scan list: {}", e))
            })?;

            debug!(
                "Scan list response: {}",
                serde_json::to_string_pretty(&scans)
                    .unwrap_or_else(|_| "Unable to format JSON".to_string())
            );

            // Check if our scan ID exists in the list
            if let Some(scans_array) = scans.as_array() {
                debug!("Found {} scans in list", scans_array.len());
                for scan in scans_array {
                    if let Some(id) = scan["scanId"].as_i64() {
                        debug!("Checking scan ID: {} against target: {}", id, scan_id);
                        if id == scan_id as i64 {
                            debug!("Scan ID {} found in scan list", scan_id);
                            return Ok(true);
                        }
                    } else {
                        debug!("Scan entry missing scanId: {:?}", scan);
                    }
                }
            } else {
                debug!("Scan list is not an array or is empty");
            }
        }

        debug!("Scan ID not found in scan list or no stored scan ID");
        Ok(false)
    }

    /// Re-register scan if needed
    pub async fn ensure_scan_registered(&self) -> Result<i32, TrackerScannerError> {
        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        // Check if we have a stored scan ID
        let stored_scan_id = self.metadata_storage.get_scan_id(scan_name)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to get scan ID: {:?}", e)))?;

        if let Some(scan_id) = stored_scan_id {
            // Verify the scan is still active
            if self.verify_scan_registration().await.unwrap_or(false) {
                info!("Tracker scan {} is still active", scan_id);

                // Update inner state
                let mut inner = self.inner.lock().await;
                inner.scan_id = Some(scan_id);
                inner.scan_active = true;

                return Ok(scan_id);
            } else {
                warn!("Stored tracker scan {} is no longer active, re-registering", scan_id);
            }
        }

        // Register new scan
        self.register_tracker_scan().await
    }

    /// Start the tracker scanner (single scan)
    pub async fn start_tracker_scanner(&self) -> Result<(), TrackerScannerError> {
        info!("Starting tracker scanner...");

        // Ensure scan is registered
        let scan_id = self.ensure_scan_registered().await?;
        info!("Tracker scan registered with ID: {}", scan_id);

        // Process tracker boxes
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
        
        // In a real implementation, we would signal the background task to stop
        // For now, we just deregister the scan
        self.deregister_tracker_scan().await?;
        
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
        current_height: 0,
        last_scanned_height: config.start_height.unwrap_or(0),
        scan_active: false,
        scan_id: None,
        last_scan_verification: None,
    };

    TrackerServerState {
        config,
        inner: Arc::new(Mutex::new(inner)),
        client: Client::new(),
        tracker_state: TrackerStateManager::new(),
        metadata_storage,
        tracker_storage,
    }
}