//! Axum 路由层。
//!
//! 本模块负责定义所有 HTTP 端点、构建 Router、配置中间件。
//! 每个 handler 保持轻量：只做参数提取，将实际工作委托给 portfolio/prices 模块。

use crate::{
    adapter, portfolio, prices,
    state::{AppState, QueryParams},
    utils::{csv, json_result},
};
use axum::{
    extract::{Path as RoutePath, Query, State},
    http::{header, Method},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

/// 构建公开、无认证的本地 API 路由。
///
/// 当前阶段故意不实现认证，等桌面端核心工作流稳定后再加入。
/// 中间件栈（由外到内）：
///   1. TraceLayer: 记录方法、路由、状态码和请求耗时
///   2. CompressionLayer: gzip 压缩响应体
///   3. CorsLayer: 允许本地 WebView 和开发环境跨域访问
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // ── 健康检查与行情接口 ──
        .route("/api/health", get(health))
        .route("/api/symbols", get(symbols))
        .route("/api/prices/refresh", post(refresh_prices))
        .route("/api/prices/latest", get(prices_latest))
        .route("/api/prices/history", get(prices_history))
        .route("/api/assets/latest", get(assets_latest))
        .route("/api/assets/history", get(assets_history))
        // ── 资产 CRUD ──
        .route("/api/portfolio/assets", get(assets).post(asset_create))
        .route(
            "/api/portfolio/assets/:id",
            put(asset_update).delete(asset_delete),
        )
        // ── 交易记录 CRUD ──
        .route(
            "/api/portfolio/transactions",
            get(transactions).post(transaction_create),
        )
        .route(
            "/api/portfolio/transactions/:id",
            put(transaction_update).delete(transaction_delete),
        )
        // ── 持仓与盈亏 ──
        .route("/api/portfolio/holdings", get(holdings))
        .route("/api/portfolio/holdings/query", post(query_latest_holdings))
        .route("/api/portfolio/holding-snapshots", get(holding_snapshots))
        .route(
            "/api/portfolio/holding-snapshots/:id",
            get(holding_snapshot),
        )
        .route("/api/portfolio/summary", get(summary))
        .route("/api/portfolio/profit-history", get(profit_history))
        // ── 导入导出 ──
        .route("/api/portfolio/export", get(export_portfolio))
        .route("/api/portfolio/import", post(import_portfolio))
        .route(
            "/api/portfolio/snapshots",
            get(snapshots).post(snapshot_create),
        )
        .route(
            "/api/portfolio/snapshots/:id",
            axum::routing::delete(snapshot_delete),
        )
        .with_state(state)
        // 通过 tracing 记录每次请求的方法、路由、状态码和耗时
        .layer(TraceLayer::new_for_http())
        // 对超过阈值（默认 256 字节）的响应体启用 gzip 压缩
        .layer(CompressionLayer::new())
        // 当前本地优先模式故意允许 Desktop WebView 和开发版 web host 调用同一服务端
        .layer(
            CorsLayer::new()
                .allow_origin(Any) // 允许任意来源（仅限本地/内网使用）
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([header::CONTENT_TYPE]),
        )
}

// ──────────────────── Handler 函数 ────────────────────
// 每个 handler 职责单一：从请求中提取参数 → 调用业务模块 → 格式化响应。

/// 健康检查：用于监控和客户端连通性测试。
async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

/// 符号列表（占位，当前返回空数组以保持 API 兼容性）。
async fn symbols(State(state): State<Arc<AppState>>, Query(query): Query<QueryParams>) -> Response {
    json_result(
        adapter::search(
            &state.adapter_url,
            state.adapter_token.as_deref(),
            query.query.as_deref().unwrap_or(""),
            query.category.as_deref(),
            query.market.as_deref(),
        )
        .await
        .map(|value| json!({"data": value["items"].clone()})),
    )
}

async fn refresh_prices(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> Response {
    json_result(
        adapter::refresh(
            &state.database_path,
            &state.adapter_url,
            state.adapter_token.as_deref(),
            input.get("assets").cloned(),
        )
        .await,
    )
}

/// 获取指定符号的最新加密货币价格（兼容旧版接口）。
/// GET /api/prices/latest?symbols=BTC,ETH
async fn prices_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::latest_crypto_prices(
        &state.database_path,
        &csv(query.symbols), // 将逗号分隔的符号字符串拆为 Vec
    ))
}

/// 获取指定符号的历史价格（兼容旧版接口）。
/// GET /api/prices/history?symbols=BTC,ETH&limit=2000
async fn prices_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::asset_history(
        &state.database_path,
        &csv(query.symbols),
        query.limit,
        true, // 默认返回完整字段
    ))
}

/// 获取指定资产的最新报价（按 asset_id 和 category 过滤）。
/// GET /api/assets/latest?asset_ids=crypto:CRYPTO:BTC
async fn assets_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::latest_assets(
        &state.database_path,
        &csv(query.asset_ids),
        &csv(query.categories.or(query.category)), // 同时兼容 categories 和 category 参数
    ))
}

/// 获取指定资产的历史价格时间序列。
/// GET /api/assets/history?asset_ids=crypto:CRYPTO:BTC&full=1
async fn assets_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::asset_history(
        &state.database_path,
        &csv(query.asset_ids),
        query.limit,
        query.full.as_deref() == Some("1"), // full=1 时返回原始价格和汇率字段
    ))
}

