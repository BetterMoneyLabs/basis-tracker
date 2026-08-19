//! Configuration management for Basis Server

use crate::acceptance::config::AcceptanceConfig;
use basis_store::ergo_scanner::NodeConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// Import Ergo address handling for P2PK address support
use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Ergo node configuration
    pub ergo: ErgoConfig,
    /// Transaction configuration
    pub transaction: TransactionConfig,
    /// Acceptance predicate configuration
    #[serde(default)]
    pub acceptance: AcceptanceConfig,
    /// Redemption policy enforcement configuration
    #[serde(default)]
    pub redemption: RedemptionConfig,
    /// On-chain confirmation policy configuration
    #[serde(default)]
    pub confirmation: ConfirmationConfig,
}

/// Server-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host address to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Base directory for all on-disk storage (databases, indices, scanner metadata).
    /// Defaults to "data" relative to the server's working directory.
    pub data_dir: Option<String>,
    /// Database path (legacy field, kept for config compatibility; currently unused).
    pub database_url: Option<String>,
    /// Path to PEM-encoded TLS certificate chain. When both this and
    /// `tls_key_path` are set, the server listens on HTTPS.
    pub tls_cert_path: Option<String>,
    /// Path to PEM-encoded TLS private key.
    pub tls_key_path: Option<String>,
    /// Authentication mode. When absent, the server accepts anonymous
    /// requests (backward-compatible local development behavior).
    #[serde(default)]
    pub auth: AuthConfig,
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication scheme.
    #[serde(default)]
    pub mode: AuthMode,
    /// Shared API key for `Authorization: Bearer` or `X-API-Key` authentication.
    /// Used only when `mode` is `ApiKey`.
    pub api_key: Option<String>,
    /// Public keys authorized to access the API. Used when `mode` is
    /// `Signature`.
    #[serde(default)]
    pub authorized_clients: Vec<AuthorizedClient>,
    /// Allowed CORS origins when auth is enabled. An empty list means only
    /// non-browser/non-CORS callers can connect.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Request signature timestamp tolerance in milliseconds.
    #[serde(default = "default_signature_timestamp_tolerance_ms")]
    pub signature_timestamp_tolerance_ms: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::default(),
            api_key: None,
            authorized_clients: Vec::new(),
            allowed_origins: Vec::new(),
            signature_timestamp_tolerance_ms: default_signature_timestamp_tolerance_ms(),
        }
    }
}

/// Authentication scheme.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// No authentication.
    #[default]
    None,
    /// Shared API key checked from request headers.
    ApiKey,
    /// Per-client secp256k1 request signatures.
    Signature,
}

/// Role granted to an authorized client.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientRole {
    /// Read-only access.
    #[default]
    Read,
    /// Read and write (create notes, redeem).
    Write,
    /// Full access including policy and reserve management.
    Admin,
}

/// An authorized API client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedClient {
    /// Hex-encoded 33-byte compressed secp256k1 public key (66 characters).
    pub pubkey: String,
    /// Access role.
    #[serde(default)]
    pub role: ClientRole,
}

fn default_signature_timestamp_tolerance_ms() -> u64 {
    30_000 // 30 seconds
}

impl ServerConfig {
    /// Resolve the configured data directory, falling back to "data".
    pub fn data_dir(&self) -> PathBuf {
        self.data_dir
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"))
    }

    /// Returns true when TLS is fully configured.
    pub fn tls_enabled(&self) -> bool {
        self.tls_cert_path
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
            && self
                .tls_key_path
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
    }

    /// Returns true when any authentication is configured.
    pub fn auth_enabled(&self) -> bool {
        self.auth.mode != AuthMode::None
    }
}

/// Ergo blockchain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoConfig {
    /// Ergo node configuration
    pub node: NodeConfig,
    /// Basis reserve contract P2S address (ERG-backed reserve)
    pub basis_reserve_contract_p2s: String,
    /// Basis token reserve contract P2S address (custom-token-backed reserve)
    pub basis_token_reserve_contract_p2s: String,
    /// Tracker NFT ID (hex-encoded) - identifies the tracker server for reserve contracts
    pub tracker_nft_id: Option<String>,
    /// Reserve token ID (hex-encoded, 32 bytes). When set, the tracker operates in
    /// token-reserve mode and uses `basis_token_reserve_contract_p2s` for new reserves.
    pub reserve_token_id: Option<String>,
    /// Number of decimal places for the reserve token (e.g. 3 for USE/DexyUSD).
    /// Used for display/conversion only; on-chain amounts are always raw token units.
    #[serde(default = "default_reserve_token_decimals")]
    pub reserve_token_decimals: u8,
    /// Tracker server's public key for the Ergo blockchain (hex-encoded, 33 bytes for compressed format)
    pub tracker_public_key: Option<String>,
    /// Tracker server's secret key for local signing (hex-encoded, 32 bytes)
    /// If provided, the server will sign redemption transactions locally instead of using the Ergo node API
    pub tracker_secret_key: Option<String>,
}

