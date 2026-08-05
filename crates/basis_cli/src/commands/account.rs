use crate::account::Account;
use crate::account::AccountManager;
use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;

#[derive(Subcommand)]
pub enum AccountCommands {
    /// Create a new account
    Create {
        /// Account name
        name: String,
    },
    /// List all accounts
    List,
    /// Switch to a different account
    Switch {
        /// Account name
        name: String,
    },
    /// Show current account info
    Info,
    /// Export account private key (hex format)
    Export {
        /// Account name
        name: String,
    },
    /// Import account from private key
    Import {
        /// Account name
        name: String,
        /// Private key in hex format
        private_key: String,
    },
    /// Delete an account
    Delete {
        /// Account name
        name: String,
    },
}

/// Result of creating a new account.
#[derive(Debug, Serialize)]
pub struct AccountCreatedResult {
    pub name: String,
    pub pubkey_hex: String,
    pub created_at: u64,
}

/// A single account entry as printed by `account list --json`.
#[derive(Debug, Serialize)]
pub struct AccountListEntry {
    pub name: String,
    pub pubkey_hex: String,
    pub current: bool,
    /// "config" for persisted accounts, "memory" for in-session accounts.
    pub source: String,
}

/// Result of switching the current account.
#[derive(Debug, Serialize)]
pub struct AccountSwitchedResult {
    pub switched: String,
}

/// Current account information (`account info`).
#[derive(Debug, Serialize)]
pub struct AccountInfoResult {
    pub name: String,
    pub pubkey_hex: String,
    pub created_at: u64,
}

/// Exported private key for an account (`account export`).
#[derive(Debug, Serialize)]
pub struct AccountExportedResult {
    pub name: String,
    pub private_key: String,
}

/// Result of importing an account from a private key.
#[derive(Debug, Serialize)]
pub struct AccountImportedResult {
    pub name: String,
    pub pubkey_hex: String,
}

/// Result of deleting an account.
#[derive(Debug, Serialize)]
pub struct AccountDeletedResult {
    pub deleted: String,
}

/// Create a new account and persist it to the config.
pub fn create_account(
    account_manager: &mut AccountManager,
    name: &str,
) -> Result<AccountCreatedResult> {
    let account = account_manager.create_account(name)?;
    Ok(AccountCreatedResult {
        name: name.to_string(),
        pubkey_hex: account.get_pubkey_hex(),
        created_at: account.created_at,
    })
}

/// Collect all accounts (persisted first, then in-memory) as typed entries.
pub fn list_account_entries(account_manager: &AccountManager) -> Vec<AccountListEntry> {
    let current_name = account_manager
        .get_current()
        .map(|current| current.name.clone());

    let mut entries = Vec::new();
    for account_config in account_manager.config_manager.list_accounts() {
        entries.push(AccountListEntry {
            current: current_name.as_deref() == Some(account_config.name.as_str()),
            name: account_config.name.clone(),
            pubkey_hex: account_config.pubkey_hex.clone(),
            source: "config".to_string(),
        });
    }
    for account in account_manager.accounts.values() {
        entries.push(AccountListEntry {
            current: current_name.as_deref() == Some(account.name.as_str()),
            name: account.name.clone(),
            pubkey_hex: account.get_pubkey_hex(),
            source: "memory".to_string(),
        });
    }
    entries
}

/// Switch the current account.
pub fn switch_account(
    account_manager: &mut AccountManager,
    name: &str,
) -> Result<AccountSwitchedResult> {
    account_manager.switch_account(name)?;
    Ok(AccountSwitchedResult {
        switched: name.to_string(),
    })
}

/// Get information about the current account, if any is selected.
pub fn current_account_info(account_manager: &AccountManager) -> Option<AccountInfoResult> {
    account_manager
        .get_current()
        .map(|account| AccountInfoResult {
            name: account.name.clone(),
            pubkey_hex: account.get_pubkey_hex(),
            created_at: account.created_at,
        })
}

/// Export the private key of an in-session account, if present.
pub fn export_account(
    account_manager: &AccountManager,
    name: &str,
) -> Option<AccountExportedResult> {
    account_manager
        .get_account(name)
        .map(|account| AccountExportedResult {
            name: name.to_string(),
            private_key: account.get_private_key_hex(),
        })
}

