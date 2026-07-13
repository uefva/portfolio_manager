//! Client for the private market-data adapter and quote persistence.
use crate::{portfolio, utils::now_text};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("HTTP client configuration is valid")
}

async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    request
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                anyhow::anyhow!("行情请求超时（15 秒）")
            } else {
                anyhow::anyhow!("行情请求失败：{error}")
            }
        })?
        .error_for_status()
        .map_err(|error| anyhow::anyhow!("行情服务返回错误：{error}"))
}

pub async fn search(
    base_url: &str,
    token: Option<&str>,
    query: &str,
    category: Option<&str>,
    market: Option<&str>,
) -> Result<Value> {
    let mut request = client()
        .get(format!("{}/v1/search", base_url.trim_end_matches('/')))
        .query(&[("query", query)]);
    if let Some(category) = category.filter(|value| !value.is_empty()) {
        request = request.query(&[("category", category)]);
    }
    if let Some(market) = market.filter(|value| !value.is_empty()) {
        request = request.query(&[("market", market)]);
    }
    if let Some(token) = token {
        request = request.header("X-Adapter-Token", token);
    }
    let response = send(request).await?;
    Ok(response.json::<Value>().await?)
}

pub async fn refresh(
    database: &Path,
    base_url: &str,
    token: Option<&str>,
    requested_assets: Option<Value>,
) -> Result<Value> {
    let assets = requested_assets.unwrap_or(portfolio::list_assets(database)?["data"].clone());
    let assets = assets.as_array().context("assets 必须是数组")?.clone();
    let expected = assets
        .iter()
        .filter_map(|asset| asset["asset_id"].as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let mut request = client()
        .post(format!("{}/v1/quotes", base_url.trim_end_matches('/')))
        .json(&json!({"assets": assets}));
    if let Some(token) = token {
        request = request.header("X-Adapter-Token", token);
    }
    let response = send(request).await?;
    let response = response.json::<Value>().await?;
    let quotes = quote_items(&response["quotes"]);
    let mut stored_ids = Vec::new();
    for quote in quotes {
        stored_ids.push(store_quote(database, &quote)?);
    }
    let missing = expected
        .iter()
        .filter(|id| !stored_ids.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(missing.is_empty(), "未找到最新行情：{}", missing.join(", "));
    anyhow::ensure!(
        response["errors"].as_array().map_or(true, Vec::is_empty),
        "行情查询失败：{}",
        response["errors"]
    );
    Ok(json!({
        "data": {"requested": assets.len(), "stored": stored_ids.len(), "errors": response["errors"].clone()}
    }))
}

fn store_quote(database: &Path, quote: &Value) -> Result<String> {
    let category = required(quote, "category")?;
    let market = required(quote, "market")?;
    let symbol = required(quote, "symbol")?.to_uppercase();
    let asset_id = quote["asset_id"]
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{}:{}:{}", asset_kind(category), market, symbol));
    let name = quote["name"].as_str().unwrap_or(&symbol);
    let currency = quote["currency"].as_str().unwrap_or("CNY");
    let price = quote["price"].as_f64().context("quote.price 必须为数字")?;
    let fx_to_cny =
        quote["fx_to_cny"]
            .as_f64()
            .unwrap_or_else(|| if currency == "CNY" { 1.0 } else { 1.0 });
    let price_cny = quote["price_cny"].as_f64().unwrap_or(price * fx_to_cny);
    let source = quote["source"].as_str().unwrap_or("adapter");
    let fetched_at = quote["fetched_at"].as_str().unwrap_or_else(|| "");
    let fetched_at = if fetched_at.is_empty() {
        now_text()
    } else {
        fetched_at.to_owned()
    };
    Connection::open(database)?.execute(
        "INSERT INTO asset_price_history \
         (asset_id,category,market,symbol,name,currency,price,fx_to_cny,price_cny,source,fetched_at) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(asset_id,fetched_at) DO UPDATE SET price=excluded.price, fx_to_cny=excluded.fx_to_cny, price_cny=excluded.price_cny, source=excluded.source",
        params![asset_id, category, market, symbol, name, currency, price, fx_to_cny, price_cny, source, fetched_at],
    )?;
    Ok(asset_id)
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|item| !item.is_empty())
        .context(format!("quote.{key} 不能为空"))
}

fn quote_items(value: &Value) -> Vec<Value> {
    if let Some(items) = value.as_array() {
        items.clone()
    } else if let Some(items) = value.as_object() {
        items.values().cloned().collect()
    } else {
        Vec::new()
    }
}

fn asset_kind(category: &str) -> &'static str {
    if category == "加密货币" {
        "crypto"
    } else if category == "基金" {
        "fund"
    } else {
        "stock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use std::fs;

    #[test]
    fn quote_is_persisted_with_a_generated_asset_id() {
        let path = std::env::temp_dir().join(format!(
            "portfolio-adapter-test-{}-{}.sqlite3",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::File::create(&path).unwrap();
        database::initialize(&path).unwrap();
        store_quote(&path, &json!({
            "category":"加密货币", "market":"CRYPTO", "symbol":"btc", "name":"Bitcoin",
            "currency":"USD", "price":100.0, "fx_to_cny":7.0, "source":"test", "fetched_at":"2026-01-01 00:00:00"
        })).unwrap();
        let count: i64 = Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM asset_price_history WHERE asset_id='crypto:CRYPTO:BTC'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn quote_items_accepts_adapter_map_response() {
        let items = quote_items(&json!({
            "crypto:CRYPTO:BTC": {"asset_id":"crypto:CRYPTO:BTC"},
            "fund:CN_FUND:006479": {"asset_id":"fund:CN_FUND:006479"}
        }));
        let ids = items
            .iter()
            .filter_map(|item| item["asset_id"].as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"crypto:CRYPTO:BTC"));
        assert!(ids.contains(&"fund:CN_FUND:006479"));
    }
}
