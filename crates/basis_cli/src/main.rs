mod account;
mod api;
mod commands;
mod config;
mod crypto;
mod demo_keys;
mod interactive;
mod output;

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

    #[arg(long, default_value = "http://127.0.0.1:3048")]
    server_url: String,

    #[arg(long)]
    config: Option<PathBuf>,

    /// Output machine-readable JSON instead of human-readable text
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Account management
    Account {
        #[command(subcommand)]
        cmd: commands::account::AccountCommands,
    },
    /// Generate a new secp256k1 keypair
    GenerateKeypair(commands::keypair::GenerateKeypairArgs),
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
    /// Transaction operations
    Transaction {
        #[command(subcommand)]
        cmd: commands::transaction::TransactionCommands,
    },
    /// Test operations
    Test {
        #[command(subcommand)]
        cmd: commands::test_redemption::TestCommands,
    },
    /// Interactive mode
    Interactive,
    /// Server status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let json = cli.json;
    output::set_json_mode(json);

    match run(cli).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if json {
                // JSON error contract: {"error": "<message>"} on stderr,
                // exit code 2 when the tracker server is unreachable, 1 otherwise.
                eprintln!("{}", serde_json::json!({ "error": error_chain_string(&e) }));
                std::process::exit(if is_server_unreachable(&e) { 2 } else { 1 });
            }
            Err(e)
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let json = cli.json;

    // Load configuration
    let config_manager = config::ConfigManager::new(cli.config)?;
    let mut account_manager = account::AccountManager::new(config_manager.clone())?;
    let client = api::TrackerClient::new(cli.server_url);

    match cli.command {
        Commands::Account { cmd } => {
            commands::account::handle_account_command(cmd, &mut account_manager, json).await
        }
        Commands::GenerateKeypair(args) => {
            commands::keypair::handle_generate_keypair_command(args, json).await
        }
        Commands::Note { cmd } => {
            commands::note::handle_note_command(cmd, &account_manager, &client, json).await
        }
        Commands::Reserve { cmd } => {
            commands::reserve::handle_reserve_command(cmd, &account_manager, &client, json).await
        }
        Commands::Transaction { cmd } => {
            commands::transaction::handle_transaction_command(cmd, &client, &account_manager, json)
                .await
        }
        Commands::Test { cmd } => {
            commands::test_redemption::handle_test_command(cmd, &client, json).await
        }
        Commands::Interactive => {
            let mut interactive = interactive::InteractiveMode::new(account_manager, client);
            interactive.run().await
        }
        Commands::Status => commands::status::handle_status_command(&client, json).await,
    }
}

/// Flatten an error and its sources into a single message for JSON error output.
fn error_chain_string(err: &anyhow::Error) -> String {
    err.chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

/// Heuristic check whether the error chain indicates that the tracker server
/// (or the Ergo node) could not be reached at all (connection refused,
/// timeout, DNS failure, ...). Used to pick exit code 2 over 1 in --json mode.
fn is_server_unreachable(err: &anyhow::Error) -> bool {
    const INDICATORS: [&str; 7] = [
        "connection refused",
        "failed to connect",
        "could not connect",
        "timed out",
        "timeout",
        "dns error",
        "connection reset",
    ];
    err.chain().any(|cause| {
        let msg = cause.to_string().to_lowercase();
        INDICATORS.iter().any(|indicator| msg.contains(indicator))
    })
}
