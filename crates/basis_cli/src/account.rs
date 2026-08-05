use crate::config::{AccountConfig, ConfigManager};
use crate::crypto::{KeyPair, PubKey};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Account {
    pub name: String,
    pub keypair: KeyPair,
    pub created_at: u64,
}

impl Account {
    pub fn new(name: String) -> Result<Self> {
        let keypair = KeyPair::new()?;
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        Ok(Self {
            name,
            keypair,
            created_at,
        })
    }

    #[allow(dead_code)]
    pub fn from_config(config: &AccountConfig, keypair: KeyPair) -> Self {
        Self {
            name: config.name.clone(),
            keypair,
            created_at: config.created_at,
        }
    }

    pub fn from_private_key_hex(name: &str, private_key_hex: &str) -> Result<Self> {
        let private_key_bytes = hex::decode(private_key_hex)
            .map_err(|e| anyhow::anyhow!("Invalid hex encoding: {}", e))?;

        let private_key_array: [u8; 32] = private_key_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Private key must be 32 bytes"))?;

        let keypair = KeyPair::from_private_key_bytes(&private_key_array)?;
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        Ok(Self {
            name: name.to_string(),
            keypair,
            created_at,
        })
    }

    pub fn get_pubkey_hex(&self) -> String {
        hex::encode(self.keypair.get_public_key_bytes())
    }

    pub fn get_private_key_hex(&self) -> String {
        hex::encode(self.keypair.get_private_key_bytes())
    }

    pub fn sign_message(&self, message: &[u8]) -> Result<[u8; 65]> {
        self.keypair.sign_message(message)
    }
}

#[derive(Debug)]
pub struct AccountManager {
    pub config_manager: ConfigManager,
    pub accounts: HashMap<String, Account>,
}

impl AccountManager {
    pub fn new(config_manager: ConfigManager) -> Result<Self> {
        let mut accounts = HashMap::new();

        // Load accounts from config with persistent private keys
        for account_config in config_manager.list_accounts() {
            // Load account with persistent private key from config
            let account = Account::from_private_key_hex(
                &account_config.name,
                &account_config.private_key_hex,
            )?;
            accounts.insert(account_config.name.clone(), account);
        }

        Ok(Self {
            config_manager,
            accounts,
        })
    }

    pub fn create_account(&mut self, name: &str) -> Result<Account> {
        if self.accounts.contains_key(name) {
            return Err(anyhow::anyhow!("Account '{}' already exists", name));
        }

        let account = Account::new(name.to_string())?;
        let pubkey_hex = account.get_pubkey_hex();
        let private_key_hex = account.get_private_key_hex();

        // Save to config with private key for persistence
        self.config_manager
            .add_account(name, &pubkey_hex, &private_key_hex)?;

        self.accounts.insert(name.to_string(), account.clone());

        // Set as current if no current account
        if self.config_manager.get_config().current_account.is_none() {
            self.config_manager.set_current_account(name)?;
        }

        Ok(account)
    }

    pub fn switch_account(&mut self, name: &str) -> Result<()> {
        if !self.accounts.contains_key(name) {
            return Err(anyhow::anyhow!("Account '{}' not found", name));
        }

        self.config_manager.set_current_account(name)
    }

    /// List in-memory accounts. Used by the TUI (basis_cli_lib); the CLI binary renders
    /// account listings through `commands::account::list_account_entries` instead.
    #[allow(dead_code)]
    pub fn list_accounts(&self) -> Vec<&Account> {
        if self.accounts.is_empty() {
            println!("No accounts in memory. Use 'basis-cli account create <name>' to create one.");
        }
        self.accounts.values().collect()
    }

    pub fn get_current(&self) -> Option<&Account> {
        // Get current account name from config
        let current_account_name = self.config_manager.get_config().current_account.as_ref()?;

        // Return the account with that name
        self.accounts.get(current_account_name)
    }

    pub fn get_account(&self, name: &str) -> Option<&Account> {
        self.accounts.get(name)
    }

    pub fn delete_account(&mut self, name: &str) -> Result<()> {
        if !self.accounts.contains_key(name) {
            return Err(anyhow::anyhow!("Account '{}' not found", name));
        }

        self.accounts.remove(name);
        self.config_manager.delete_account(name)
    }

    #[allow(dead_code)]
    pub fn get_current_pubkey(&self) -> Option<PubKey> {
        self.get_current()
            .map(|account| account.keypair.get_public_key_bytes())
    }

    pub fn get_current_pubkey_hex(&self) -> Option<String> {
        self.get_current().map(|account| account.get_pubkey_hex())
    }

    #[allow(dead_code)]
    pub fn sign_with_current(&self, message: &[u8]) -> Result<[u8; 65]> {
        let current = self
            .get_current()
            .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

        current.sign_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CliConfig;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_config_path() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "basis_account_manager_test_{}_{}.toml",
            std::process::id(),
            id
        ));
        let _ = fs::remove_file(&path);
        path
    }

    fn make_manager(path: PathBuf) -> AccountManager {
        let config = CliConfig {
            current_account: Some("alice".to_string()),
            accounts: {
                let mut map = HashMap::new();
                map.insert(
                    "alice".to_string(),
                    AccountConfig {
                        name: "alice".to_string(),
                        pubkey_hex: "02alice".to_string(),
                        private_key_hex:
                            "0000000000000000000000000000000000000000000000000000000000000001"
                                .to_string(),
                        created_at: 1,
                    },
                );
                map.insert(
                    "bob".to_string(),
                    AccountConfig {
                        name: "bob".to_string(),
                        pubkey_hex: "02bob".to_string(),
                        private_key_hex:
                            "0000000000000000000000000000000000000000000000000000000000000002"
                                .to_string(),
                        created_at: 2,
                    },
                );
                map
            },
            server_url: "http://127.0.0.1:3048".to_string(),
        };
        fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
        let config_manager = ConfigManager::new(Some(path)).unwrap();
        AccountManager::new(config_manager).unwrap()
    }

    #[test]
    fn test_delete_account_removes_from_memory_and_config() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        assert!(manager.get_account("bob").is_some());
        manager.delete_account("bob").unwrap();
        assert!(manager.get_account("bob").is_none());
        assert!(manager.config_manager.get_account("bob").is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_delete_current_account_clears_current_in_config() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        manager.delete_account("alice").unwrap();
        assert!(manager.get_account("alice").is_none());
        assert!(manager
            .config_manager
            .get_config()
            .current_account
            .is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_delete_nonexistent_account_fails() {
        let path = temp_config_path();
        let mut manager = make_manager(path.clone());

        let result = manager.delete_account("charlie");
        assert!(result.is_err());

        let _ = fs::remove_file(&path);
    }
}
