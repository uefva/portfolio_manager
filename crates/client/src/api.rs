//! Rust 投资组合服务端的 HTTP 客户端。
//!
//! 封装了与服务端的通信细节：
//!   - 自动拼接 URL
//!   - 统一错误处理（网络错误 vs 业务错误）
//!   - 解析服务端的 `{ data: ... }` 标准信封格式
//!   - 所有请求日志以 `[client]` 前缀输出到终端

use serde_json::Value;

/// 读取默认服务端地址。
///
/// 优先使用环境变量 `CPM_SERVER_URL`，未设置时回退到本地默认地址。
/// 这个默认值与 Python 旧版客户端的历史设定一致。
pub fn default_server_url() -> String {
    std::env::var("CPM_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8765".to_owned())
}

/// GET 请求，自动从响应中提取 `data` 字段。
///
/// 服务端统一使用 `{ "data": ... }` 格式包装成功响应，
/// 本函数会自动解包。如果响应中包含错误，返回 Err。
pub async fn get(server_url: &str, path: &str) -> Result<Value, String> {
    request(reqwest::Method::GET, server_url, path, None).await
}

/// POST 请求，自动从响应中提取 `data` 字段。
///
/// body 为 JSON Value，请求头自动设置为 `Content-Type: application/json`。
pub async fn post(server_url: &str, path: &str, body: Value) -> Result<Value, String> {
    request(reqwest::Method::POST, server_url, path, Some(body)).await
}

/// 统一的 HTTP 请求执行器。
///
/// # 错误处理
///   1. 网络层错误（服务不可达、超时等）→ "服务端不可用：{error}"
///   2. 响应非 JSON              → "服务端返回无效 JSON：{error}"
///   3. HTTP 状态码非 2xx         → 提取 `error.message` 字段
///   4. 响应体含 error 对象       → 提取 `error.message` 字段
///
/// # 返回
/// 成功时返回响应体中的 `data` 字段；如果响应体没有 `data` 字段，返回整个响应体。
async fn request(
    method: reqwest::Method,
    server_url: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    // 拼接完整 URL：移除服务端地址末尾的斜杠后再拼接路径
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    println!("[client] {} {}", method, url);

    let client = reqwest::Client::new();
    let request = client.request(method, &url);

    // 有 body 时以 JSON 格式发送，否则发送空请求
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

    // HTTP 状态码非成功 → 尝试提取错误消息
    if !status.is_success() {
        return Err(value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("服务端请求失败")
            .to_owned());
    }

    // 即使状态码为 2xx，也可能包含业务错误（如校验失败）
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(message.to_owned());
    }

    // 解包标准信封：提取 data 字段，若无则返回原始值
    Ok(value.get("data").cloned().unwrap_or(value))
}
