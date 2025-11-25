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
    /// Transaction configuration
    pub transaction: TransactionConfig,
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
    /// Basis reserve contract P2S address
    pub basis_reserve_contract_p2s: String,
    /// Tracker NFT ID (hex-encoded) - identifies the tracker server for reserve contracts
    pub tracker_nft_id: Option<String>,
    /// Tracker server's public key for the Ergo blockchain (hex-encoded, 33 bytes for compressed format)
    pub tracker_public_key: Option<String>,
}

/// Transaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Default transaction fee in nanoERG (0.001 ERG = 1,000,000 nanoERG)
    pub fee: u64,
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
            // Node configuration defaults
            .set_default("ergo.node.start_height", "")?
            .set_default("ergo.node.reserve_contract_p2s", "")?
            .set_default("ergo.node.node_url", "http://159.89.116.15:11088")?
            .set_default("ergo.node.scan_name", "Basis Reserve Scanner")?
            .set_default("ergo.node.api_key", "hello")?
            // Transaction configuration defaults
            .set_default("transaction.fee", 1000000)? // 0.001 ERG
            // Tracker public key (optional)
            .set_default("ergo.tracker_public_key", "")?
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
        self.ergo.node.clone()
    }

    /// Get the Basis reserve contract P2S address
    pub fn basis_reserve_contract_p2s(&self) -> &str {
        &self.ergo.basis_reserve_contract_p2s
    }

    /// Get the tracker NFT ID bytes (if configured)
    pub fn tracker_nft_bytes(&self) -> Result<Option<Vec<u8>>, hex::FromHexError> {
        match &self.ergo.tracker_nft_id {
            Some(nft_id) if !nft_id.is_empty() => hex::decode(nft_id).map(Some),
            _ => Ok(None),
        }
    }

    /// Get the default transaction fee
    pub fn transaction_fee(&self) -> u64 {
        self.transaction.fee
    }

    /// Get the tracker public key bytes (if configured)
    pub fn tracker_public_key_bytes(&self) -> Result<Option<[u8; 33]>, hex::FromHexError> {
        match &self.ergo.tracker_public_key {
            Some(pubkey_hex) if !pubkey_hex.is_empty() => {
                let bytes = hex::decode(pubkey_hex)?;
                if bytes.len() != 33 {
                    return Err(hex::FromHexError::InvalidStringLength);
                }
                let mut pubkey_bytes = [0u8; 33];
                pubkey_bytes.copy_from_slice(&bytes);
                Ok(Some(pubkey_bytes))
            }
            _ => Ok(None),
        }
    }

    /// Get the tracker public key as hex string (if configured)
    pub fn tracker_public_key_hex(&self) -> Option<&str> {
        match &self.ergo.tracker_public_key {
            Some(pubkey_hex) if !pubkey_hex.is_empty() => Some(pubkey_hex),
            _ => None,
        }
    }
}
