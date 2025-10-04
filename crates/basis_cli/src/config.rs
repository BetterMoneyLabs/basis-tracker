use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub current_account: Option<String>,
    pub accounts: HashMap<String, AccountConfig>,
    pub server_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub pubkey_hex: String,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
    config: CliConfig,
}

impl ConfigManager {
    pub fn new(custom_path: Option<PathBuf>) -> Result<Self> {
        let config_path = match custom_path {
            Some(path) => path,
            None => {
                let mut path = dirs::home_dir()
                    .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
                path.push(".basis");
                fs::create_dir_all(&path)?;
                path.push("cli.toml");
                path
            }
        };

        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else {
            CliConfig {
                current_account: None,
                accounts: HashMap::new(),
                server_url: "http://127.0.0.1:3000".to_string(),
            }
        };

        Ok(Self {
            config_path,
            config,
        })
    }

    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn get_config(&self) -> &CliConfig {
        &self.config
    }

    pub fn get_config_mut(&mut self) -> &mut CliConfig {
        &mut self.config
    }

    pub fn set_current_account(&mut self, name: &str) -> Result<()> {
        self.config.current_account = Some(name.to_string());
        self.save()
    }

    pub fn add_account(&mut self, name: &str, pubkey_hex: &str) -> Result<()> {
        let account_config = AccountConfig {
            name: name.to_string(),
            pubkey_hex: pubkey_hex.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        };
        
        self.config.accounts.insert(name.to_string(), account_config);
        self.save()
    }

    pub fn get_account(&self, name: &str) -> Option<&AccountConfig> {
        self.config.accounts.get(name)
    }

    pub fn list_accounts(&self) -> Vec<&AccountConfig> {
        self.config.accounts.values().collect()
    }

    pub fn get_current_account(&self) -> Option<&AccountConfig> {
        self.config
            .current_account
            .as_ref()
            .and_then(|name| self.config.accounts.get(name))
    }
}