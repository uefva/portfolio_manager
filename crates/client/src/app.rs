//! Application shell and shared client-side service state.

use crate::{
    api,
    views::{Assets, Chart, Holdings, Snapshots, Transactions},
};
use dioxus::prelude::*;
use serde_json::{json, Value};

const TAB_HOLDINGS: &str = "持仓";
const TAB_ASSETS: &str = "资产管理";
const TAB_TRANSACTIONS: &str = "交易记录";
const TAB_SNAPSHOTS: &str = "历史持仓结果";
const TAB_CHART: &str = "收益走势";

/// Root component. Page components share these signals and update them after
/// successful service calls, matching the original Python client's online flow.
#[component]
pub fn App() -> Element {
    let mut active_tab = use_signal(|| TAB_HOLDINGS.to_owned());
    let mut server_url = use_signal(api::default_server_url);
    let status = use_signal(|| "就绪：请点击刷新从服务端读取数据".to_owned());
    let holdings = use_signal(|| json!({}));
    let assets = use_signal(Vec::<Value>::new);
    let transactions = use_signal(Vec::<Value>::new);
    let chart = use_signal(|| json!({}));

    // The original Python desktop app refreshes its service-backed cache during
    // startup. Mirror that behaviour here and repeat it when the server URL is
    // changed, so users do not need to click each page's refresh button first.
    use_effect(move || {
        let url = server_url();
        let mut status = status;
        let mut holdings = holdings;
        let mut assets = assets;
        let mut transactions = transactions;
        let mut chart = chart;
        spawn(async move {
            status.set("正在自动加载持仓、资产、交易和收益走势数据…".to_owned());
            let holdings_result = api::get(&url, "/api/portfolio/holdings?category=全部").await;
            let assets_result = api::get(&url, "/api/portfolio/assets").await;
            let transactions_result = api::get(&url, "/api/portfolio/transactions").await;
            let chart_result = api::get(&url, "/api/portfolio/profit-history").await;

            match (
                holdings_result,
                assets_result,
                transactions_result,
                chart_result,
            ) {
                (Ok(holdings_data), Ok(assets_data), Ok(transactions_data), Ok(chart_data)) => {
                    let holding_count = holdings_data["holdings"].as_array().map_or(0, Vec::len);
                    let assets_data = assets_data.as_array().cloned().unwrap_or_default();
                    let transactions_data =
                        transactions_data.as_array().cloned().unwrap_or_default();
                    println!("[client] startup load: holdings={holding_count}, assets={}, transactions={}", assets_data.len(), transactions_data.len());
                    holdings.set(holdings_data);
                    assets.set(assets_data);
                    transactions.set(transactions_data);
                    chart.set(chart_data);
                    status.set(format!("自动加载完成：{holding_count} 项持仓"));
                }
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

    rsx! {
        // Embed the stylesheet so `cargo run` Desktop builds do not depend on
        // Dioxus CLI asset copying. The same CSS also works in Web builds.
        style { {include_str!("../assets/style.css")} }
        main { class: "app",
            header {
                div { h1 { "多资产持仓管理" } p { "服务端优先模式" } }
                label { class: "server-url", "服务端", input {
                    value: "{server_url()}",
                    oninput: move |event| server_url.set(event.value()),
                } }
            }
            nav { class: "tabs",
                for label in [TAB_HOLDINGS, TAB_ASSETS, TAB_TRANSACTIONS, TAB_SNAPSHOTS, TAB_CHART] {
                    button {
                        class: if active_tab() == label { "active" } else { "" },
                        onclick: move |_| { active_tab.set(label.to_owned()); },
                        "{label}"
                    }
                }
            }
            section { class: "panel",
                if active_tab() == TAB_HOLDINGS { Holdings { status, server_url, holdings } }
                else if active_tab() == TAB_ASSETS { Assets { status, server_url, assets } }
                else if active_tab() == TAB_TRANSACTIONS { Transactions { status, server_url, transactions } }
                else if active_tab() == TAB_SNAPSHOTS { Snapshots { status, server_url } }
                else { Chart { status, server_url, chart } }
            }
            footer { class: "status", "{status()}" }
        }
    }
}
