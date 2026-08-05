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
    pub private_key_hex: String,
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
                server_url: "http://127.0.0.1:3048".to_string(),
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

    #[allow(dead_code)]
    pub fn get_config_mut(&mut self) -> &mut CliConfig {
        &mut self.config
    }

    pub fn set_current_account(&mut self, name: &str) -> Result<()> {
        self.config.current_account = Some(name.to_string());
        self.save()
    }

    pub fn add_account(
        &mut self,
        name: &str,
        pubkey_hex: &str,
        private_key_hex: &str,
    ) -> Result<()> {
        let account_config = AccountConfig {
            name: name.to_string(),
            pubkey_hex: pubkey_hex.to_string(),
            private_key_hex: private_key_hex.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        };

        self.config
            .accounts
            .insert(name.to_string(), account_config);
        self.save()
    }

    #[allow(dead_code)]
    pub fn get_account(&self, name: &str) -> Option<&AccountConfig> {
        self.config.accounts.get(name)
    }

    pub fn delete_account(&mut self, name: &str) -> Result<()> {
        self.config.accounts.remove(name);
        if self.config.current_account.as_deref() == Some(name) {
            self.config.current_account = None;
        }
        self.save()
    }

    pub fn list_accounts(&self) -> Vec<&AccountConfig> {
        self.config.accounts.values().collect()
    }

    #[allow(dead_code)]
    pub fn get_current_account(&self) -> Option<&AccountConfig> {
        self.config
            .current_account
            .as_ref()
            .and_then(|name| self.config.accounts.get(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_config_path() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "basis_cli_config_test_{}_{}.toml",
            std::process::id(),
            id
        ));
        // ensure a fresh start
        let _ = fs::remove_file(&path);
        path
    }

    fn make_manager(path: PathBuf) -> ConfigManager {
        let config = CliConfig {
            current_account: Some("alice".to_string()),
            accounts: {
                let mut map = HashMap::new();
                map.insert(
                    "alice".to_string(),
                    AccountConfig {
                        name: "alice".to_string(),
                        pubkey_hex: "02alice".to_string(),
                        private_key_hex: "00alice".to_string(),
                        created_at: 1,
                    },
                );
                map.insert(
                    "bob".to_string(),
                    AccountConfig {
                        name: "bob".to_string(),
                        pubkey_hex: "02bob".to_string(),
                        private_key_hex: "00bob".to_string(),
                        created_at: 2,
                    },
                );
                map
            },
            server_url: "http://127.0.0.1:3048".to_string(),
        };
        fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
        ConfigManager::new(Some(path)).unwrap()
    }

    #[test]
    fn test_delete_existing_account_keeps_current() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        manager.delete_account("bob").unwrap();

        assert!(manager.get_account("bob").is_none());
        assert!(manager.get_account("alice").is_some());
        assert_eq!(
            manager.get_config().current_account.as_deref(),
            Some("alice")
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_delete_current_account_clears_current() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        manager.delete_account("alice").unwrap();

        assert!(manager.get_account("alice").is_none());
        assert!(manager.get_config().current_account.is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_delete_nonexistent_account_is_noop() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        manager.delete_account("charlie").unwrap();

        assert_eq!(manager.list_accounts().len(), 2);

        let _ = fs::remove_file(&path);
    }
}
