//! Small operational CLI for checking the Rust HTTP service without the UI.

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Command-line options intentionally mirror the server's default local URL.
#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8765")]
    server: String,
    #[command(subcommand)]
    command: Command,
}
/// Read-only requests safe to use for deployment diagnostics.
#[derive(Subcommand)]
enum Command {
    Health,
    Assets,
    Holdings,
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    // Keep endpoint selection explicit so CLI usage documents the public API.
    let path = match cli.command {
        Command::Health => "/api/health",
        Command::Assets => "/api/portfolio/assets",
        Command::Holdings => "/api/portfolio/holdings",
    };
    let response = reqwest::get(format!("{}{}", cli.server, path))
        .await?
        .text()
        .await?;
    println!("{response}");
    Ok(())
}
