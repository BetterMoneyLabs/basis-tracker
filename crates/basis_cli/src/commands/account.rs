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
            let accounts = account_manager.list_accounts();
            let current_account = account_manager.get_current();
            
            if accounts.is_empty() {
                println!("No accounts found. Use 'basis-cli account create <name>' to create one.");
            } else {
                println!("Accounts:");
                for account in accounts {
                    let is_current = current_account
                        .map(|current| current.name == account.name)
                        .unwrap_or(false);
                    
                    let current_indicator = if is_current { " (current)" } else { "" };
                    println!("  {}: {}{}", account.name, account.get_pubkey_hex(), current_indicator);
                }
            }
        }
        AccountCommands::Switch { name } => {
            account_manager.switch_account(&name)?;
            println!("✅ Switched to account '{}'", name);
        }
        AccountCommands::Info => {
            if let Some(account) = account_manager.get_current() {
                println!("Current Account: {}", account.name);
                println!("Public Key: {}", account.get_pubkey_hex());
                println!("Created at: {}", account.created_at);
            } else {
                println!("No current account selected. Use 'basis-cli account switch <name>' to select one.");
            }
        }
    }
    
    Ok(())
}