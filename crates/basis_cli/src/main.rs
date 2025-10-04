mod account;
mod api;
mod commands;
mod config;
mod crypto;
mod interactive;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "basis-cli")]
#[command(about = "Basis Tracker CLI Client")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    server_url: String,
    
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Account management
    Account {
        #[command(subcommand)]
        cmd: commands::account::AccountCommands,
    },
    /// Note operations
    Note {
        #[command(subcommand)]
        cmd: commands::note::NoteCommands,
    },
    /// Reserve operations
    Reserve {
        #[command(subcommand)]
        cmd: commands::reserve::ReserveCommands,
    },
    /// Interactive mode
    Interactive,
    /// Server status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Load configuration
    let config_manager = config::ConfigManager::new(cli.config)?;
    let mut account_manager = account::AccountManager::new(config_manager.clone())?;
    let client = api::TrackerClient::new(cli.server_url);
    
    match cli.command {
        Commands::Account { cmd } => {
            commands::account::handle_account_command(cmd, &mut account_manager).await
        }
        Commands::Note { cmd } => {
            commands::note::handle_note_command(cmd, &account_manager, &client).await
        }
        Commands::Reserve { cmd } => {
            commands::reserve::handle_reserve_command(cmd, &account_manager, &client).await
        }
        Commands::Interactive => {
            let mut interactive = interactive::InteractiveMode::new(account_manager, client);
            interactive.run().await
        }
        Commands::Status => {
            commands::status::handle_status_command(&client).await
        }
    }
}