//! Acceptance-policy configuration stored at `~/.basis/ui.toml`.
//!
//! Structurally mirrors `TuiConfig`/`TuiConfigManager` in `basis_app` (crates/basis_app/src/app.rs)
//! so the same file parses for both the TUI and this MCP server. Keep the two in sync.

use anyhow::Result;
use basis_core::acceptance::AcceptanceConfig;

/// ui.toml layout: server URL, TUI current account, and the acceptance policy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiConfig {
    pub server_url: String,
    pub current_account: Option<String>,
    #[serde(default = "AcceptanceConfig::default_collateral")]
    pub acceptance: AcceptanceConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:3048".to_string(),
            current_account: None,
            acceptance: AcceptanceConfig::default_collateral(),
        }
    }
}

/// Manages the ui.toml configuration file at ~/.basis/ui.toml
#[derive(Debug, Clone)]
pub struct UiConfigManager {
    config_path: std::path::PathBuf,
    config: UiConfig,
}

impl UiConfigManager {
    pub fn new() -> Result<Self> {
        let mut config_path =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        config_path.push(".basis");
        std::fs::create_dir_all(&config_path)?;
        config_path.push("ui.toml");

        let config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            UiConfig::default()
        };

        Ok(Self {
            config_path,
            config,
        })
    }

    pub fn get_acceptance(&self) -> &AcceptanceConfig {
        &self.config.acceptance
    }

    pub fn update_acceptance(&mut self, config: AcceptanceConfig) -> Result<()> {
        self.config.acceptance = config;
        self.save()
    }

    fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }
}
