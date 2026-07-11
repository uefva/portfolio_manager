//! Portfolio server process entry point.
//!
//! The executable only loads configuration, prepares SQLite, and starts Axum.
//! Route handlers, schema migration, and domain operations are separated into
//! focused modules to keep each change reviewable.

mod api;
mod database;
mod portfolio;
mod prices;
mod state;
mod utils;

use anyhow::Result;
use state::AppState;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<()> {
    // `RUST_LOG=debug` enables more verbose tracing without code changes.
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "portfolio_server=info,tower_http=info".to_owned()),
        )
        .with_target(false)
        .init();

    // `.env` is optional convenience for local development; no variable is
    // required to start the local server.
    dotenvy::dotenv().ok();
    let database_path = env::var("CPM_DATABASE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/price_history.sqlite3"));

    tracing::info!(database = %database_path.display(), "initializing portfolio database");
    database::initialize(&database_path)?;
    let state = Arc::new(AppState { database_path });
    let address: SocketAddr = env::var("CPM_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8765".to_owned())
        .parse()?;

    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "portfolio server listening");
    axum::serve(listener, api::router(state)).await?;
    Ok(())
}