/// 列出所有资产。
/// GET /api/portfolio/assets
async fn assets(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_assets(&state.database_path))
}

/// 新增资产。
/// POST /api/portfolio/assets
async fn asset_create(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> Response {
    json_result(
        portfolio::save_asset(&state.database_path, None, &input)
            .map(|value| json!({"data":value})),
    )
}

/// 更新资产（通过 URL 路径中的 :id 定位）。
/// PUT /api/portfolio/assets/:id
async fn asset_update(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<String>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::save_asset(&state.database_path, Some(id), &input)
            .map(|value| json!({"data":value})),
    )
}

/// 删除资产（仅允许删除没有关联交易的资产）。
/// DELETE /api/portfolio/assets/:id
async fn asset_delete(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    json_result(portfolio::delete_asset(&state.database_path, &id))
}

/// 列出所有交易记录（关联了资产信息）。
/// GET /api/portfolio/transactions
async fn transactions(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_transactions(&state.database_path))
}

/// 新增交易记录。
/// POST /api/portfolio/transactions
/// 如果请求中未提供 asset_id，会自动根据 category/market/symbol 创建资产。
async fn transaction_create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::save_transaction(&state.database_path, None, &input)
            .map(|value| json!({"data":value})),
    )
}

/// 更新交易记录。
/// PUT /api/portfolio/transactions/:id
async fn transaction_update(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<i64>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::save_transaction(&state.database_path, Some(id), &input)
            .map(|value| json!({"data":value})),
    )
}

/// 删除交易记录。
/// DELETE /api/portfolio/transactions/:id
async fn transaction_delete(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<i64>,
) -> Response {
    json_result(portfolio::delete_transaction(&state.database_path, id))
}

/// 查询持仓汇总（按类别过滤，默认 "全部"）。
/// GET /api/portfolio/holdings?category=all
/// 返回每项资产的持仓量、均价、当前价、人民币市值、人民币成本和盈亏。
async fn holdings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(
        portfolio::holdings(
            &state.database_path,
            query.category.as_deref().unwrap_or("all"),
        )
        .map(|value| json!({"data":value})),
    )
}

async fn query_latest_holdings(State(state): State<Arc<AppState>>) -> Response {
    let result = async {
        adapter::refresh(
            &state.database_path,
            &state.adapter_url,
            state.adapter_token.as_deref(),
            Some(portfolio::active_assets(&state.database_path)?["data"].clone()),
        )
        .await?;
        let holdings = portfolio::holdings(&state.database_path, "all")?;
        let snapshot = portfolio::save_holding_query_snapshot(&state.database_path, &holdings)?;
        Ok::<_, anyhow::Error>(json!({"data": {"holdings": holdings, "snapshot": snapshot}}))
    }
    .await;
    json_result(result)
}

async fn holding_snapshots(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_holding_query_snapshots(
        &state.database_path,
    ))
}

async fn holding_snapshot(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<i64>,
) -> Response {
    json_result(portfolio::holding_query_snapshot(&state.database_path, id))
}

/// 查询持仓摘要（只返回汇总数据，不含明细）。
/// GET /api/portfolio/summary?category=all
async fn summary(State(state): State<Arc<AppState>>, Query(query): Query<QueryParams>) -> Response {
    json_result(
        portfolio::holdings(
            &state.database_path,
            query.category.as_deref().unwrap_or("all"),
        )
        .map(|value| {
            // 从完整持仓结果中提取汇总字段
            json!({"data":{
                "total_value":      value["total_value"],
                "total_cost":       value["total_cost"],
                "total_profit":     value["total_profit"],
                "total_profit_rate": value["total_profit_rate"],
                "category_totals":  value["category_totals"],
            }})
        }),
    )
}

/// 查询盈亏历史走势数据。
/// GET /api/portfolio/profit-history?metric=收益金额
/// metric 可选值："收益金额"（默认）或 "收益率"
async fn profit_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(
        portfolio::profit_history(
            &state.database_path,
            "收益金额",
            query.from.as_deref(),
            query.to.as_deref(),
        )
        .map(|value| json!({"data":value})),
    )
}

/// 导出投资组合为 v2 JSON 格式（兼容旧版桌面客户端）。
/// GET /api/portfolio/export
async fn export_portfolio(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::export_json(&state.database_path).map(|value| json!({"data":value})))
}

/// 从 v2 JSON 格式导入投资组合。
/// POST /api/portfolio/import
/// 导入是幂等的：相同 asset/type/date/amount/price 的交易会被跳过。
async fn import_portfolio(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::import_json(
            &state.database_path,
            // 兼容两种请求格式：{"portfolio": {...}} 或直接 {...}
            input.get("portfolio").cloned().unwrap_or(input),
        )
        .map(|value| json!({"data":value})),
    )
}

async fn snapshots(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_snapshots(&state.database_path))
}

async fn snapshot_create(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> Response {
    json_result(portfolio::create_snapshot(
        &state.database_path,
        input["name"].as_str().unwrap_or(""),
    ))
}

async fn snapshot_delete(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<i64>,
) -> Response {
    json_result(portfolio::delete_snapshot(&state.database_path, id))
}
