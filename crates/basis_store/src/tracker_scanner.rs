//! Tracker box scanner for monitoring Basis tracker state commitment boxes
//! This module provides blockchain integration using /scan API with containsAsset rule

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::Client;

use crate::{
    persistence::{ScannerMetadataStorage, TrackerStorage},
    TrackerStateManager,
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

        // Register scan using containsAsset predicate with tracker NFT ID
        let scan_payload = serde_json::json!({
            "scanName": scan_name,
            "walletInteraction": "off",
            "trackingRule": {
                "predicate": "containsAsset",
                "assetId": tracker_nft_id
            },
            "removeOffchain": true
        });

        let url = format!("{}/scan/register", self.config.node_url);
        
        info!("Registering tracker scan with NFT ID: {}", tracker_nft_id);
        
        let response = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&scan_payload)
            .send()
            .await
            .map_err(|e| TrackerScannerError::HttpError(format!("Failed to send request: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(TrackerScannerError::NodeError(format!(
                "Scan registration failed with status {}: {}",
                status,
                error_text
            )));
        }

        let scan_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TrackerScannerError::JsonError(format!("Failed to parse response: {}", e)))?;

        let scan_id = scan_response["scanId"]
            .as_i64()
            .ok_or_else(|| TrackerScannerError::JsonError("Missing scanId in response".to_string()))? as i32;

        info!("Successfully registered tracker scan with ID: {}", scan_id);

        // Store scan ID for persistence
        self.metadata_storage.store_scan_id(scan_name, scan_id)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to store scan ID: {:?}", e)))?;

        // Update inner state
        let mut inner = self.inner.lock().await;
        inner.scan_id = Some(scan_id);
        inner.scan_active = true;

        Ok(scan_id)
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

    /// Verify scan registration is still active
    pub async fn verify_scan_registration(&self) -> Result<bool, TrackerScannerError> {
        let scan_name = self.config.scan_name.as_deref()
            .unwrap_or("tracker_boxes");

        let stored_scan_id = self.metadata_storage.get_scan_id(scan_name)
            .map_err(|e| TrackerScannerError::StoreError(format!("Failed to get scan ID: {:?}", e)))?;

        if let Some(scan_id) = stored_scan_id {
            let url = format!("{}/scan/listAll", self.config.node_url);
            
            let response = self
                .request_builder(reqwest::Method::GET, &url)
                .send()
                .await
                .map_err(|e| TrackerScannerError::HttpError(format!("Failed to list scans: {}", e)))?;

            if !response.status().is_success() {
                return Ok(false);
            }

            let scans: Vec<serde_json::Value> = response
                .json()
                .await
                .map_err(|e| TrackerScannerError::JsonError(format!("Failed to parse scans: {}", e)))?;

            // Check if our scan ID is still in the list
            for scan in scans {
                if let Some(id) = scan["scanId"].as_i64() {
                    if id == scan_id as i64 {
                        return Ok(true);
                    }
                }
            }
        }

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