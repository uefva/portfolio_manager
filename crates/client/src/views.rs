//! 基于 Rust HTTP 服务端的交互式 Dioxus 页面。
//!
//! 每个组件接收来自 App 根组件的 Signal，与服务端交互并更新 UI。
//! 各组件都是自包含的：查询 → 显示 → 用户操作 → 刷新。

use crate::api;
use dioxus::prelude::*;
use serde_json::{json, Value};

// ═══════════════════════════════════════════════════════════════
// 持仓页（首页）
// ═══════════════════════════════════════════════════════════════

/// 加载并显示服务端计算好的持仓快照。
///
/// 数据来源：GET /api/portfolio/holdings?category=全部
/// 显示字段：类别、市场、代码、名称、数量、当前价、人民币市值、人民币收益
#[component]
pub fn Holdings(
    status: Signal<String>,
    server_url: Signal<String>,
    holdings: Signal<Value>,
) -> Element {
    // 从 Signal 中提取持仓列表和总收益
    let rows = holdings()
        .get("holdings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total_profit = format!("{:.2}", holdings()["total_profit"].as_f64().unwrap_or(0.0));

    rsx! {
        div { class: "toolbar",
            h2 { "持仓" }
            button { onclick: move |_| refresh_holdings(status, server_url, holdings), "查询并保存" }
        }
        // 汇总卡片：显示当前总收益
        div { class: "summary-card", "当前总收益：{total_profit} CNY" }
        table { class: "data-table",
            thead { tr {
                th { "类别" } th { "市场" } th { "代码" } th { "名称" }
                th { "数量" } th { "当前价" } th { "持仓价值(CNY)" } th { "收益(CNY)" }
            }}
            tbody {
                for item in rows {
                    tr {
                        td { {text(&item, "category")} }
                        td { {text(&item, "market")} }
                        td { {text(&item, "symbol")} }
                        td { {text(&item, "name")} }
                        td { {number(&item, "quantity")} }
                        td { {number(&item, "price")} }
                        td { {number(&item, "value_cny")} }
                        td { {number(&item, "profit_cny")} }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 资产管理页
// ═══════════════════════════════════════════════════════════════

/// 资产目录管理：新增资产 + 查看所有已注册资产。
///
/// 新增通过 POST /api/portfolio/assets，然后重新加载列表。
/// 资产 ID 由服务端根据类别、市场、代码自动生成（格式：类型:市场:代码）。
#[component]
pub fn Assets(
    status: Signal<String>,
    server_url: Signal<String>,
    assets: Signal<Vec<Value>>,
) -> Element {
    // 表单字段信号，默认值适配加密货币场景
    let category = use_signal(|| "加密货币".to_owned());
    let market = use_signal(|| "CRYPTO".to_owned());
    let symbol = use_signal(String::new);
    let name = use_signal(String::new);
    let rows = assets();

    rsx! {
        section { class: "form",
            h2 { "资产管理" }
            div { class: "fields",
                TextField { label: "类别", value: category }
                TextField { label: "市场", value: market }
                TextField { label: "代码", value: symbol }
                TextField { label: "名称", value: name }
            }
            div { class: "actions",
                button {
                    onclick: move |_| create_asset(
                        status, server_url, assets,
                        category(), market(), symbol(), name(),
                    ),
                    "新增资产"
                }
                button {
                    onclick: move |_| refresh_assets(status, server_url, assets),
                    "刷新资产"
                }
            }
        }
        // 资产列表表格
        table { class: "data-table",
            thead { tr { th { "类别" } th { "市场" } th { "代码" } th { "名称" } th { "币种" } } }
            tbody {
                for item in rows {
                    tr {
                        td { {text(&item, "category")} }
                        td { {text(&item, "market")} }
                        td { {text(&item, "symbol")} }
                        td { {text(&item, "name")} }
                        td { {text(&item, "currency")} }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 交易记录页
// ═══════════════════════════════════════════════════════════════

/// 交易记录管理：新增买入/卖出 + 查看所有交易。
///
/// 新增通过 POST /api/portfolio/transactions，然后重新加载列表。
/// 如果请求中未提供 asset_id，服务端会根据表单字段自动创建资产。
/// 卖出时服务端会校验是否超过当前持仓量。
#[component]
pub fn Transactions(
    status: Signal<String>,
    server_url: Signal<String>,
    transactions: Signal<Vec<Value>>,
) -> Element {
    // 表单字段信号
    let category = use_signal(|| "加密货币".to_owned());
    let market = use_signal(|| "CRYPTO".to_owned());
    let symbol = use_signal(String::new);
    let name = use_signal(String::new);
    let transaction_type = use_signal(|| "buy".to_owned());  // 默认买入
    let amount = use_signal(String::new);   // 数量（字符串输入，提交时解析为 f64）
    let price = use_signal(String::new);    // 单价
    let date = use_signal(String::new);     // 交易日期（留空则使用当前时间）
    let rows = transactions();

    rsx! {
        section { class: "form",
            h2 { "新增交易" }
            div { class: "fields",
                TextField { label: "类别", value: category }
                TextField { label: "市场", value: market }
                TextField { label: "代码", value: symbol }
                TextField { label: "名称", value: name }
                TextField { label: "类型（buy/sell）", value: transaction_type }
                TextField { label: "数量", value: amount }
                TextField { label: "价格", value: price }
                TextField { label: "日期", value: date }
            }
            div { class: "actions",
                button {
                    onclick: move |_| create_transaction(
                        status, server_url, transactions,
                        category(), market(), symbol(), name(),
                        transaction_type(), amount(), price(), date(),
                    ),
                    "新增交易"
                }
                button {
                    onclick: move |_| refresh_transactions(status, server_url, transactions),
                    "刷新交易"
                }
            }
        }
        // 交易列表表格
        table { class: "data-table",
            thead { tr { th { "类型" } th { "代码" } th { "日期" } th { "数量" } th { "价格" } th { "成交额" } } }
            tbody {
                for item in rows {
                    tr {
                        td { {text(&item, "type")} }
                        td { {text(&item, "symbol")} }
                        td { {text(&item, "date")} }
                        td { {number(&item, "amount")} }
                        td { {number(&item, "price")} }
                        td { {number(&item, "total")} }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 历史快照页
// ═══════════════════════════════════════════════════════════════

/// 历史持仓结果页（占位）。
///
/// 当前仅提供导出按钮以验证服务端连通性。
/// Desktop 快照文件与 Web 导入导出功能将在后续迭代中加入。
#[component]
pub fn Snapshots(status: Signal<String>, server_url: Signal<String>) -> Element {
    rsx! {
        div { class: "toolbar",
            h2 { "历史持仓结果" }
            button { onclick: move |_| export_portfolio(status, server_url), "导出服务端数据" }
        }
        p { "Desktop 快照文件与 Web 导入导出将在下一步接入。" }
    }
}

// ═══════════════════════════════════════════════════════════════
// 收益走势页
// ═══════════════════════════════════════════════════════════════

/// 收益走势图表页。
///
/// 从服务端 GET /api/portfolio/profit-history 获取时序数据，
/// 使用内嵌 SVG polyline 绘制简易折线图（避免引入重量级图表库）。
#[component]
pub fn Chart(
    status: Signal<String>,
    server_url: Signal<String>,
    chart: Signal<Value>,
) -> Element {
    // 提取总资产系列的数据点
    let labels = chart()["labels"].as_array().cloned().unwrap_or_default();
    let points = chart()["series"]["总资产"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // 每个点格式为 [index, value]，取第二个元素作为 Y 值
    let values = points
        .iter()
        .filter_map(|point| point.get(1).and_then(Value::as_f64))
        .collect::<Vec<_>>();

    // 将数值序列转为 SVG polyline 的 points 属性字符串
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
        div { class: "toolbar",
            h2 { "收益走势" }
            button { onclick: move |_| refresh_chart(status, server_url, chart), "刷新图表" }
        }
        div { class: "chart",
            if values.is_empty() {
                "暂无收益走势数据。请确认服务端已有历史价格记录。"
            } else {
                // 显示统计信息
                div { class: "chart-meta",
                    "{labels.len()} 个时间点 · 最低 {minimum} · 最高 {maximum}"
                }
                // 简易 SVG 折线图
                svg {
                    class: "profit-svg",
                    view_box: "0 0 900 320",
                    preserve_aspect_ratio: "none",
                    // X 轴基线
                    line { x1: "20", y1: "300", x2: "880", y2: "300", class: "chart-axis" }
                    // 折线
                    polyline { points: "{polyline}", class: "profit-line" }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 可复用组件
// ═══════════════════════════════════════════════════════════════

/// 受控文本输入框组件。
///
/// 每次按键都更新 Signal，实现即时双向绑定。
/// 相当于 `<label>{label}<input value={signal} oninput={...} /></label>`
#[component]
fn TextField(label: &'static str, mut value: Signal<String>) -> Element {
    rsx! {
        label {
            "{label}",
            input {
                value: "{value()}",
                placeholder: "{label}",
                oninput: move |event| value.set(event.value()),
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 数据刷新函数（异步，在 spawn 中运行）
// ═══════════════════════════════════════════════════════════════

/// 从服务端刷新持仓数据。
///
/// 流程：设置 "加载中" 状态 → GET 请求 → 成功则更新信号 → 失败则显示错误。
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

/// 从服务端刷新资产列表。
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

/// 从服务端刷新交易记录列表。
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

/// 创建新资产：POST 到服务端，成功后自动刷新资产列表。
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
        // 前端校验：代码不能为空
        if symbol.trim().is_empty() {
            status.set("资产代码不能为空".into());
            return;
        }
        status.set("正在保存资产…".into());
        match api::post(
            &url,
            "/api/portfolio/assets",
            json!({
                "category": category,
                "market":   market,
                "symbol":   symbol,
                "name":     name,
            }),
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

/// 创建新交易：POST 到服务端，成功后自动刷新交易列表。
///
/// 前端校验：
///   - amount 必须是有效数字
///   - price 必须是有效数字
/// 服务端校验：
///   - amount > 0, price >= 0
///   - 卖出时持仓量足够
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
        // 数量解析与校验
        let amount = match amount.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                status.set("数量必须是数字".into());
                return;
            }
        };
        // 价格解析与校验
        let price = match price.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                status.set("价格必须是数字".into());
                return;
            }
        };
        status.set("正在保存交易…".into());
        match api::post(
            &url,
            "/api/portfolio/transactions",
            json!({
                "category": category,
                "market":   market,
                "symbol":   symbol,
                "name":     name,
                "type":     transaction_type,
                "amount":   amount,
                "price":    price,
                "date":     date,
            }),
        )
        .await
        {
            Ok(_) => {
                status.set("交易已保存，正在刷新列表…".into());
                refresh_transactions(status, server_url, transactions)
            }
            Err(error) => status.set(error),
        }
    });
}

/// 请求服务端导出投资组合数据（JSON 格式）。
///
/// 当前只验证连通性并返回资产数量，实际文件下载功能待后续接入。
fn export_portfolio(mut status: Signal<String>, server_url: Signal<String>) {
    let url = server_url();
    spawn(async move {
        status.set("正在导出服务端数据…".into());
        match api::get(&url, "/api/portfolio/export").await {
            Ok(value) => {
                let count = value["assets"]
                    .as_object()
                    .map_or(0, |assets| assets.len());
                status.set(format!(
                    "服务端导出成功：{count} 项资产（下载文件功能待接入）"
                ));
            }
            Err(error) => status.set(error),
        }
    });
}

/// 从服务端刷新收益走势数据。
fn refresh_chart(
    mut status: Signal<String>,
    server_url: Signal<String>,
    mut chart: Signal<Value>,
) {
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

// ═══════════════════════════════════════════════════════════════
// 图表渲染辅助函数
// ═══════════════════════════════════════════════════════════════

/// 将一维数值序列缩放并映射为 SVG polyline 的 points 字符串。
///
/// 不添加重量级图表依赖，直接生成折线图的 SVG 坐标点。
///
/// # 参数
/// * `values` - Y 轴数值序列
/// * `width`  - 绘图区宽度（像素）
/// * `height` - 绘图区高度（像素）
///
/// # 返回
/// 格式为 `"x1,y1 x2,y2 ..."` 的字符串，可直接作为 `<polyline points="...">` 的属性值。
fn chart_polyline(values: &[f64], width: f64, height: f64) -> String {
    if values.is_empty() {
        return String::new();
    }
    // 计算 Y 轴范围
    let minimum = values.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let maximum = values.iter().copied().reduce(f64::max).unwrap_or(0.0);
    let range = (maximum - minimum).max(1.0);  // 防止除以零

    // X 轴步长 = 总宽度 / (点数 - 1)
    let denominator = values.len().saturating_sub(1).max(1) as f64;

    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            // X = 左边距(20) + 索引比例 × 宽度
            let x = 20.0 + index as f64 / denominator * width;
            // Y = 上边距(20) + (最大值-当前值)/范围 × 高度（SVG Y 轴向下）
            let y = 20.0 + (maximum - value) / range * height;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 安全地从 JSON Value 中提取字符串字段，默认返回 "-"。
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("-").to_owned()
}

/// 安全地从 JSON Value 中提取数值字段，格式化为两位小数，默认返回 "-"。
fn number(value: &Value, key: &str) -> String {
    value[key]
        .as_f64()
        .map(|number| format!("{number:.2}"))
        .unwrap_or_else(|| "-".into())
}
