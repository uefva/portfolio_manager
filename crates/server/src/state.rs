//! Shared application state and query DTOs.

use serde::Deserialize;
use std::path::PathBuf;

/// State injected into every HTTP handler.
///
/// The server opens short-lived SQLite connections per request. This avoids
/// sharing a blocking SQLite connection across Tokio worker threads.
#[derive(Clone)]
pub struct AppState {
    pub database_path: PathBuf,
}

/// Query parameters used by the legacy price and portfolio endpoints.
/// All values remain strings here so the parsing behaviour is compatible with
/// the original Python HTTP API.
#[derive(Debug, Default, Deserialize)]
pub struct QueryParams {
    pub symbols: Option<String>,
    pub asset_ids: Option<String>,
    pub category: Option<String>,
    pub categories: Option<String>,
    pub limit: Option<String>,
    pub full: Option<String>,
    pub metric: Option<String>,
}