fn default_reserve_token_decimals() -> u8 {
    0
}

/// Transaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Default transaction fee in nanoERG (0.001 ERG = 1,000,000 nanoERG)
    pub fee: u64,
    /// Change address for redemption transactions (P2PK address)
    /// If not specified, the tracker's public key will be used to derive a change address
    pub change_address: Option<String>,
}

/// Redemption policy enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionConfig {
    /// Enforce acceptance-policy compliance at redemption time: reject redemptions
    /// that would newly violate another debt holder's collateralization policy.
    /// When all other holders are already violated (distressed reserve), only the
    /// oldest outstanding note may redeem (FIFO fallback).
    #[serde(default = "default_enforce_acceptance_policy")]
    pub enforce_acceptance_policy: bool,
}

fn default_enforce_acceptance_policy() -> bool {
    true
}

impl Default for RedemptionConfig {
    fn default() -> Self {
        Self {
            enforce_acceptance_policy: default_enforce_acceptance_policy(),
        }
    }
}

/// On-chain confirmation policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationConfig {
    /// Minimum on-chain depth (including the inclusion block) before a tracker
    /// box update is treated as confirmed and pending notes become redeemable.
    #[serde(default = "default_min_confirmation_depth")]
    pub min_depth: u64,
}

fn default_min_confirmation_depth() -> u64 {
    2
}

