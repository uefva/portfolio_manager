use crate::api;
use chrono::{Duration, Utc};
use dioxus::prelude::*;
use serde_json::{json, Value};

const TOTAL: &str = "total";
const CRYPTO: &str = "crypto";
const FUND: &str = "fund";
const STOCK: &str = "stock";

#[component]
pub fn Holdings(
    status: Signal<String>,
    server_url: Signal<String>,
    holdings: Signal<Value>,
) -> Element {
    rsx! {
        div { class: "toolbar",
            h2 { "持仓" }
            button { onclick: move |_| query_latest_holdings(status, server_url, holdings), "查询最新市场价" }
        }
        HoldingTable { data: holdings() }
    }
}

#[component]
fn HoldingTable(data: Value) -> Element {
    let rows = data["holdings"].as_array().cloned().unwrap_or_default();
    let total_profit = number(&data, "total_profit");
    rsx! {
        div { class: "summary-card", "当前总收益：{total_profit} CNY" }
        table { class: "data-table",
            thead { tr { th { "类别" } th { "市场" } th { "代码" } th { "名称" } th { "数量" } th { "当前价" } th { "持仓价值(CNY)" } th { "收益(CNY)" } } }
            tbody { for item in rows {
                tr { td { {text(&item, "category")} } td { {text(&item, "market")} } td { {text(&item, "symbol")} } td { {text(&item, "name")} }
                    td { {number(&item, "quantity")} } td { {number(&item, "price")} } td { {number(&item, "value_cny")} } td { {number(&item, "profit_cny")} } }
            }}
        }
    }
}

#[component]
pub fn Assets(
    status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
) -> Element {
    let category = use_signal(|| CRYPTO.to_owned());
    let market = use_signal(|| "CRYPTO".to_owned());
    let symbol = use_signal(String::new);
    let name = use_signal(String::new);
    let selected_id = use_signal(|| None::<String>);
    let rows = assets();
    rsx! {
        section { class: "form",
            h2 { if selected_id().is_some() { "编辑资产" } else { "新增资产" } }
            div { class: "fields",
                TextField { label: "类别", value: category } TextField { label: "市场", value: market }
                TextField { label: "代码", value: symbol } TextField { label: "名称", value: name }
            }
            div { class: "actions",
                button { onclick: move |_| create_asset(status, server_url, assets, category(), market(), symbol(), name(), selected_id), "新增" }
                button { onclick: move |_| update_asset(status, server_url, assets, selected_id(), category(), market(), symbol(), name(), selected_id), "更新选中项" }
                button { onclick: move |_| delete_asset(status, server_url, assets, selected_id(), selected_id), "删除选中项" }
                button { onclick: move |_| refresh_assets(status, server_url, assets), "刷新列表" }
            }
        }
        p { "点击下方资产行即可填充表单并进入编辑模式。" }
        table { class: "data-table", thead { tr { th { "类别" } th { "市场" } th { "代码" } th { "名称" } th { "币种" } } }
            tbody { for item in rows { AssetRow { item, selected_id, category, market, symbol, name } } }
        }
    }
}

#[component]
fn AssetRow(
    item: Value,
    mut selected_id: Signal<Option<String>>,
    mut category: Signal<String>,
    mut market: Signal<String>,
    mut symbol: Signal<String>,
    mut name: Signal<String>,
) -> Element {
    let selected = item.clone();
    rsx! { tr { onclick: move |_| {
        selected_id.set(selected["asset_id"].as_str().map(ToOwned::to_owned));
        category.set(asset_category_code(&selected)); market.set(text(&selected, "market"));
        symbol.set(text(&selected, "symbol")); name.set(text(&selected, "name"));
    }, td { {text(&item, "category")} } td { {text(&item, "market")} } td { {text(&item, "symbol")} } td { {text(&item, "name")} } td { {text(&item, "currency")} } } }
}

