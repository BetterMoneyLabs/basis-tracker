use crate::account::AccountManager;
use crate::api::TrackerClient;
use crate::commands::{account, note, reserve, status};
use anyhow::Result;
use std::io::{self, Write};

pub struct InteractiveMode {
    account_manager: AccountManager,
    client: TrackerClient,
}

impl InteractiveMode {
    pub fn new(account_manager: AccountManager, client: TrackerClient) -> Self {
        Self {
            account_manager,
            client,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("🚀 Basis Tracker CLI - Interactive Mode");
        println!("Type 'help' for available commands, 'exit' to quit\n");

        loop {
            let current_account = self.account_manager.get_current()
                .map(|acc| acc.name.clone())
                .unwrap_or_else(|| "none".to_string());

            print!("basis-cli [{}] > ", current_account);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            match input {
                "exit" | "quit" | "q" => {
                    println!("Goodbye!");
                    break;
                }
                "help" | "h" => {
                    self.show_help();
                }
                "status" | "s" => {
                    status::handle_status_command(&self.client).await?;
                }
                _ => {
                    self.handle_command(input).await?;
                }
            }
        }

        Ok(())
    }

    fn show_help(&self) {
        println!("\nAvailable Commands:");
        println!("  account create <name>    - Create a new account");
        println!("  account list             - List all accounts");
        println!("  account switch <name>    - Switch to an account");
        println!("  account info             - Show current account info");
        println!("  note create --recipient <pubkey> --amount <amount>");
        println!("  note list --issuer       - List notes where you are issuer");
        println!("  note list --recipient    - List notes where you are recipient");
        println!("  note get --issuer <pubkey> --recipient <pubkey>");
        println!("  note redeem --issuer <pubkey> --amount <amount>");
        println!("  reserve status [--issuer <pubkey>]");
        println!("  reserve collateralization [--issuer <pubkey>]");
        println!("  status                   - Show server status and recent events");
        println!("  help                     - Show this help");
        println!("  exit                     - Exit interactive mode");
        println!();
    }

    async fn handle_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "account" => {
                if parts.len() >= 2 {
                    match parts[1] {
                        "create" if parts.len() >= 3 => {
                            let name = parts[2];
                            let cmd = account::AccountCommands::Create { name: name.to_string() };
                            account::handle_account_command(cmd, &mut self.account_manager).await?;
                        }
                        "list" => {
                            let cmd = account::AccountCommands::List;
                            account::handle_account_command(cmd, &mut self.account_manager).await?;
                        }
                        "switch" if parts.len() >= 3 => {
                            let name = parts[2];
                            let cmd = account::AccountCommands::Switch { name: name.to_string() };
                            account::handle_account_command(cmd, &mut self.account_manager).await?;
                        }
                        "info" => {
                            let cmd = account::AccountCommands::Info;
                            account::handle_account_command(cmd, &mut self.account_manager).await?;
                        }
                        _ => {
                            println!("Unknown account command. Use 'help' for available commands.");
                        }
                    }
                } else {
                    println!("Account command requires subcommand. Use 'help' for available commands.");
                }
            }
            "note" => {
                if parts.len() >= 2 {
                    match parts[1] {
                        "create" => {
                            // Parse --recipient and --amount flags
                            let mut recipient = None;
                            let mut amount = None;
                            
                            let mut i = 2;
                            while i < parts.len() {
                                match parts[i] {
                                    "--recipient" if i + 1 < parts.len() => {
                                        recipient = Some(parts[i + 1]);
                                        i += 2;
                                    }
                                    "--amount" if i + 1 < parts.len() => {
                                        amount = Some(parts[i + 1].parse()?);
                                        i += 2;
                                    }
                                    _ => {
                                        i += 1;
                                    }
                                }
                            }
                            
                            if let (Some(recipient), Some(amount)) = (recipient, amount) {
                                let cmd = note::NoteCommands::Create {
                                    recipient: recipient.to_string(),
                                    amount,
                                };
                                note::handle_note_command(cmd, &self.account_manager, &self.client).await?;
                            } else {
                                println!("Note create requires --recipient <pubkey> and --amount <amount>");
                            }
                        }
                        "list" => {
                            let mut issuer = false;
                            let mut recipient = false;
                            
                            for part in &parts[2..] {
                                match *part {
                                    "--issuer" => issuer = true,
                                    "--recipient" => recipient = true,
                                    _ => {}
                                }
                            }
                            
                            let cmd = note::NoteCommands::List { issuer, recipient };
                            note::handle_note_command(cmd, &self.account_manager, &self.client).await?;
                        }
                        _ => {
                            println!("Unknown note command. Use 'help' for available commands.");
                        }
                    }
                } else {
                    println!("Note command requires subcommand. Use 'help' for available commands.");
                }
            }
            "reserve" => {
                if parts.len() >= 2 {
                    match parts[1] {
                        "status" => {
                            let mut issuer = None;
                            
                            let mut i = 2;
                            while i < parts.len() {
                                match parts[i] {
                                    "--issuer" if i + 1 < parts.len() => {
                                        issuer = Some(parts[i + 1].to_string());
                                        i += 2;
                                    }
                                    _ => {
                                        i += 1;
                                    }
                                }
                            }
                            
                            let cmd = reserve::ReserveCommands::Status { issuer };
                            reserve::handle_reserve_command(cmd, &self.account_manager, &self.client).await?;
                        }
                        "collateralization" => {
                            let mut issuer = None;
                            
                            let mut i = 2;
                            while i < parts.len() {
                                match parts[i] {
                                    "--issuer" if i + 1 < parts.len() => {
                                        issuer = Some(parts[i + 1].to_string());
                                        i += 2;
                                    }
                                    _ => {
                                        i += 1;
                                    }
                                }
                            }
                            
                            let cmd = reserve::ReserveCommands::Collateralization { issuer };
                            reserve::handle_reserve_command(cmd, &self.account_manager, &self.client).await?;
                        }
                        _ => {
                            println!("Unknown reserve command. Use 'help' for available commands.");
                        }
                    }
                } else {
                    println!("Reserve command requires subcommand. Use 'help' for available commands.");
                }
            }
            _ => {
                println!("Unknown command '{}'. Type 'help' for available commands.", parts[0]);
            }
        }

        Ok(())
    }
}