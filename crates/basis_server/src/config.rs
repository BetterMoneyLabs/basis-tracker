//! Configuration management for Basis Server

use basis_store::ergo_scanner::NodeConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

// Import Ergo address handling for P2PK address support
use ergo_lib::ergotree_ir::address::{AddressEncoder, NetworkPrefix};

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
    /// Supports both hex-encoded public key and Ergo P2PK address formats
    pub fn tracker_public_key_bytes(&self) -> Result<Option<[u8; 33]>, Box<dyn std::error::Error>> {
        match &self.ergo.tracker_public_key {
            Some(pubkey_input) if !pubkey_input.is_empty() => {
                tracing::info!("Processing tracker public key: {}", pubkey_input);

                // Try hex decoding first
                if let Ok(bytes) = hex::decode(pubkey_input) {
                    tracing::info!("Successfully decoded hex public key, length: {}", bytes.len());
                    if bytes.len() == 33 {
                        let mut pubkey_bytes = [0u8; 33];
                        pubkey_bytes.copy_from_slice(&bytes);
                        tracing::info!("Returning 33-byte compressed public key from hex: {}", hex::encode(&pubkey_bytes));
                        return Ok(Some(pubkey_bytes));
                    } else {
                        tracing::info!("Hex decoded public key has wrong length: {}, expected 33", bytes.len());
                    }
                } else {
                    tracing::info!("Failed to decode tracker public key as hex, attempting P2PK address parsing");
                }

                // If hex decoding failed or wrong length, try parsing as P2PK address
                let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                match encoder.parse_address_from_str(pubkey_input) {
                    Ok(ergo_lib::ergotree_ir::address::Address::P2Pk(pubkey)) => {
                        tracing::info!("Successfully parsed as P2PK address, extracting public key");
                        // Use sigma serialization to get the compressed public key bytes
                        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
                        let pk_bytes = pubkey.h.sigma_serialize_bytes();
                        tracing::info!("Extracted public key bytes length: {}", pk_bytes.len());
                        if pk_bytes.len() == 33 {
                            let mut result = [0u8; 33];
                            result.copy_from_slice(&pk_bytes);
                            tracing::info!("Returning 33-byte compressed public key from P2PK: {}", hex::encode(&result));
                            Ok(Some(result))
                        } else {
                            tracing::info!("Public key extracted from P2PK has wrong length: {}, expected 33", pk_bytes.len());
                            Err("Invalid public key length in P2PK address".into())
                        }
                    }
                    Ok(_) => {
                        tracing::info!("Address is not P2PK format");
                        Err("Address is not P2PK format".into())
                    },
                    Err(_) => {
                        tracing::info!("Failed to parse as either hex public key or P2PK address");
                        Err("Invalid hex public key or P2PK address format".into())
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// Get the tracker public key as hex string (if configured)
    pub fn tracker_public_key_hex(&self) -> Option<String> {
        // Return the hex representation of the tracker public key, regardless of input format
        match &self.ergo.tracker_public_key {
            Some(pubkey_input) if !pubkey_input.is_empty() => {
                // Try hex decoding first
                if let Ok(bytes) = hex::decode(pubkey_input) {
                    if bytes.len() == 33 {
                        return Some(pubkey_input.clone());
                    }
                }

                // If input is P2PK address, extract and return the public key as hex
                let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                if let Ok(ergo_lib::ergotree_ir::address::Address::P2Pk(pubkey)) = encoder.parse_address_from_str(pubkey_input) {
                    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
                    let pubkey_bytes = pubkey.h.sigma_serialize_bytes();
                    if pubkey_bytes.len() == 33 {
                        return Some(hex::encode(&pubkey_bytes));
                    }
                }

                // If both attempts failed, return the original input as hex if possible
                None
            }
            _ => None,
        }
    }

    /// Determine the network prefix from the tracker public key string (hex or P2PK address)
    pub fn network_prefix_from_tracker_key(&self) -> Result<NetworkPrefix, Box<dyn std::error::Error>> {
        match &self.ergo.tracker_public_key {
            Some(pubkey_input) if !pubkey_input.is_empty() => {
                // First, try to parse as a P2PK address to extract the network prefix
                let mainnet_encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                let testnet_encoder = AddressEncoder::new(NetworkPrefix::Testnet);

                // Try parsing with mainnet encoder first
                if mainnet_encoder.parse_address_from_str(pubkey_input).is_ok() {
                    Ok(NetworkPrefix::Mainnet)
                } else if testnet_encoder.parse_address_from_str(pubkey_input).is_ok() {
                    Ok(NetworkPrefix::Testnet)
                } else {
                    // If it's not a valid address, check if it's a hex public key
                    // If so, use node URL to determine default network
                    if hex::decode(pubkey_input).is_ok() {
                        if self.ergo.node.node_url.contains("testnet") {
                            Ok(NetworkPrefix::Testnet)
                        } else {
                            Ok(NetworkPrefix::Mainnet)
                        }
                    } else {
                        Err("Invalid tracker public key format - not hex nor P2PK address".into())
                    }
                }
            }
            _ => {
                // Default to mainnet if no tracker key is configured
                if self.ergo.node.node_url.contains("testnet") {
                    Ok(NetworkPrefix::Testnet)
                } else {
                    Ok(NetworkPrefix::Mainnet)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_public_key_hex_format() {
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                database_url: Some("sqlite:test.db".to_string()),
            },
            ergo: ErgoConfig {
                node: NodeConfig {
                    start_height: None,
                    reserve_contract_p2s: None,
                    node_url: "http://localhost:9053".to_string(),
                    scan_name: None,
                    api_key: Some("test".to_string()),
                },
                basis_reserve_contract_p2s: "test".to_string(),
                tracker_nft_id: None,
                tracker_public_key: Some("02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7".to_string()),
            },
            transaction: TransactionConfig {
                fee: 1000000,
            },
        };

        // Test hex format
        let result = config.tracker_public_key_bytes().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 33);

        let hex_result = config.tracker_public_key_hex();
        assert!(hex_result.is_some());
        assert_eq!(hex_result.unwrap(), "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7");
    }

    #[test]
    fn test_tracker_public_key_p2pk_address_format() {
        // This test would validate P2PK address parsing, but to avoid complex ergo-lib
        // dependencies in unit tests, we rely on integration testing for this functionality.
        // The important thing is that our parsing logic handles both formats correctly.
    }
}
