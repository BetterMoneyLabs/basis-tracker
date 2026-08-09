//! Configuration management for Basis Server

use crate::acceptance::config::AcceptanceConfig;
use basis_store::ergo_scanner::NodeConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// Import Ergo address handling for P2PK address support
use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};

type DefaultConfigBuilder = config::ConfigBuilder<config::builder::DefaultState>;

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
}

/// Ergo blockchain configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct ErgoConfig {
    /// Ergo node configuration
    pub node: NodeConfig,
    /// Basis reserve contract P2S address
    pub basis_reserve_contract_p2s: String,
    /// Tracker NFT ID (hex-encoded) - identifies the tracker server for reserve contracts
    pub tracker_nft_id: Option<String>,
    /// One-shot operator approval to initialize a previously unbound, empty data
    /// directory for the configured tracker NFT. Defaults to false.
    #[serde(default)]
    pub allow_fresh_tracker_generation: bool,
    /// Tracker server's public key for the Ergo blockchain (hex-encoded, 33 bytes for compressed format)
    pub tracker_public_key: Option<String>,
    /// Tracker server's secret key for local signing (hex-encoded, 32 bytes)
    /// If provided, the server will sign redemption transactions locally instead of using the Ergo node API
    pub tracker_secret_key: Option<String>,
}

