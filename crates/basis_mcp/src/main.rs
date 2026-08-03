mod policy;
mod server;

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "basis-mcp")]
#[command(about = "Basis wallet MCP server (stdio transport)")]
#[command(version)]
struct Cli {
    /// Tracker server URL (default: from ~/.basis/cli.toml)
    #[arg(long, env = "BASIS_SERVER_URL")]
    server_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Logging goes to stderr only; stdout is reserved for MCP protocol messages.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    // Route basis_cli_lib progress prints (used inside the typed command cores)
    // to stderr so stdout carries only protocol messages.
    basis_cli_lib::output::set_json_mode(true);

    let cli = Cli::parse();

    let server = server::BasisMcp::new(cli.server_url)?;
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
