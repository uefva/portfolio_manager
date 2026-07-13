//! 共享应用状态与查询参数 DTO。
//!
//! 服务端为每个请求打开短期 SQLite 连接，避免在 Tokio 工作线程间共享阻塞型连接。

use serde::Deserialize;
use std::path::PathBuf;

/// 注入到每个 HTTP 处理器中的应用状态。
///
/// 服务端按请求打开短期 SQLite 连接，这避免了在 Tokio 工作线程间
/// 共享阻塞型 SQLite 连接带来的并发问题。
#[derive(Clone)]
pub struct AppState {
    pub database_path: PathBuf, // SQLite 数据库文件的路径
}

/// 兼容旧版价格和持仓接口的查询参数。
/// 所有值在此保持字符串类型，以保证与原始 Python HTTP API 的解析行为一致。
#[derive(Debug, Default, Deserialize)]
pub struct QueryParams {
    pub symbols: Option<String>, // 逗号分隔的代码列表，用于 /api/prices/latest
    pub asset_ids: Option<String>, // 逗号分隔的资产 ID 列表
    pub category: Option<String>, // 按类别过滤（"全部" 表示不过滤）
    pub categories: Option<String>, // 按多个类别过滤的别名参数
    pub limit: Option<String>,   // 限制返回的历史数据条数
    pub full: Option<String>,    // 是否返回完整字段（含原始价格和汇率）
    pub metric: Option<String>,  // 盈亏走势的指标："收益金额" 或 "收益率"
}
