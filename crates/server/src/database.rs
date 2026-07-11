//! SQLite bootstrap and legacy price-history migration.

use anyhow::Result;
use rusqlite::Connection;
use std::{fs, path::Path};

/// Source database is read only: first launch copies it to the new project.
const LEGACY_DATABASE: &str =
    r"E:\fengkai\my_project\crypto_portfolio_manager\price_history.sqlite3";

/// Create the compatible schema without changing any legacy business table.
pub fn initialize(database_path: &Path) -> Result<()> {
    if !database_path.exists() && Path::new(LEGACY_DATABASE).exists() {
        fs::create_dir_all(database_path.parent().unwrap_or(Path::new(".")))?;
        fs::copy(LEGACY_DATABASE, database_path)?;
        tracing::info!(source = LEGACY_DATABASE, target = %database_path.display(), "copied legacy SQLite database");
    }
    fs::create_dir_all(database_path.parent().unwrap_or(Path::new(".")))?;
    let connection = Connection::open(database_path)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    anyhow::ensure!(integrity == "ok", "SQLite integrity check failed");
    tracing::debug!(%integrity, "SQLite integrity check passed");

    connection.execute_batch(include_str!("schema.sql"))?;
    tracing::debug!("ensured SQLite schema");
    migrate_legacy_crypto_prices(&connection)?;
    Ok(())
}

/// The predecessor only stored cryptocurrency rows in `price_history`.
/// Keep that table untouched and insert its rows into the multi-asset table.
fn migrate_legacy_crypto_prices(connection: &Connection) -> Result<()> {
    let has_legacy_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='price_history')",
        [],
        |row| row.get(0),
    )?;
    if has_legacy_table {
        let migrated = connection.execute(
            "INSERT OR IGNORE INTO asset_price_history(asset_id,category,market,symbol,name,currency,price,fx_to_cny,price_cny,source,fetched_at) SELECT 'crypto:CRYPTO:' || upper(symbol),'加密货币','CRYPTO',upper(symbol),upper(symbol),'USD',price,7.2,price*7.2,coalesce(source,'legacy'),fetched_at FROM price_history",
            [],
        )?;
        tracing::info!(migrated, "migrated legacy cryptocurrency price records");
    }
    Ok(())
}