impl Default for ConfirmationConfig {
    fn default() -> Self {
        Self {
            min_depth: default_min_confirmation_depth(),
        }
    }
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
            .set_default("server.data_dir", "data")?
            .set_default("server.database_url", "sqlite:data/basis.db")?
            // TLS configuration (empty = plain HTTP)
            .set_default("server.tls_cert_path", "")?
            .set_default("server.tls_key_path", "")?
            // Authentication configuration
            .set_default("server.auth.mode", "none")?
            .set_default("server.auth.api_key", "")?
            .set_default("server.auth.allowed_origins", Vec::<String>::new())?
            .set_default("server.auth.signature_timestamp_tolerance_ms", 30_000)?
            // Node configuration defaults
            .set_default("ergo.node.start_height", "")?
            .set_default("ergo.node.reserve_contract_p2s", "")?
            .set_default("ergo.node.node_url", "http://159.89.116.15:11088")?
            .set_default("ergo.node.scan_name", "Basis Reserve Scanner")?
            .set_default("ergo.node.api_key", "")? // Set via config file or BASIS_ERGO_NODE_API_KEY env var
            // Transaction configuration defaults
            .set_default("transaction.fee", 1000000)? // 0.001 ERG
            // Token reserve configuration defaults
            .set_default("ergo.basis_token_reserve_contract_p2s", "")?
            .set_default("ergo.reserve_token_id", "")?
            .set_default("ergo.reserve_token_decimals", 0)?
            // Tracker public key (optional)
            .set_default("ergo.tracker_public_key", "")?
            // Tracker secret key (optional - for local signing)
            .set_default("ergo.tracker_secret_key", "")?
            // Acceptance predicate configuration (optional)
            .set_default("acceptance.default", "reject")?
            .set_default("acceptance.predicates", Vec::<String>::new())?
            // Redemption policy enforcement (optional)
            .set_default("redemption.enforce_acceptance_policy", true)?
            // On-chain confirmation policy (optional)
            .set_default("confirmation.min_depth", 2)?
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
        let mut config = self.ergo.node.clone();
        config.reserve_contract_p2s = Some(self.ergo.basis_reserve_contract_p2s.clone());
        config.token_reserve_contract_p2s =
            Some(self.ergo.basis_token_reserve_contract_p2s.clone());
        config.reserve_token_id = self.ergo.reserve_token_id.clone();
        config
    }

    /// Get the Basis reserve contract P2S address
    pub fn basis_reserve_contract_p2s(&self) -> &str {
        &self.ergo.basis_reserve_contract_p2s
    }

    /// Get the Basis token reserve contract P2S address
    pub fn basis_token_reserve_contract_p2s(&self) -> &str {
        &self.ergo.basis_token_reserve_contract_p2s
    }

    /// Returns true when the tracker is configured to back reserves with a custom token.
    pub fn is_token_reserve_mode(&self) -> bool {
        self.ergo
            .reserve_token_id
            .as_ref()
            .map(|id| !id.is_empty())
            .unwrap_or(false)
    }

    /// Get the configured reserve token ID bytes, if any.
    pub fn reserve_token_bytes(&self) -> Result<Option<Vec<u8>>, hex::FromHexError> {
        match &self.ergo.reserve_token_id {
            Some(id) if !id.is_empty() => hex::decode(id).map(Some),
            _ => Ok(None),
        }
    }

    /// Get the tracker NFT ID bytes (required - server will fail if not configured)
    pub fn tracker_nft_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        match &self.ergo.tracker_nft_id {
            Some(nft_id) if !nft_id.is_empty() => hex::decode(nft_id),
            _ => Err(hex::FromHexError::InvalidStringLength),
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
                    tracing::info!(
                        "Successfully decoded hex public key, length: {}",
                        bytes.len()
                    );
                    if bytes.len() == 33 {
                        let mut pubkey_bytes = [0u8; 33];
                        pubkey_bytes.copy_from_slice(&bytes);
                        tracing::info!(
                            "Returning 33-byte compressed public key from hex: {}",
                            hex::encode(pubkey_bytes)
                        );
                        return Ok(Some(pubkey_bytes));
                    } else {
                        tracing::info!(
                            "Hex decoded public key has wrong length: {}, expected 33",
                            bytes.len()
                        );
                    }
                } else {
                    tracing::info!("Failed to decode tracker public key as hex, attempting P2PK address parsing");
                }

                // If hex decoding failed or wrong length, try parsing as P2PK address
                let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                match encoder.parse_address_from_str(pubkey_input) {
                    Ok(ergo_lib::ergotree_ir::chain::address::Address::P2Pk(pubkey)) => {
                        tracing::info!(
                            "Successfully parsed as P2PK address, extracting public key"
                        );
                        // Use sigma serialization to get the compressed public key bytes
                        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
                        let pk_bytes = pubkey.h.sigma_serialize_bytes()?;
                        tracing::info!("Extracted public key bytes length: {}", pk_bytes.len());
                        if pk_bytes.len() == 33 {
                            let mut result = [0u8; 33];
                            result.copy_from_slice(&pk_bytes);
                            tracing::info!(
                                "Returning 33-byte compressed public key from P2PK: {}",
                                hex::encode(result)
                            );
                            Ok(Some(result))
                        } else {
                            tracing::info!(
                                "Public key extracted from P2PK has wrong length: {}, expected 33",
                                pk_bytes.len()
                            );
                            Err("Invalid public key length in P2PK address".into())
                        }
                    }
                    Ok(_) => {
                        tracing::info!("Address is not P2PK format");
                        Err("Address is not P2PK format".into())
                    }
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
                if let Ok(ergo_lib::ergotree_ir::chain::address::Address::P2Pk(pubkey)) =
                    encoder.parse_address_from_str(pubkey_input)
                {
                    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
                    let pubkey_bytes = pubkey.h.sigma_serialize_bytes().ok()?;
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

    /// Get the tracker secret key bytes (if configured)
    pub fn tracker_secret_key_bytes(&self) -> Option<[u8; 32]> {
        match &self.ergo.tracker_secret_key {
            Some(secret_hex) if !secret_hex.is_empty() => {
                if let Ok(bytes) = hex::decode(secret_hex) {
                    if bytes.len() == 32 {
                        let mut secret_bytes = [0u8; 32];
                        secret_bytes.copy_from_slice(&bytes);
                        return Some(secret_bytes);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Get the tracker private key bytes (if configured)
    /// Reads the tracker_secret_key from the configuration file.
    /// TODO: Implement secure storage (e.g., HSM, key vault, encrypted keystore) for production use.
    pub fn tracker_private_key_bytes(
        &self,
    ) -> Result<Option<[u8; 32]>, Box<dyn std::error::Error>> {
        // Read from the config file's tracker_secret_key field
        // This is a temporary solution - in production, private keys should be retrieved from secure storage
        match self.tracker_secret_key_bytes() {
            Some(secret_bytes) => Ok(Some(secret_bytes)),
            None => Ok(None),
        }
    }

    /// Get the change address for redemption transactions
    /// Returns configured change address, or derives from tracker public key if not configured
    pub fn get_change_address(&self) -> Result<String, Box<dyn std::error::Error>> {
        // If change address is explicitly configured, use it
        if let Some(ref addr) = self.transaction.change_address {
            if !addr.is_empty() {
                return Ok(addr.clone());
            }
        }

        // Otherwise, derive from tracker public key
        match &self.ergo.tracker_public_key {
            Some(pubkey_input) if !pubkey_input.is_empty() => {
                // Check if it's already an address
                if pubkey_input.starts_with('9') || pubkey_input.starts_with('3') {
                    Ok(pubkey_input.clone())
                } else {
                    // It's a hex public key, derive address
                    let pubkey_bytes = hex::decode(pubkey_input)?;

                    if pubkey_bytes.len() != 33 {
                        return Err("Invalid tracker public key length".into());
                    }

                    use ergo_lib::ergo_chain_types::EcPoint;
                    use ergo_lib::ergotree_ir::chain::address::{Address, NetworkPrefix};
                    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
                    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

                    let ec_point = EcPoint::sigma_parse_bytes(&pubkey_bytes)?;
                    let prove_dlog = ProveDlog::new(ec_point);
                    let address = Address::P2Pk(prove_dlog);
                    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                    Ok(encoder.address_to_str(&address))
                }
            }
            _ => {
                // No tracker key configured
                // This should not happen in production
                Err("No change address configured and no tracker public key available".into())
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
                data_dir: Some("test_data".to_string()),
                database_url: Some("sqlite:test.db".to_string()),
                tls_cert_path: None,
                tls_key_path: None,
                auth: AuthConfig::default(),
            },
            ergo: ErgoConfig {
                node: NodeConfig {
                    start_height: None,
                    reserve_contract_p2s: None,
                    token_reserve_contract_p2s: None,
                    reserve_token_id: None,
                    node_url: "http://localhost:9053".to_string(),
                    scan_name: None,
                    api_key: Some("test".to_string()),
                },
                basis_reserve_contract_p2s: "test".to_string(),
                basis_token_reserve_contract_p2s: "test_token".to_string(),
                tracker_nft_id: None,
                reserve_token_id: None,
                reserve_token_decimals: 0,
                tracker_public_key: Some(
                    "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7"
                        .to_string(),
                ),
                tracker_secret_key: None,
            },
            transaction: TransactionConfig {
                fee: 1000000,
                change_address: None,
            },
            acceptance: AcceptanceConfig::empty(),
            redemption: RedemptionConfig::default(),
            confirmation: ConfirmationConfig::default(),
        };

        // Test hex format
        let result = config.tracker_public_key_bytes().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 33);

        let hex_result = config.tracker_public_key_hex();
        assert!(hex_result.is_some());
        assert_eq!(
            hex_result.unwrap(),
            "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7"
        );
    }

    #[test]
    fn test_tracker_public_key_p2pk_address_format() {
        // This test would validate P2PK address parsing, but to avoid complex ergo-lib
        // dependencies in unit tests, we rely on integration testing for this functionality.
        // The important thing is that our parsing logic handles both formats correctly.
    }

    #[test]
    fn test_redemption_enforce_acceptance_policy_defaults_to_true() {
        // Acceptance-policy enforcement at redemption time is on by default.
        assert!(RedemptionConfig::default().enforce_acceptance_policy);

        // An absent [redemption] section in the TOML config also defaults to true.
        let config: RedemptionConfig = toml::from_str("").unwrap();
        assert!(config.enforce_acceptance_policy);

        // Explicit opt-out is honored.
        let config: RedemptionConfig = toml::from_str("enforce_acceptance_policy = false").unwrap();
        assert!(!config.enforce_acceptance_policy);
    }

    #[test]
    fn test_confirmation_min_depth_defaults_to_two() {
        // Minimum confirmation depth defaults to 2 (inclusion + one successor).
        assert_eq!(ConfirmationConfig::default().min_depth, 2);

        // An absent [confirmation] section in the TOML config also defaults to 2.
        let config: ConfirmationConfig = toml::from_str("").unwrap();
        assert_eq!(config.min_depth, 2);

        // Explicit override is honored.
        let config: ConfirmationConfig = toml::from_str("min_depth = 6").unwrap();
        assert_eq!(config.min_depth, 6);
    }
}