#[component]
pub fn Transactions(
    status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
    assets: Signal<Vec<Value>>,
) -> Element {
    let mut category = use_signal(|| CRYPTO.to_owned());
    let mut market = use_signal(|| "CRYPTO".to_owned());
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| "buy".to_owned());
    let amount = use_signal(String::new);
    let price = use_signal(String::new);
    let mut date = use_signal(String::new);
    let mut selected_asset_id = use_signal(String::new);
    let selected_id = use_signal(|| None::<i64>);
    let rows = transactions();
    let asset_rows = assets();
    let asset_options = asset_rows.clone();
    rsx! {
        section { class: "form", h2 { if selected_id().is_some() { "编辑交易" } else { "新增交易" } }
            div { class: "fields",
                SelectField { label: "类别", value: category, options: vec![CRYPTO.to_owned(), FUND.to_owned(), STOCK.to_owned()] }
                SelectField { label: "市场", value: market, options: vec!["CRYPTO".into(), "SH".into(), "SZ".into(), "HK".into(), "NASDAQ".into()] }
                label { "名称", select { value: "{selected_asset_id()}", oninput: move |event| {
                    let id = event.value();
                    if let Some(asset) = asset_options.iter().find(|asset| asset["asset_id"].as_str() == Some(id.as_str())) {
                        selected_asset_id.set(id); category.set(asset_category_code(asset)); market.set(text(asset, "market")); name.set(text(asset, "name"));
                    }
                }, option { value: "", "请选择已注册资产" }
                    for asset in asset_rows { AssetOption { asset } }
                } }
                label { "类型", select { value: "{kind()}", oninput: move |event| kind.set(event.value()), option { value: "buy", "买入" } option { value: "sell", "卖出" } } }
                TextField { label: "数量", value: amount } TextField { label: "价格", value: price }
                label { "时间", input { r#type: "datetime-local", value: "{date()}", oninput: move |event| date.set(event.value()) } }
            }
            div { class: "actions",
                button { onclick: move |_| create_transaction(status, server_url, transactions, selected_asset_id(), kind(), amount(), price(), date(), selected_id), "新增" }
                button { onclick: move |_| update_transaction(status, server_url, transactions, selected_id(), selected_asset_id(), kind(), amount(), price(), date(), selected_id), "更新选中项" }
                button { onclick: move |_| delete_transaction(status, server_url, transactions, selected_id(), selected_id), "删除选中项" }
                button { onclick: move |_| refresh_transactions(status, server_url, transactions), "刷新列表" }
            }
        }
        p { "点击下方交易行即可填充表单并进入编辑模式。" }
        table { class: "data-table", thead { tr { th { "类别" } th { "市场" } th { "名称" } th { "类型" } th { "时间" } th { "数量" } th { "价格" } th { "成交额" } } }
            tbody { for item in rows { TransactionRow { item, selected_id, selected_asset_id, category, market, name, kind, amount, price, date } } }
        }
    }
}

#[component]
fn TransactionRow(
    item: Value,
    mut selected_id: Signal<Option<i64>>,
    mut selected_asset_id: Signal<String>,
    mut category: Signal<String>,
    mut market: Signal<String>,
    mut name: Signal<String>,
    mut kind: Signal<String>,
    mut amount: Signal<String>,
    mut price: Signal<String>,
    mut date: Signal<String>,
) -> Element {
    let selected = item.clone();
    rsx! { tr { onclick: move |_| {
        selected_id.set(selected["id"].as_i64()); selected_asset_id.set(text(&selected, "asset_id")); category.set(asset_category_code(&selected)); market.set(text(&selected, "market")); name.set(text(&selected, "name")); kind.set(text(&selected, "type")); amount.set(selected["amount"].as_f64().map(|v| v.to_string()).unwrap_or_default()); price.set(selected["price"].as_f64().map(|v| v.to_string()).unwrap_or_default()); date.set(datetime_local(&text(&selected, "date")));
    }, td { {text(&item, "category")} } td { {text(&item, "market")} } td { {text(&item, "name")} } td { {text(&item, "type")} } td { {text(&item, "date")} } td { {number(&item, "amount")} } td { {number(&item, "price")} } td { {number(&item, "total")} } } }
}

#[component]
pub fn Snapshots(status: Signal<String>, server_url: Signal<String>) -> Element {
    let snapshots = use_signal(Vec::<Value>::new);
    let selected = use_signal(|| Value::Null);
    let rows = snapshots();
    rsx! {
        div { class: "toolbar", h2 { "历史持仓结果" } button { onclick: move |_| refresh_holding_snapshots(status, server_url, snapshots), "刷新历史结果" } }
        table { class: "data-table", thead { tr { th { "查询时间" } th { "总市值(CNY)" } th { "总收益(CNY)" } } }
            tbody { for item in rows { HoldingSnapshotRow { item, status, server_url, selected } } }
        }
        if !selected().is_null() { h3 { "所选历史结果" } HoldingTable { data: selected() } }
    }
}

#[component]
fn HoldingSnapshotRow(
    item: Value,
    status: Signal<String>,
    server_url: Signal<String>,
    selected: Signal<Value>,
) -> Element {
    let id = item["id"].as_i64().unwrap_or_default();
    rsx! { tr { onclick: move |_| load_holding_snapshot(status, server_url, selected, id), td { {text(&item, "queried_at")} } td { {number(&item, "total_value")} } td { {number(&item, "total_profit")} } } }
}

#[component]
pub fn Chart(status: Signal<String>, server_url: Signal<String>, chart: Signal<Value>) -> Element {
    let mut range = use_signal(|| "30".to_owned());
    let from = use_signal(String::new);
    let to = use_signal(String::new);
    let mut show_total = use_signal(|| true);
    let mut show_crypto = use_signal(|| true);
    let mut show_fund = use_signal(|| true);
    let mut show_stock = use_signal(|| true);
    let labels = chart()["labels"].as_array().cloned().unwrap_or_default();
    let start_label = labels
        .first()
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_owned();
    let end_label = labels
        .last()
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_owned();
    let selected = [
        (TOTAL, show_total(), "#2563eb"),
        (CRYPTO, show_crypto(), "#f97316"),
        (FUND, show_fund(), "#16a34a"),
        (STOCK, show_stock(), "#9333ea"),
    ];
    let values = selected
        .iter()
        .filter(|(_, enabled, _)| *enabled)
        .flat_map(|(label, _, _)| series_values(&chart(), label))
        .collect::<Vec<_>>();
    let min = values.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let max = values.iter().copied().reduce(f64::max).unwrap_or(0.0);
    rsx! {
        div { class: "toolbar", h2 { "收益走势" }
            button { onclick: move |_| refresh_chart(status, server_url, chart, range(), from(), to()), "应用范围" }
        }
        div { class: "fields",
            button { onclick: move |_| range.set("7".into()), "7天" } button { onclick: move |_| range.set("30".into()), "30天" } button { onclick: move |_| range.set("90".into()), "90天" } button { onclick: move |_| range.set("365".into()), "1年" } button { onclick: move |_| range.set("all".into()), "全部" } button { onclick: move |_| range.set("custom".into()), "自定义" }
            if range() == "custom" { TextField { label: "开始日期 YYYY-MM-DD", value: from } TextField { label: "结束日期 YYYY-MM-DD", value: to } }
        }
        div { class: "chart", div { class: "chart-options", label { input { r#type: "checkbox", checked: show_total(), onclick: move |_| show_total.toggle() } "总资产" } label { input { r#type: "checkbox", checked: show_crypto(), onclick: move |_| show_crypto.toggle() } "加密货币" } label { input { r#type: "checkbox", checked: show_fund(), onclick: move |_| show_fund.toggle() } "基金" } label { input { r#type: "checkbox", checked: show_stock(), onclick: move |_| show_stock.toggle() } "股票" } }
            if values.is_empty() { p { "暂无所选范围内的收益数据。" } } else { svg { class: "profit-svg", view_box: "0 0 900 320", preserve_aspect_ratio: "none", line { x1: "20", y1: "300", x2: "880", y2: "300", class: "chart-axis" }
                for (label, enabled, color) in selected { if enabled { SeriesLine { values: series_values(&chart(), label), min, max, color } } }
            } }
            div { class: "chart-meta", "X 轴：{start_label} 至 {end_label}；Y 轴：收益金额（CNY）" }
        }
    }
}

#[component]
fn TextField(label: &'static str, mut value: Signal<String>) -> Element {
    rsx! { label { "{label}", input { value: "{value()}", placeholder: "{label}", oninput: move |event| value.set(event.value()) } } }
}

#[component]
fn SelectField(label: &'static str, mut value: Signal<String>, options: Vec<String>) -> Element {
    rsx! { label { "{label}", select { value: "{value()}", oninput: move |event| value.set(event.value()), for option_value in options { option { value: "{option_value}", "{option_value}" } } } } }
}

#[component]
fn AssetOption(asset: Value) -> Element {
    let id = text(&asset, "asset_id");
    let name = text(&asset, "name");
    rsx! { option { value: "{id}", "{name}" } }
}

#[component]
fn SeriesLine(values: Vec<f64>, min: f64, max: f64, color: &'static str) -> Element {
    rsx! { polyline { points: "{chart_polyline(&values, min, max)}", stroke: "{color}", fill: "none", stroke_width: "2" } }
}

fn query_latest_holdings(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut holdings: Signal<Value>,
) {
    let url = server_url();
    spawn(async move {
        status.set("正在查询最新市场价…".into());
        match api::post(&url, "/api/portfolio/holdings/query", json!({})).await {
            Ok(value) => {
                holdings.set(value["holdings"].clone());
                status.set(format!(
                    "查询完成，已保存历史结果：{}",
                    value["snapshot"]["queried_at"].as_str().unwrap_or("")
                ));
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
        match api::get(&url, "/api/portfolio/assets").await {
            Ok(value) => {
                assets.set(value.as_array().cloned().unwrap_or_default());
                status.set("资产列表已刷新".into());
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
        match api::get(&url, "/api/portfolio/transactions").await {
            Ok(value) => {
                transactions.set(value.as_array().cloned().unwrap_or_default());
                status.set("交易列表已刷新".into());
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
    mut selected: Signal<Option<String>>,
) {
    let url = server_url();
    spawn(async move {
        match api::post(
            &url,
            "/api/portfolio/assets",
            json!({"category":category,"market":market,"symbol":symbol,"name":name}),
        )
        .await
        {
            Ok(_) => {
                selected.set(None);
                refresh_assets(status, server_url, assets);
            }
            Err(error) => status.set(error),
        }
    });
}
fn update_asset(
    mut status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
    id: Option<String>,
    category: String,
    market: String,
    symbol: String,
    name: String,
    mut selected: Signal<Option<String>>,
) {
    let url = server_url();
    spawn(async move {
        let Some(id) = id else {
            status.set("请先选择一个资产".into());
            return;
        };
        match api::put(
            &url,
            &format!("/api/portfolio/assets/{id}"),
            json!({"category":category,"market":market,"symbol":symbol,"name":name}),
        )
        .await
        {
            Ok(_) => {
                selected.set(None);
                refresh_assets(status, server_url, assets);
            }
            Err(error) => status.set(error),
        }
    });
}
fn delete_asset(
    mut status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
    id: Option<String>,
    mut selected: Signal<Option<String>>,
) {
    let url = server_url();
    spawn(async move {
        let Some(id) = id else {
            status.set("请先选择一个资产".into());
            return;
        };
        match api::delete(&url, &format!("/api/portfolio/assets/{id}")).await {
            Ok(_) => {
                selected.set(None);
                refresh_assets(status, server_url, assets);
            }
            Err(error) => status.set(error),
        }
    });
}
fn transaction_body(
    asset_id: String,
    kind: String,
    amount: String,
    price: String,
    date: String,
) -> Result<Value, String> {
    if asset_id.trim().is_empty() {
        return Err("请选择资产名称".into());
    }
    Ok(
        json!({"asset_id":asset_id,"type":kind,"amount":amount.parse::<f64>().map_err(|_| "数量必须是数字")?,"price":price.parse::<f64>().map_err(|_| "价格必须是数字")?,"date":date}),
    )
}
fn create_transaction(
    mut status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
    asset_id: String,
    kind: String,
    amount: String,
    price: String,
    date: String,
    mut selected: Signal<Option<i64>>,
) {
    let body = match transaction_body(asset_id, kind, amount, price, date) {
        Ok(value) => value,
        Err(error) => {
            status.set(error);
            return;
        }
    };
    let url = server_url();
    spawn(async move {
        match api::post(&url, "/api/portfolio/transactions", body).await {
            Ok(_) => {
                selected.set(None);
                refresh_transactions(status, server_url, transactions);
            }
            Err(error) => status.set(error),
        }
    });
}
fn update_transaction(
    mut status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
    id: Option<i64>,
    asset_id: String,
    kind: String,
    amount: String,
    price: String,
    date: String,
    mut selected: Signal<Option<i64>>,
) {
    let Some(id) = id else {
        status.set("请先选择一笔交易".into());
        return;
    };
    let body = match transaction_body(asset_id, kind, amount, price, date) {
        Ok(value) => value,
        Err(error) => {
            status.set(error);
            return;
        }
    };
    let url = server_url();
    spawn(async move {
        match api::put(&url, &format!("/api/portfolio/transactions/{id}"), body).await {
            Ok(_) => {
                selected.set(None);
                refresh_transactions(status, server_url, transactions);
            }
            Err(error) => status.set(error),
        }
    });
}
fn delete_transaction(
    mut status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
    id: Option<i64>,
    mut selected: Signal<Option<i64>>,
) {
    let url = server_url();
    spawn(async move {
        let Some(id) = id else {
            status.set("请先选择一笔交易".into());
            return;
        };
        match api::delete(&url, &format!("/api/portfolio/transactions/{id}")).await {
            Ok(_) => {
                selected.set(None);
                refresh_transactions(status, server_url, transactions);
            }
            Err(error) => status.set(error),
        }
    });
}
fn refresh_holding_snapshots(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut snapshots: Signal<Vec<Value>>,
) {
    let url = server_url();
    spawn(async move {
        match api::get(&url, "/api/portfolio/holding-snapshots").await {
            Ok(value) => {
                snapshots.set(value.as_array().cloned().unwrap_or_default());
                status.set("历史结果已刷新".into());
            }
            Err(error) => status.set(error),
        }
    });
}
fn load_holding_snapshot(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut selected: Signal<Value>,
    id: i64,
) {
    let url = server_url();
    spawn(async move {
        match api::get(&url, &format!("/api/portfolio/holding-snapshots/{id}")).await {
            Ok(value) => {
                selected.set(value);
                status.set("已加载历史持仓结果".into());
            }
            Err(error) => status.set(error),
        }
    });
}
fn refresh_chart(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut chart: Signal<Value>,
    range: String,
    custom_from: String,
    custom_to: String,
) {
    let (from, to) = if range == "custom" {
        (custom_from, custom_to)
    } else if range == "all" {
        (String::new(), String::new())
    } else {
        let days = range.parse::<i64>().unwrap_or(30);
        (
            (Utc::now() - Duration::days(days))
                .format("%Y-%m-%d")
                .to_string(),
            Utc::now().format("%Y-%m-%d").to_string(),
        )
    };
    let url = server_url();
    spawn(async move {
        let path = format!(
            "/api/portfolio/profit-history?from={}&to={}",
            urlencoding::encode(&from),
            urlencoding::encode(&to)
        );
        match api::get(&url, &path).await {
            Ok(value) => {
                let count = value["labels"].as_array().map_or(0, Vec::len);
                chart.set(value);
                status.set(format!("收益走势已加载：{count} 个时间点"));
            }
            Err(error) => status.set(error),
        }
    });
}
fn chart_polyline(values: &[f64], min: f64, max: f64) -> String {
    let range = (max - min).max(1.0);
    let denominator = values.len().saturating_sub(1).max(1) as f64;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            format!(
                "{:.1},{:.1}",
                20.0 + index as f64 / denominator * 860.0,
                20.0 + (max - value) / range * 280.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn series_values(chart: &Value, label: &str) -> Vec<f64> {
    chart["series"][label]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|point| point.get(1).and_then(Value::as_f64))
        .collect()
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("-").to_owned()
}
fn asset_category_code(value: &Value) -> String {
    text(value, "asset_id")
        .split(':')
        .next()
        .filter(|kind| matches!(*kind, "crypto" | "fund" | "stock"))
        .unwrap_or("crypto")
        .to_owned()
}
fn datetime_local(value: &str) -> String {
    value.replace(' ', "T").chars().take(16).collect()
}
fn number(value: &Value, key: &str) -> String {
    value[key]
        .as_f64()
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "-".into())
}
