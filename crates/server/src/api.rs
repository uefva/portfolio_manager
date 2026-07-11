//! Axum routing layer. Handlers stay thin and delegate business work to modules.

use crate::{
    portfolio, prices,
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

/// Build the public, unauthenticated local API. Authentication is intentionally
/// deferred until the core desktop workflow is stable.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/symbols", get(symbols))
        .route("/api/prices/latest", get(prices_latest))
        .route("/api/prices/history", get(prices_history))
        .route("/api/assets/latest", get(assets_latest))
        .route("/api/assets/history", get(assets_history))
        .route("/api/portfolio/assets", get(assets).post(asset_create))
        .route(
            "/api/portfolio/assets/:id",
            put(asset_update).delete(asset_delete),
        )
        .route(
            "/api/portfolio/transactions",
            get(transactions).post(transaction_create),
        )
        .route(
            "/api/portfolio/transactions/:id",
            put(transaction_update).delete(transaction_delete),
        )
        .route("/api/portfolio/holdings", get(holdings))
        .route("/api/portfolio/summary", get(summary))
        .route("/api/portfolio/profit-history", get(profit_history))
        .route("/api/portfolio/export", get(export_portfolio))
        .route("/api/portfolio/import", post(import_portfolio))
        .with_state(state)
        // Emits method, route, status and request duration through `tracing`.
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        // Current local-first mode intentionally permits the Desktop WebView
        // and development web host to call the same server.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
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

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}
async fn symbols() -> Json<Value> {
    Json(json!({"symbols":[]}))
}
async fn prices_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::latest_crypto_prices(
        &state.database_path,
        &csv(query.symbols),
    ))
}
async fn prices_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::asset_history(
        &state.database_path,
        &csv(query.symbols),
        query.limit,
        true,
    ))
}
async fn assets_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::latest_assets(
        &state.database_path,
        &csv(query.asset_ids),
        &csv(query.categories.or(query.category)),
    ))
}
async fn assets_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(prices::asset_history(
        &state.database_path,
        &csv(query.asset_ids),
        query.limit,
        query.full.as_deref() == Some("1"),
    ))
}
async fn assets(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_assets(&state.database_path))
}
async fn asset_create(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> Response {
    json_result(
        portfolio::save_asset(&state.database_path, None, &input)
            .map(|value| json!({"data":value})),
    )
}
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
async fn asset_delete(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<String>,
) -> Response {
    json_result(portfolio::delete_asset(&state.database_path, &id))
}
async fn transactions(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::list_transactions(&state.database_path))
}
async fn transaction_create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::save_transaction(&state.database_path, None, &input)
            .map(|value| json!({"data":value})),
    )
}
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
async fn transaction_delete(
    State(state): State<Arc<AppState>>,
    RoutePath(id): RoutePath<i64>,
) -> Response {
    json_result(portfolio::delete_transaction(&state.database_path, id))
}
async fn holdings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    json_result(
        portfolio::holdings(
            &state.database_path,
            query.category.as_deref().unwrap_or("全部"),
        )
        .map(|value| json!({"data":value})),
    )
}
async fn summary(State(state): State<Arc<AppState>>, Query(query): Query<QueryParams>) -> Response {
    json_result(portfolio::holdings(&state.database_path,query.category.as_deref().unwrap_or("全部")).map(|value|json!({"data":{"total_value":value["total_value"],"total_cost":value["total_cost"],"total_profit":value["total_profit"],"total_profit_rate":value["total_profit_rate"],"category_totals":value["category_totals"]}})))
}
async fn profit_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryParams>,
) -> Response {
    let metric = query.metric.as_deref().unwrap_or("收益金额");
    json_result(
        portfolio::profit_history(&state.database_path, metric).map(|value| json!({"data":value})),
    )
}
async fn export_portfolio(State(state): State<Arc<AppState>>) -> Response {
    json_result(portfolio::export_json(&state.database_path).map(|value| json!({"data":value})))
}
async fn import_portfolio(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> Response {
    json_result(
        portfolio::import_json(
            &state.database_path,
            input.get("portfolio").cloned().unwrap_or(input),
        )
        .map(|value| json!({"data":value})),
    )
}
