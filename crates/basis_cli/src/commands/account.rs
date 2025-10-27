use crate::account::Account;
use crate::account::AccountManager;
use anyhow::Result;
use clap::Subcommand;

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
}

pub async fn handle_account_command(
    cmd: AccountCommands,
    account_manager: &mut AccountManager,
) -> Result<()> {
    match cmd {
        AccountCommands::Create { name } => {
            let account = account_manager.create_account(&name)?;
            println!("✅ Created account '{}'", name);
            println!("  Public Key: {}", account.get_pubkey_hex());
            println!("  Created at: {}", account.created_at);
        }
        AccountCommands::List => {
            let in_memory_accounts = account_manager.list_accounts();
            let config_accounts = account_manager.config_manager.list_accounts();
            let current_account = account_manager.get_current();

            if in_memory_accounts.is_empty() && config_accounts.is_empty() {
                println!("No accounts found. Use 'basis-cli account create <name>' to create one.");
            } else {
                if !config_accounts.is_empty() {
                    println!("Persisted accounts (from config):");
                    for account_config in config_accounts {
                        let is_current = current_account
                            .map(|current| current.name == account_config.name)
                            .unwrap_or(false);

                        let current_indicator = if is_current { " ⭐ (current)" } else { "" };
                        println!(
                            "  {}: {}{}",
                            account_config.name, account_config.pubkey_hex, current_indicator
                        );
                    }
                }

                if !in_memory_accounts.is_empty() {
                    println!("\nIn-memory accounts (current session):");
                    for account in in_memory_accounts {
                        let is_current = current_account
                            .map(|current| current.name == account.name)
                            .unwrap_or(false);

                        let current_indicator = if is_current { " ⭐ (current)" } else { "" };
                        println!(
                            "  {}: {}{}",
                            account.name,
                            account.get_pubkey_hex(),
                            current_indicator
                        );
                    }
                }
            }
        }
        AccountCommands::Switch { name } => {
            account_manager.switch_account(&name)?;
            println!("✅ Switched to account '{}'", name);
        }
        AccountCommands::Info => {
            if let Some(account) = account_manager.get_current() {
                println!("⭐ Current Account: {}", account.name);
                println!("  Public Key: {}", account.get_pubkey_hex());
                println!("  Created at: {}", account.created_at);
            } else {
                println!("No current account selected.");
                println!("Use 'basis-cli account create <name>' to create an account.");
                println!("Use 'basis-cli account switch <name>' to select an existing account.");
            }
        }
        AccountCommands::Export { name } => {
            if let Some(account) = account_manager.get_account(&name) {
                let private_key_hex = account.get_private_key_hex();
                println!("Private key for account '{}':", name);
                println!("{}", private_key_hex);
                println!(
                    "\n⚠️  WARNING: Keep this private key secure! Do not share it with anyone."
                );
            } else {
                println!("Account '{}' not found in current session.", name);
            }
        }
        AccountCommands::Import { name, private_key } => {
            if account_manager.get_account(&name).is_some() {
                return Err(anyhow::anyhow!("Account '{}' already exists", name));
            }

            let account = Account::from_private_key_hex(&name, &private_key)?;
            let pubkey_hex = account.get_pubkey_hex();

            // Save to config
            account_manager
                .config_manager
                .add_account(&name, &pubkey_hex, &private_key)?;

            // Add to in-memory accounts
            account_manager.accounts.insert(name.clone(), account);

            println!("✅ Successfully imported account '{}'", name);
            println!("Public Key: {}", pubkey_hex);
        }
    }

    Ok(())
}
