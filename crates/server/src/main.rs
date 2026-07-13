//! Portfolio 服务端进程入口。
//!
//! 本可执行文件只负责加载配置、初始化 SQLite 数据库、启动 Axum HTTP 服务。
//! 路由处理、数据库迁移、领域操作分别放在独立模块中，便于逐个审查每次变更。

mod adapter;
mod api; // Axum 路由层：定义所有 HTTP 端点
mod database; // SQLite 数据库初始化与旧版数据迁移
mod portfolio; // 资产、交易、持仓与盈亏走势的核心业务逻辑
mod prices; // 历史价格查询（只读）
mod state; // 共享应用状态与请求 DTO 定义
mod utils; // 小工具函数：CSV 解析、时间戳、错误响应格式化

use anyhow::Result;
use state::AppState;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<()> {
    // `RUST_LOG=debug` 可在不改代码的情况下启用更详细的追踪日志。
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "portfolio_server=info,tower_http=info".to_owned()),
        )
        .with_target(false) // 不在日志行末尾打印目标模块名，保持简洁
        .init();

    // `.env` 是本地开发的可选便利文件；启动本地服务端不需要任何环境变量。
    dotenvy::dotenv().ok();
    let database_path = env::var("CPM_DATABASE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/price_history.sqlite3"));

    // 初始化数据库：创建表结构、从旧项目迁移数据、检查完整性
    tracing::info!(database = %database_path.display(), "正在初始化投资组合数据库");
    database::initialize(&database_path)?;

    // 构建全局共享状态，用 Arc 包装以支持多线程访问
    let state = Arc::new(AppState {
        database_path,
        adapter_url: env::var("CPM_ADAPTER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8786".to_owned()),
        adapter_token: env::var("CPM_ADAPTER_TOKEN")
            .ok()
            .filter(|value| !value.is_empty()),
    });

    // 监听地址优先使用环境变量 CPM_BIND，默认绑定本地回环地址
    let address: SocketAddr = env::var("CPM_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8765".to_owned())
        .parse()?;

    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "portfolio server 正在监听");
    axum::serve(listener, api::router(state)).await?;
    Ok(())
}
