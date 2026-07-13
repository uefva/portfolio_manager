//! 应用外壳与共享的客户端服务状态。
//!
//! 本模块是 Dioxus 组件树的根节点，负责：
//!   1. 顶部栏：应用标题 + 服务端地址输入框
//!   2. 标签页导航：持仓 / 资产管理 / 交易记录 / 历史快照 / 收益走势
//!   3. 底部状态栏：显示操作结果或错误消息
//!   4. 启动时自动从服务端加载全部数据
//!   5. 服务端地址变更时自动重新加载

use crate::{
    api,
    views::{Assets, Chart, Holdings, Snapshots, Transactions},
};
use dioxus::prelude::*;
use serde_json::{json, Value};

// ── 标签页名称常量 ──
const TAB_HOLDINGS: &str = "持仓";
const TAB_ASSETS: &str = "资产管理";
const TAB_TRANSACTIONS: &str = "交易记录";
const TAB_SNAPSHOTS: &str = "历史持仓结果";
const TAB_CHART: &str = "收益走势";

/// 根组件。
///
/// 页面组件通过 Dioxus Signal 共享数据：
///   - `holdings`、`assets`、`transactions`、`chart` 存储服务端返回的数据
///   - `status` 存储底部状态栏消息
///   - `server_url` 存储服务端地址（用户可在界面上修改）
///
/// 每次成功的服务端调用后数据信号会被更新，与 Python 旧版客户端的行为一致。
#[component]
pub fn App() -> Element {
    // ── 全局信号（跨标签页共享） ──
    let mut active_tab = use_signal(|| TAB_HOLDINGS.to_owned());
    let mut server_url = use_signal(api::default_server_url);
    let status = use_signal(|| "就绪：请点击刷新从服务端读取数据".to_owned());
    let holdings = use_signal(|| json!({}));
    let assets = use_signal(Vec::<Value>::new);
    let transactions = use_signal(Vec::<Value>::new);
    let chart = use_signal(|| json!({}));

    // ── 启动时自动加载 + 服务端地址变更时重新加载 ──
    // Python 旧版桌面客户端在启动时自动刷新服务端缓存。
    // 此处复刻该行为：use_effect 会在组件挂载时和 server_url 变化时触发，
    // 用户不需要手动点击每个页签的刷新按钮。
    use_effect(move || {
        let url = server_url();
        let mut status = status;
        let mut holdings = holdings;
        let mut assets = assets;
        let mut transactions = transactions;
        let mut chart = chart;
        spawn(async move {
            // 并行启动四个请求，减少等待时间
            status.set("正在联网查询最新市场价，并加载资产、交易和收益走势数据…".to_owned());
            let holdings_result = api::post(&url, "/api/portfolio/holdings/query", json!({})).await;
            let assets_result = api::get(&url, "/api/portfolio/assets").await;
            let transactions_result = api::get(&url, "/api/portfolio/transactions").await;
            let chart_result = api::get(&url, "/api/portfolio/profit-history").await;

            match (
                holdings_result,
                assets_result,
                transactions_result,
                chart_result,
            ) {
                (Ok(holdings_result), Ok(assets_data), Ok(transactions_data), Ok(chart_data)) => {
                    let holdings_data = holdings_result["holdings"].clone();
                    let holding_count = holdings_data["holdings"].as_array().map_or(0, Vec::len);
                    let assets_data = assets_data.as_array().cloned().unwrap_or_default();
                    let transactions_data =
                        transactions_data.as_array().cloned().unwrap_or_default();

                    // 输出加载日志到终端
                    println!(
                        "[client] startup load: holdings={}, assets={}, transactions={}",
                        holding_count,
                        assets_data.len(),
                        transactions_data.len()
                    );

                    // 更新各页签的数据信号
                    holdings.set(holdings_data);
                    assets.set(assets_data);
                    transactions.set(transactions_data);
                    chart.set(chart_data);

                    status.set(format!("自动加载完成：{holding_count} 项持仓"));
                }
                // 任一请求失败则显示错误
                (Err(error), _, _, _)
                | (_, Err(error), _, _)
                | (_, _, Err(error), _)
                | (_, _, _, Err(error)) => {
                    println!("[client] startup load failed: {error}");
                    status.set(error);
                }
            }
        });
    });

    // ── 渲染 ──
    rsx! {
        // 内嵌样式表，Desktop 构建不依赖 Dioxus CLI 复制静态资源。
        // 同一份 CSS 在 Web 构建中也同样有效。
        style { {include_str!("../assets/style.css")} }
        main { class: "app",
            // 顶部栏：应用标题 + 服务端地址输入
            header {
                div { h1 { "多资产持仓管理" } p { "服务端优先模式" } }
                label { class: "server-url",
                    "服务端",
                    input {
                        value: "{server_url()}",
                        // 每次按键都更新信号，但不自动提交 — 用户需按回车或触发刷新
                        oninput: move |event| server_url.set(event.value()),
                    }
                }
            }

            // 标签页导航
            nav { class: "tabs",
                for label in [TAB_HOLDINGS, TAB_ASSETS, TAB_TRANSACTIONS, TAB_SNAPSHOTS, TAB_CHART] {
                    button {
                        class: if active_tab() == label { "active" } else { "" },
                        onclick: move |_| { active_tab.set(label.to_owned()); },
                        "{label}"
                    }
                }
            }

            // 当前标签页内容
            section { class: "panel",
                if active_tab() == TAB_HOLDINGS      { Holdings     { status, server_url, holdings } }
                else if active_tab() == TAB_ASSETS        { Assets       { status, server_url, assets } }
                else if active_tab() == TAB_TRANSACTIONS  { Transactions { status, server_url, transactions, assets } }
                else if active_tab() == TAB_SNAPSHOTS     { Snapshots    { status, server_url } }
                else                                       { Chart        { status, server_url, chart } }
            }

            // 底部状态栏：显示最近一次操作的结果或错误
            footer { class: "status", "{status()}" }
        }
    }
}
