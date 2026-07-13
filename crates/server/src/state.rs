use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub database_path: PathBuf,
    pub adapter_url: String,
    pub adapter_token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct QueryParams {
    pub symbols: Option<String>,
    pub asset_ids: Option<String>,
    pub category: Option<String>,
    pub categories: Option<String>,
    pub limit: Option<String>,
    pub full: Option<String>,
    pub query: Option<String>,
    pub market: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}
