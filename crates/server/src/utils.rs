//! 小型共享工具函数，将 API 响应格式化统一在一个位置。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};

/// 将旧版接口使用的逗号分隔查询字符串拆分为 Vec。
/// 例如 "BTC,ETH" → ["BTC", "ETH"]
pub fn csv(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// 生成与现有 SQLite 记录和 JSON 导出兼容的时间戳字符串。
/// 格式为 UTC "YYYY-MM-DD HH:MM:SS"，与旧版 Python 项目保持一致。
pub fn now_text() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 构造统一的 API 错误响应：`{ error: { code: "...", message: "..." } }`
pub fn api_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"error": {"code": code, "message": message}})),
    )
}

/// 将服务层返回的 `anyhow::Result<Value>` 转换为 HTTP 响应。
/// - 成功：直接以 JSON 格式返回
/// - 失败：包装为 `{ error: { ... } }` 格式，状态码 400
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
