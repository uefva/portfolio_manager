//! HTTP client for the Rust portfolio service.

use serde_json::Value;

/// Read the endpoint from `CPM_SERVER_URL` when supplied; otherwise keep the
/// Python client's historic local-server default.
pub fn default_server_url() -> String {
    std::env::var("CPM_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned())
}

/// GET a JSON payload and unwrap the standard `{ data: ... }` API envelope.
pub async fn get(server_url: &str, path: &str) -> Result<Value, String> {
    request(reqwest::Method::GET, server_url, path, None).await
}

/// POST a JSON payload and unwrap the standard `{ data: ... }` API envelope.
pub async fn post(server_url: &str, path: &str, body: Value) -> Result<Value, String> {
    request(reqwest::Method::POST, server_url, path, Some(body)).await
}

async fn request(
    method: reqwest::Method,
    server_url: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    println!("[client] {} {}", method, url);
    let client = reqwest::Client::new();
    let request = client.request(method, &url);
    let response = match body {
        Some(body) => request.json(&body).send().await,
        None => request.send().await,
    }
    .map_err(|error| format!("服务端不可用：{error}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("服务端返回无效 JSON：{error}"))?;
    if !status.is_success() {
        return Err(value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("服务端请求失败")
            .to_owned());
    }
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(message.to_owned());
    }
    Ok(value.get("data").cloned().unwrap_or(value))
}
