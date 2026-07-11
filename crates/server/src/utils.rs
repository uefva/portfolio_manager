//! Small shared helpers that deliberately keep API formatting in one place.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};

/// Split the comma-separated query format used by the legacy endpoints.
pub fn csv(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Keep timestamps compatible with existing SQLite records and JSON exports.
pub fn now_text() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn api_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"error": {"code": code, "message": message}})),
    )
}

/// Convert service errors to the established `{ error: { ... } }` response.
pub fn json_result(result: anyhow::Result<Value>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => api_error(
            StatusCode::BAD_REQUEST,
            "request_failed",
            &error.to_string(),
        )
        .into_response(),
    }
}
