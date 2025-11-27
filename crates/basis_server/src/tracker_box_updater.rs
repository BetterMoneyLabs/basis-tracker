//! Tracker Box Updater Service
//!
//! This module implements a background service that periodically logs the R4 and R5 register values
//! of the tracker box every 10 minutes without submitting actual transactions to the Ergo blockchain.

use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{error, info};

/// Shared state for the tracker box updater
#[derive(Debug, Clone)]
pub struct SharedTrackerState {
    pub avl_root_digest: Arc<RwLock<[u8; 33]>>,
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
}

impl SharedTrackerState {
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new([0x02u8; 33])), // Initialize with compressed pubkey marker
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
}

impl Default for TrackerBoxUpdateConfig {
    fn default() -> Self {
        Self {
            update_interval_seconds: 600, // 10 minutes
            enabled: true,
            submit_transaction: false,
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

        let mut interval = tokio::time::interval(Duration::from_secs(config.update_interval_seconds));
        
        // Skip the first immediate tick to avoid immediate execution
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Access the shared state to get current values
                    let current_root = shared_tracker_state.get_avl_root_digest();
                    let tracker_pubkey = shared_tracker_state.get_tracker_pubkey();

                    // Construct register values
                    let r4_hex = hex::encode(&tracker_pubkey);
                    let r5_hex = hex::encode(&current_root);

                    // Log register values (initial implementation - no transaction submission)
                    info!(
                        "Tracker Box Update: R4={}, R5={}, timestamp={}, root_digest={}",
                        r4_hex,
                        r5_hex,
                        current_timestamp(),
                        hex::encode(&current_root)
                    );
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