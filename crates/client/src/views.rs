//! Interactive Dioxus pages backed by the Rust HTTP service.

use crate::api;
use dioxus::prelude::*;
use serde_json::{json, Value};

/// Load and display the server-calculated holdings snapshot.
#[component]
pub fn Holdings(
    status: Signal<String>,
    server_url: Signal<String>,
    holdings: Signal<Value>,
) -> Element {
    let rows = holdings()
        .get("holdings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total_profit = format!("{:.2}", holdings()["total_profit"].as_f64().unwrap_or(0.0));
    rsx! {
        div { class: "toolbar", h2 { "持仓" } button { onclick: move |_| refresh_holdings(status, server_url, holdings), "查询并保存" } }
        div { class: "summary-card", "当前总收益：{total_profit} CNY" }
        table { class: "data-table",
            thead { tr { th { "类别" } th { "市场" } th { "代码" } th { "名称" } th { "数量" } th { "当前价" } th { "持仓价值(CNY)" } th { "收益(CNY)" } } }
            tbody { for item in rows {
                tr { td { {text(&item, "category")} } td { {text(&item, "market")} } td { {text(&item, "symbol")} } td { {text(&item, "name")} } td { {number(&item, "quantity")} } td { {number(&item, "price")} } td { {number(&item, "value_cny")} } td { {number(&item, "profit_cny")} } }
            }}
        }
    }
}

/// Create assets through `POST /api/portfolio/assets`, then reload catalogue.
#[component]
pub fn Assets(
    status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
) -> Element {
    let category = use_signal(|| "加密货币".to_owned());
    let market = use_signal(|| "CRYPTO".to_owned());
    let symbol = use_signal(String::new);
    let name = use_signal(String::new);
    let rows = assets();
    rsx! {
        section { class: "form", h2 { "资产管理" }
            div { class: "fields",
                TextField { label:"类别", value: category }
                TextField { label:"市场", value: market }
                TextField { label:"代码", value: symbol }
                TextField { label:"名称", value: name }
            }
            div { class: "actions",
                button { onclick: move |_| create_asset(status, server_url, assets, category(), market(), symbol(), name()), "新增资产" }
                button { onclick: move |_| refresh_assets(status, server_url, assets), "刷新资产" }
            }
        }
        table { class: "data-table", thead { tr { th { "类别" } th { "市场" } th { "代码" } th { "名称" } th { "币种" } } }
            tbody { for item in rows { tr { td { {text(&item, "category")} } td { {text(&item, "market")} } td { {text(&item, "symbol")} } td { {text(&item, "name")} } td { {text(&item, "currency")} } } } }
        }
    }
}

/// Create buy/sell records through `POST /api/portfolio/transactions`.
#[component]
pub fn Transactions(
    status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
) -> Element {
    let category = use_signal(|| "加密货币".to_owned());
    let market = use_signal(|| "CRYPTO".to_owned());
    let symbol = use_signal(String::new);
    let name = use_signal(String::new);
    let transaction_type = use_signal(|| "buy".to_owned());
    let amount = use_signal(String::new);
    let price = use_signal(String::new);
    let date = use_signal(String::new);
    let rows = transactions();
    rsx! {
        section { class: "form", h2 { "新增交易" }
            div { class: "fields",
                TextField { label:"类别", value: category }
                TextField { label:"市场", value: market }
                TextField { label:"代码", value: symbol }
                TextField { label:"名称", value: name }
                TextField { label:"类型（buy/sell）", value: transaction_type }
                TextField { label:"数量", value: amount }
                TextField { label:"价格", value: price }
                TextField { label:"日期", value: date }
            }
            div { class: "actions", button { onclick: move |_| create_transaction(status, server_url, transactions, category(), market(), symbol(), name(), transaction_type(), amount(), price(), date()), "新增交易" } button { onclick: move |_| refresh_transactions(status, server_url, transactions), "刷新交易" } }
        }
        table { class: "data-table", thead { tr { th { "类型" } th { "代码" } th { "日期" } th { "数量" } th { "价格" } th { "成交额" } } }
            tbody { for item in rows { tr { td { {text(&item,"type")} } td { {text(&item,"symbol")} } td { {text(&item,"date")} } td { {number(&item,"amount")} } td { {number(&item,"price")} } td { {number(&item,"total")} } } } }
        }
    }
}

/// Export confirms connectivity for snapshots until native file pickers are added.
#[component]
pub fn Snapshots(status: Signal<String>, server_url: Signal<String>) -> Element {
    rsx! { div { class:"toolbar", h2 { "历史持仓结果" } button { onclick: move |_| export_portfolio(status, server_url), "导出服务端数据" } } p { "Desktop 快照文件与 Web 导入导出将在下一步接入。" } }
}

/// Profit-history endpoint is queried so the chart button has real server work.
#[component]
pub fn Chart(status: Signal<String>, server_url: Signal<String>, chart: Signal<Value>) -> Element {
    let labels = chart()["labels"].as_array().cloned().unwrap_or_default();
    let points = chart()["series"]["总资产"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let values = points
        .iter()
        .filter_map(|point| point.get(1).and_then(Value::as_f64))
        .collect::<Vec<_>>();
    let polyline = chart_polyline(&values, 860.0, 280.0);
    let minimum = format!(
        "{:.2}",
        values.iter().copied().reduce(f64::min).unwrap_or(0.0)
    );
    let maximum = format!(
        "{:.2}",
        values.iter().copied().reduce(f64::max).unwrap_or(0.0)
    );
    rsx! {
        div { class:"toolbar", h2 { "收益走势" } button { onclick: move |_| refresh_chart(status, server_url, chart), "刷新图表" } }
        div { class:"chart",
            if values.is_empty() { "暂无收益走势数据。请确认服务端已有历史价格记录。" }
            else {
                div { class:"chart-meta", "{labels.len()} 个时间点 · 最低 {minimum} · 最高 {maximum}" }
                svg { class:"profit-svg", view_box:"0 0 900 320", preserve_aspect_ratio:"none",
                    line { x1:"20", y1:"300", x2:"880", y2:"300", class:"chart-axis" }
                    polyline { points:"{polyline}", class:"profit-line" }
                }
            }
        }
    }
}

/// Reusable controlled text field. The signal is updated on every keystroke.
#[component]
fn TextField(label: &'static str, mut value: Signal<String>) -> Element {
    rsx! { label { "{label}", input { value:"{value()}", placeholder:"{label}", oninput: move |event| value.set(event.value()) } } }
}

fn refresh_holdings(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut holdings: Signal<Value>,
) {
    let url = server_url();
    spawn(async move {
        status.set("正在加载持仓…".into());
        match api::get(&url, "/api/portfolio/holdings?category=全部").await {
            Ok(value) => {
                let count = value["holdings"].as_array().map_or(0, Vec::len);
                println!("[client] holdings loaded: {count}");
                holdings.set(value);
                status.set(format!("持仓已刷新：{count} 项"));
            }
            Err(error) => status.set(error),
        }
    });
}
fn refresh_assets(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut assets: Signal<Vec<Value>>,
) {
    let url = server_url();
    spawn(async move {
        status.set("正在加载资产…".into());
        match api::get(&url, "/api/portfolio/assets").await {
            Ok(value) => {
                let values = value.as_array().cloned().unwrap_or_default();
                let count = values.len();
                assets.set(values);
                status.set(format!("资产已刷新：{count} 项"));
            }
            Err(error) => status.set(error),
        }
    });
}
fn refresh_transactions(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut transactions: Signal<Vec<Value>>,
) {
    let url = server_url();
    spawn(async move {
        status.set("正在加载交易…".into());
        match api::get(&url, "/api/portfolio/transactions").await {
            Ok(value) => {
                let values = value.as_array().cloned().unwrap_or_default();
                let count = values.len();
                transactions.set(values);
                status.set(format!("交易已刷新：{count} 条"));
            }
            Err(error) => status.set(error),
        }
    });
}
fn create_asset(
    mut status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
    category: String,
    market: String,
    symbol: String,
    name: String,
) {
    let url = server_url();
    spawn(async move {
        if symbol.trim().is_empty() {
            status.set("资产代码不能为空".into());
            return;
        }
        status.set("正在保存资产…".into());
        match api::post(
            &url,
            "/api/portfolio/assets",
            json!({"category":category,"market":market,"symbol":symbol,"name":name}),
        )
        .await
        {
            Ok(_) => {
                status.set("资产已保存，正在刷新列表…".into());
                refresh_assets(status, server_url, assets)
            }
            Err(error) => status.set(error),
        }
    });
}
fn create_transaction(
    mut status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
    category: String,
    market: String,
    symbol: String,
    name: String,
    transaction_type: String,
    amount: String,
    price: String,
    date: String,
) {
    let url = server_url();
    spawn(async move {
        let amount = match amount.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                status.set("数量必须是数字".into());
                return;
            }
        };
        let price = match price.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                status.set("价格必须是数字".into());
                return;
            }
        };
        status.set("正在保存交易…".into());
        match api::post(&url,"/api/portfolio/transactions",json!({"category":category,"market":market,"symbol":symbol,"name":name,"type":transaction_type,"amount":amount,"price":price,"date":date})).await { Ok(_)=>{status.set("交易已保存，正在刷新列表…".into());refresh_transactions(status,server_url,transactions)},Err(error)=>status.set(error)}
    });
}
fn export_portfolio(mut status: Signal<String>, server_url: Signal<String>) {
    let url = server_url();
    spawn(async move {
        status.set("正在导出服务端数据…".into());
        match api::get(&url, "/api/portfolio/export").await {
            Ok(value) => {
                let count = value["assets"].as_object().map_or(0, |assets| assets.len());
                status.set(format!(
                    "服务端导出成功：{count} 项资产（下载文件功能待接入）"
                ));
            }
            Err(error) => status.set(error),
        }
    });
}
fn refresh_chart(mut status: Signal<String>, server_url: Signal<String>, mut chart: Signal<Value>) {
    let url = server_url();
    spawn(async move {
        status.set("正在加载收益走势…".into());
        match api::get(&url, "/api/portfolio/profit-history").await {
            Ok(value) => {
                let points = value["labels"].as_array().map_or(0, Vec::len);
                chart.set(value);
                status.set(format!("收益走势已加载：{points} 个时间点"));
            }
            Err(error) => status.set(error),
        }
    });
}

/// Scale a series into an SVG polyline without adding a heavyweight chart crate.
fn chart_polyline(values: &[f64], width: f64, height: f64) -> String {
    if values.is_empty() {
        return String::new();
    }
    let minimum = values.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let maximum = values.iter().copied().reduce(f64::max).unwrap_or(0.0);
    let range = (maximum - minimum).max(1.0);
    let denominator = values.len().saturating_sub(1).max(1) as f64;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = 20.0 + index as f64 / denominator * width;
            let y = 20.0 + (maximum - value) / range * height;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("-").to_owned()
}
fn number(value: &Value, key: &str) -> String {
    value[key]
        .as_f64()
        .map(|number| format!("{number:.2}"))
        .unwrap_or_else(|| "-".into())
}