impl std::fmt::Debug for ErgoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErgoConfig")
            .field("node", &self.node)
            .field(
                "basis_reserve_contract_p2s",
                &self.basis_reserve_contract_p2s,
            )
            .field("tracker_nft_id", &self.tracker_nft_id)
            .field("tracker_public_key", &self.tracker_public_key)
            .field(
                "tracker_secret_key",
                &self.tracker_secret_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
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

impl AppConfig {
    fn default_builder() -> Result<DefaultConfigBuilder, config::ConfigError> {
        let legacy_read_only_p2s = basis_store::contract_compiler::get_basis_reserve_contract_p2s()
            .map_err(|e| config::ConfigError::Message(e.to_string()))?;

        config::Config::builder()
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3048)?
            .set_default("server.data_dir", "data")?
            .set_default("server.database_url", "sqlite:data/basis.db")?
            .set_default("ergo.node.node_url", "http://159.89.116.15:11088")?
            .set_default("ergo.node.scan_name", "Basis Reserve Scanner")?
            .set_default("ergo.node.api_key", "")?
            .set_default("ergo.basis_reserve_contract_p2s", legacy_read_only_p2s)?
            .set_default("transaction.fee", 1000000)?
            .set_default("ergo.tracker_public_key", "")?
            .set_default("ergo.tracker_secret_key", "")?
            .set_default("acceptance.default", "reject")?
            .set_default("acceptance.predicates", Vec::<String>::new())
    }

    fn environment() -> config::Environment {
        config::Environment::with_prefix("BASIS")
            .prefix_separator("_")
            .separator("__")
            .ignore_empty(true)
    }

    fn load_with_sources<F>(
        file: F,
        environment: config::Environment,
    ) -> Result<Self, config::ConfigError>
    where
        F: config::Source + Send + Sync + 'static,
    {
        Self::default_builder()?
            // Later sources have higher priority: explicit environment values
            // override the optional configuration file.
            .add_source(file)
            .add_source(environment)
            .build()?
            .try_deserialize()
    }

    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::from(path.as_ref()))
            .build()?;

        config.try_deserialize()
    }

    /// Load configuration from default locations
    pub fn load() -> Result<Self, config::ConfigError> {
        Self::load_with_sources(
            config::File::with_name("config/basis").required(false),
            Self::environment(),
        )
    }

    /// Load the process configuration without substituting a different
    /// contract generation when parsing or deserialization fails.
    pub fn load_for_startup() -> Result<Self, String> {
        Self::require_loaded(Self::load())
    }

    fn require_loaded(result: Result<Self, config::ConfigError>) -> Result<Self, String> {
        result.map_err(|error| format!("failed to load Basis configuration: {error}"))
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

    /// Require the exact Basis v2 ERG identity from the pinned source-to-byte receipt.
    pub fn validate_basis_v2_erg_contract(&self) -> Result<(), String> {
        basis_store::contract_compiler::validate_basis_v2_contract_p2s(
            self.basis_reserve_contract_p2s(),
            basis_store::contract_compiler::BasisV2ContractKind::Erg,
        )
        .map_err(|e| format!("Basis v2 contract identity check failed: {e}"))
    }

    /// V2-A carries exact contract identity and message primitives only. The
    /// current scanner and state store are still v1-shaped, so activating the
    /// exact v2 tree here would be unsafe. The historical identity is retained
    /// temporarily for compatibility; `/reserves/create`,
    /// `/config/reserve-contract-p2s`, and `/redemption/build` remain
    /// tombstones. This does not attest safety of legacy library builders or
    /// acceptance state.
    pub fn validate_runtime_contract_mode(&self) -> Result<(), String> {
        let configured = self.basis_reserve_contract_p2s();
        let legacy = basis_store::contract_compiler::get_basis_reserve_contract_p2s()
            .map_err(|e| format!("cannot resolve historical contract identity: {e}"))?;
        if configured == legacy {
            return Ok(());
        }
        if self.validate_basis_v2_erg_contract().is_ok() {
            return Err(
                "Basis v2 contract identity is recognized, but v2 scanner and BNS2/BRS2 state are not installed; runtime activation is disabled"
                    .to_string(),
            );
        }
        Err("configured reserve contract is neither the supported read-only legacy identity nor the exact embedded Basis v2 ERG identity".to_string())
    }

    /// Existing construction code still emits the retired v1 ABI. It must not
    /// be re-enabled merely because a non-legacy-looking P2S was configured.
    pub fn reject_unsupported_reserve_builder(&self) -> Result<(), String> {
        self.validate_basis_v2_erg_contract()?;
        Err(
            "the exact Basis v2 contract is configured, but reserve/redemption construction remains disabled until the v2 runtime builder is installed"
                .to_string(),
        )
    }

    /// Get the tracker NFT ID bytes (required - server will fail if not configured)
    pub fn tracker_nft_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        match &self.ergo.tracker_nft_id {
            Some(nft_id) if !nft_id.is_empty() => {
                let bytes = hex::decode(nft_id)?;
                if bytes.len() == 32 {
                    Ok(bytes)
                } else {
                    Err(hex::FromHexError::InvalidStringLength)
                }
            }
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
                            hex::encode(&pubkey_bytes)
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
                                hex::encode(&result)
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

    fn environment_from(entries: &[(&str, String)]) -> config::Environment {
        AppConfig::environment().source(Some(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        ))
    }

    fn write_config_file(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("basis.toml");
        std::fs::write(&path, contents).unwrap();
        (directory, path)
    }

    #[test]
    fn nested_environment_values_override_the_file_source() {
        let legacy = basis_store::contract_compiler::get_basis_reserve_contract_p2s().unwrap();
        let v2 = basis_store::contract_compiler::get_basis_v2_contract_p2s(
            basis_store::contract_compiler::BasisV2ContractKind::Erg,
        )
        .unwrap();
        let (_directory, path) = write_config_file(&format!(
            r#"
[server]
port = 3048
data_dir = "from-file"

[ergo]
basis_reserve_contract_p2s = "{legacy}"

[ergo.node]
node_url = "http://file-node:9053"

[transaction]
fee = 1000000
"#
        ));
        let environment = environment_from(&[
            ("BASIS_SERVER__PORT", "4050".to_string()),
            ("BASIS_SERVER__DATA_DIR", "from-environment".to_string()),
            ("BASIS_ERGO__BASIS_RESERVE_CONTRACT_P2S", v2.clone()),
            (
                "BASIS_ERGO__NODE__NODE_URL",
                "http://environment-node:9053".to_string(),
            ),
        ]);

        let config = AppConfig::load_with_sources(config::File::from(path), environment).unwrap();

        assert_eq!(config.server.port, 4050);
        assert_eq!(config.server.data_dir.as_deref(), Some("from-environment"));
        assert_eq!(config.ergo.node.node_url, "http://environment-node:9053");
        assert_eq!(config.basis_reserve_contract_p2s(), v2);
        assert!(config
            .validate_runtime_contract_mode()
            .unwrap_err()
            .contains("runtime activation is disabled"));
    }

    #[test]
    fn environment_contract_identity_is_validated_without_legacy_fallback() {
        let legacy = basis_store::contract_compiler::get_basis_reserve_contract_p2s().unwrap();
        let v2 = basis_store::contract_compiler::get_basis_v2_contract_p2s(
            basis_store::contract_compiler::BasisV2ContractKind::Erg,
        )
        .unwrap();
        let (_directory, path) = write_config_file("");

        let legacy_config = AppConfig::load_with_sources(
            config::File::from(path.clone()),
            environment_from(&[("BASIS_ERGO__BASIS_RESERVE_CONTRACT_P2S", legacy.clone())]),
        )
        .unwrap();
        legacy_config.validate_runtime_contract_mode().unwrap();

        let v2_config = AppConfig::load_with_sources(
            config::File::from(path.clone()),
            environment_from(&[("BASIS_ERGO__BASIS_RESERVE_CONTRACT_P2S", v2)]),
        )
        .unwrap();
        assert!(v2_config
            .validate_runtime_contract_mode()
            .unwrap_err()
            .contains("runtime activation is disabled"));

        let unknown_config = AppConfig::load_with_sources(
            config::File::from(path),
            environment_from(&[(
                "BASIS_ERGO__BASIS_RESERVE_CONTRACT_P2S",
                "unknown-contract".to_string(),
            )]),
        )
        .unwrap();
        assert!(unknown_config
            .validate_runtime_contract_mode()
            .unwrap_err()
            .contains("neither the supported read-only legacy identity"));
    }

    #[test]
    fn app_config_debug_redacts_node_and_tracker_secrets() {
        let node_sentinel = "sentinel-node-api-key-do-not-log";
        let tracker_sentinel = "11".repeat(32);
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                data_dir: None,
                database_url: None,
            },
            ergo: ErgoConfig {
                node: NodeConfig {
                    api_key: Some(node_sentinel.to_string()),
                    ..NodeConfig::default()
                },
                basis_reserve_contract_p2s: "configured-explicitly".to_string(),
                tracker_nft_id: None,
                tracker_public_key: None,
                tracker_secret_key: Some(tracker_sentinel.clone()),
                allow_fresh_tracker_generation: false,
            },
            transaction: TransactionConfig {
                fee: 1_000_000,
                change_address: None,
            },
            acceptance: AcceptanceConfig::empty(),
        };

        let rendered = format!("{config:?}");
        assert!(!rendered.contains(node_sentinel));
        assert!(!rendered.contains(&tracker_sentinel));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn validates_only_the_exact_basis_v2_contract() {
        let mut config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                data_dir: None,
                database_url: None,
            },
            ergo: ErgoConfig {
                node: NodeConfig::default(),
                basis_reserve_contract_p2s:
                    basis_store::contract_compiler::get_basis_reserve_contract_p2s().unwrap(),
                tracker_nft_id: None,
                tracker_public_key: None,
                tracker_secret_key: None,
                allow_fresh_tracker_generation: false,
            },
            transaction: TransactionConfig {
                fee: 1_000_000,
                change_address: None,
            },
            acceptance: AcceptanceConfig::empty(),
        };

        let error = config.validate_basis_v2_erg_contract().unwrap_err();
        assert!(error.contains("identity check failed"));
        config.validate_runtime_contract_mode().unwrap();

        config.ergo.basis_reserve_contract_p2s =
            basis_store::contract_compiler::get_basis_v2_contract_p2s(
                basis_store::contract_compiler::BasisV2ContractKind::Erg,
            )
            .unwrap();
        config.validate_basis_v2_erg_contract().unwrap();
        assert!(config.reject_unsupported_reserve_builder().is_err());
        assert!(config
            .validate_runtime_contract_mode()
            .unwrap_err()
            .contains("runtime activation is disabled"));

        config.ergo.basis_reserve_contract_p2s = "unrecognized".to_string();
        assert!(config
            .validate_runtime_contract_mode()
            .unwrap_err()
            .contains("neither the supported read-only legacy identity"));
    }

    #[test]
    fn startup_configuration_errors_never_fall_back_to_legacy() {
        let error = AppConfig::require_loaded(Err(config::ConfigError::Message(
            "sentinel malformed configuration".to_string(),
        )))
        .unwrap_err();
        assert!(error.contains("sentinel malformed configuration"));
        assert!(!error.contains("default configuration"));
    }

    #[test]
    fn test_tracker_public_key_hex_format() {
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                data_dir: Some("test_data".to_string()),
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
                allow_fresh_tracker_generation: false,
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
}
