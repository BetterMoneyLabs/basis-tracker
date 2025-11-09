//! Configuration management for Basis Server

use basis_store::ergo_scanner::NodeConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Ergo node configuration
    pub ergo: ErgoConfig,
}

/// Server-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host address to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Database path (if using persistent storage)
    pub database_url: Option<String>,
}

/// Ergo blockchain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoConfig {
    /// Ergo node configuration
    pub node: NodeConfig,
    /// Basis contract template (hex-encoded)
    pub basis_contract_template: String,
    /// Starting block height for scanning (legacy, use node.start_height instead)
    pub start_height: u64,
    /// Tracker NFT ID (hex-encoded) - identifies the tracker server for reserve contracts
    pub tracker_nft_id: Option<String>,
}

impl AppConfig {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::from(path.as_ref()))
            .build()?;

        config.try_deserialize()
    }

    /// Load configuration from default locations
    pub fn load() -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            // Default configuration
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3048)?
            .set_default("server.database_url", "sqlite:data/basis.db")?
            .set_default("ergo.start_height", 0)?
            // Node configuration defaults
            .set_default("ergo.node.start_height", "")?
            .set_default("ergo.node.contract_template", "")?
            .set_default("ergo.node.node_url", "http://159.89.116.15:11088")?
            .set_default("ergo.node.scan_name", "Basis Reserve Scanner")?
            .set_default("ergo.node.api_key", "hello")?
            // Environment variables
            .add_source(config::Environment::with_prefix("BASIS"))
            // Configuration file
            .add_source(config::File::with_name("config/basis").required(false))
            .build()?;

        config.try_deserialize()
    }

    /// Get the socket address for the server
    pub fn socket_addr(&self) -> std::net::SocketAddr {
        format!("{}:{}", self.server.host, self.server.port)
            .parse()
            .expect("Invalid socket address")
    }

    /// Get the Ergo node configuration
    pub fn ergo_node_config(&self) -> NodeConfig {
        let mut node_config = self.ergo.node.clone();
        // Set start_height from legacy field if not already set
        if node_config.start_height.is_none() && self.ergo.start_height > 0 {
            node_config.start_height = Some(self.ergo.start_height);
        }
        node_config
    }

    /// Get the Basis contract template bytes
    pub fn basis_contract_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(&self.ergo.basis_contract_template)
    }

    /// Get the tracker NFT ID bytes (if configured)
    pub fn tracker_nft_bytes(&self) -> Result<Option<Vec<u8>>, hex::FromHexError> {
        match &self.ergo.tracker_nft_id {
            Some(nft_id) if !nft_id.is_empty() => hex::decode(nft_id).map(Some),
            _ => Ok(None),
        }
    }
}