/// Import an account from a hex-encoded private key.
pub fn import_account(
    account_manager: &mut AccountManager,
    name: &str,
    private_key: &str,
) -> Result<AccountImportedResult> {
    if account_manager.get_account(name).is_some() {
        return Err(anyhow::anyhow!("Account '{}' already exists", name));
    }

    let account = Account::from_private_key_hex(name, private_key)?;
    let pubkey_hex = account.get_pubkey_hex();

    // Save to config
    account_manager
        .config_manager
        .add_account(name, &pubkey_hex, private_key)?;

    // Add to in-memory accounts
    account_manager.accounts.insert(name.to_string(), account);

    Ok(AccountImportedResult {
        name: name.to_string(),
        pubkey_hex,
    })
}

/// Delete an account from config and the current session.
pub fn delete_account(
    account_manager: &mut AccountManager,
    name: &str,
) -> Result<AccountDeletedResult> {
    account_manager.delete_account(name)?;
    Ok(AccountDeletedResult {
        deleted: name.to_string(),
    })
}

pub async fn handle_account_command(
    cmd: AccountCommands,
    account_manager: &mut AccountManager,
    json: bool,
) -> Result<()> {
    match cmd {
        AccountCommands::Create { name } => {
            let result = create_account(account_manager, &name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("✅ Created account '{}'", result.name);
                println!("  Public Key: {}", result.pubkey_hex);
                println!("  Created at: {}", result.created_at);
            }
        }
        AccountCommands::List => {
            let entries = list_account_entries(account_manager);
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                let in_memory: Vec<&AccountListEntry> =
                    entries.iter().filter(|e| e.source == "memory").collect();
                let persisted: Vec<&AccountListEntry> =
                    entries.iter().filter(|e| e.source == "config").collect();

                if in_memory.is_empty() {
                    println!(
                        "No accounts in memory. Use 'basis-cli account create <name>' to create one."
                    );
                }

                if entries.is_empty() {
                    println!(
                        "No accounts found. Use 'basis-cli account create <name>' to create one."
                    );
                } else {
                    if !persisted.is_empty() {
                        println!("Persisted accounts (from config):");
                        for entry in persisted {
                            let current_indicator =
                                if entry.current { " ⭐ (current)" } else { "" };
                            println!(
                                "  {}: {}{}",
                                entry.name, entry.pubkey_hex, current_indicator
                            );
                        }
                    }

                    if !in_memory.is_empty() {
                        println!("\nIn-memory accounts (current session):");
                        for entry in in_memory {
                            let current_indicator =
                                if entry.current { " ⭐ (current)" } else { "" };
                            println!(
                                "  {}: {}{}",
                                entry.name, entry.pubkey_hex, current_indicator
                            );
                        }
                    }
                }
            }
        }
        AccountCommands::Switch { name } => {
            let result = switch_account(account_manager, &name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("✅ Switched to account '{}'", result.switched);
            }
        }
        AccountCommands::Info => {
            let result = current_account_info(account_manager);
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(account) = result {
                println!("⭐ Current Account: {}", account.name);
                println!("  Public Key: {}", account.pubkey_hex);
                println!("  Created at: {}", account.created_at);
            } else {
                println!("No current account selected.");
                println!("Use 'basis-cli account create <name>' to create an account.");
                println!("Use 'basis-cli account switch <name>' to select an existing account.");
            }
        }
        AccountCommands::Export { name } => {
            let result = export_account(account_manager, &name);
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(exported) = result {
                println!("Private key for account '{}':", exported.name);
                println!("{}", exported.private_key);
                println!(
                    "\n⚠️  WARNING: Keep this private key secure! Do not share it with anyone."
                );
            } else {
                println!("Account '{}' not found in current session.", name);
            }
        }
        AccountCommands::Import { name, private_key } => {
            let result = import_account(account_manager, &name, &private_key)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("✅ Successfully imported account '{}'", result.name);
                println!("Public Key: {}", result.pubkey_hex);
            }
        }
        AccountCommands::Delete { name } => {
            let result = delete_account(account_manager, &name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("✅ Deleted account '{}'", result.deleted);
            }
        }
    }

    Ok(())
}
