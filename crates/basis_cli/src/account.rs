use crate::config::{ConfigManager, AccountConfig};
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

    pub fn from_config(config: &AccountConfig, keypair: KeyPair) -> Self {
        Self {
            name: config.name.clone(),
            keypair,
            created_at: config.created_at,
        }
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
    config_manager: ConfigManager,
    accounts: HashMap<String, Account>,
}

impl AccountManager {
    pub fn new(config_manager: ConfigManager) -> Result<Self> {
        let accounts = HashMap::new();
        
        // We don't load accounts from config to avoid key management complexity
        // Accounts are created fresh each session for testing purposes
        
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
        
        // Save to config (just for reference, not for loading private keys)
        self.config_manager.add_account(name, &pubkey_hex)?;
        
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

    pub fn list_accounts(&self) -> Vec<&Account> {
        if self.accounts.is_empty() {
            println!("No accounts in memory. Use 'basis-cli account create <name>' to create one.");
        }
        self.accounts.values().collect()
    }

    pub fn get_current(&self) -> Option<&Account> {
        // For now, just return the first account if any exist
        // In production, we'd properly track current account
        self.accounts.values().next()
    }

    pub fn get_account(&self, name: &str) -> Option<&Account> {
        self.accounts.get(name)
    }

    pub fn get_current_pubkey(&self) -> Option<PubKey> {
        self.get_current().map(|account| account.keypair.get_public_key_bytes())
    }

    pub fn get_current_pubkey_hex(&self) -> Option<String> {
        self.get_current().map(|account| account.get_pubkey_hex())
    }

    pub fn sign_with_current(&self, message: &[u8]) -> Result<[u8; 65]> {
        let current = self.get_current()
            .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;
        
        current.sign_message(message)
    }
}