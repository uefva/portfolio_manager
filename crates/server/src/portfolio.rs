//! Portfolio persistence, validation, holdings calculation, and JSON migration.

use crate::utils::now_text;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, path::Path};

/// Return the complete asset catalogue. Empty assets are intentionally included.
pub fn list_assets(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare("SELECT asset_id,category,market,symbol,name,currency,created_at,updated_at FROM portfolio_assets ORDER BY category,market,symbol")?;
    let assets = statement.query_map([], |row| Ok(json!({
        "asset_id": row.get::<_, String>(0)?, "category": row.get::<_, String>(1)?,
        "market": row.get::<_, String>(2)?, "symbol": row.get::<_, String>(3)?,
        "name": row.get::<_, String>(4)?, "currency": row.get::<_, String>(5)?,
        "created_at": row.get::<_, String>(6)?, "updated_at": row.get::<_, String>(7)?, "transactions": [],
    })))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": assets}))
}

/// Insert or rename an asset. Assets with transactions cannot change identity.
pub fn save_asset(database: &Path, previous_id: Option<String>, input: &Value) -> Result<Value> {
    let category = input["category"].as_str().unwrap_or("加密货币");
    let market = input["market"].as_str().unwrap_or("CRYPTO");
    let symbol = input["symbol"].as_str().unwrap_or("").trim().to_uppercase();
    anyhow::ensure!(!symbol.is_empty(), "symbol required");
    let asset_id = format!("{}:{}:{}", asset_kind(category), market, symbol);
    let name = input["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&symbol);
    let currency = currency_for(category, market);
    let connection = Connection::open(database)?;
    if let Some(previous_id) = previous_id {
        let transaction_count: i64 = connection.query_row(
            "SELECT count(*) FROM portfolio_transactions WHERE asset_id=?",
            [&previous_id],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            transaction_count == 0 || previous_id == asset_id,
            "asset code cannot change after transaction"
        );
        connection.execute(
            "DELETE FROM portfolio_assets WHERE asset_id=?",
            [previous_id],
        )?;
    }
    connection.execute(
        "INSERT INTO portfolio_assets VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(asset_id) DO UPDATE SET name=excluded.name,updated_at=excluded.updated_at",
        params![asset_id, category, market, symbol, name, currency, now_text(), now_text()],
    )?;
    Ok(
        json!({"asset_id": asset_id, "category": category, "market": market, "symbol": symbol, "name": name, "currency": currency}),
    )
}

/// Delete only empty assets; preserving transaction history is deliberate.
pub fn delete_asset(database: &Path, asset_id: &str) -> Result<Value> {
    let connection = Connection::open(database)?;
    let transaction_count: i64 = connection.query_row(
        "SELECT count(*) FROM portfolio_transactions WHERE asset_id=?",
        [asset_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(transaction_count == 0, "asset has transactions");
    connection.execute("DELETE FROM portfolio_assets WHERE asset_id=?", [asset_id])?;
    Ok(json!({"data":{"deleted":true}}))
}

pub fn list_transactions(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare("SELECT t.id,t.asset_id,t.type,t.date,t.amount,t.price,t.total,t.currency,a.category,a.market,a.symbol,a.name FROM portfolio_transactions t JOIN portfolio_assets a ON a.asset_id=t.asset_id ORDER BY t.date,t.id")?;
    let transactions = statement.query_map([], |row| Ok(json!({
        "id": row.get::<_, i64>(0)?, "asset_id": row.get::<_, String>(1)?, "type": row.get::<_, String>(2)?,
        "date": row.get::<_, String>(3)?, "amount": row.get::<_, f64>(4)?, "price": row.get::<_, f64>(5)?,
        "total": row.get::<_, f64>(6)?, "currency": row.get::<_, String>(7)?, "category": row.get::<_, String>(8)?,
        "market": row.get::<_, String>(9)?, "symbol": row.get::<_, String>(10)?, "name": row.get::<_, String>(11)?,
    })))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": transactions}))
}

/// Validate a transaction before persisting. A sell cannot exceed current units.
pub fn save_transaction(
    database: &Path,
    transaction_id: Option<i64>,
    input: &Value,
) -> Result<Value> {
    let asset_id = match input["asset_id"].as_str() {
        Some(id) => id.to_owned(),
        None => save_asset(database, None, input)?["asset_id"]
            .as_str()
            .expect("asset id created")
            .to_owned(),
    };
    let transaction_type = input["type"].as_str().unwrap_or("buy");
    let amount = input["amount"].as_f64().context("amount required")?;
    let price = input["price"].as_f64().context("price required")?;
    anyhow::ensure!(amount > 0.0 && price >= 0.0, "invalid amount or price");
    let connection = Connection::open(database)?;
    if transaction_type == "sell" {
        let units: f64 = connection.query_row("SELECT coalesce(sum(CASE WHEN type='buy' THEN amount ELSE -amount END),0) FROM portfolio_transactions WHERE asset_id=?", [&asset_id], |row| row.get(0))?;
        anyhow::ensure!(units >= amount, "cannot sell more than holding");
    }
    let currency: String = connection.query_row(
        "SELECT currency FROM portfolio_assets WHERE asset_id=?",
        [&asset_id],
        |row| row.get(0),
    )?;
    let date = input["date"].as_str().unwrap_or(&now_text()).to_owned();
    let total = amount * price;
    let id = if let Some(id) = transaction_id {
        connection.execute("UPDATE portfolio_transactions SET type=?,date=?,amount=?,price=?,total=?,updated_at=? WHERE id=?", params![transaction_type,date,amount,price,total,now_text(),id])?;
        id
    } else {
        connection.execute("INSERT INTO portfolio_transactions(asset_id,type,date,amount,price,total,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)", params![asset_id,transaction_type,date,amount,price,total,currency,now_text(),now_text()])?;
        connection.last_insert_rowid()
    };
    Ok(
        json!({"id":id,"asset_id":asset_id,"type":transaction_type,"date":date,"amount":amount,"price":price,"total":total,"currency":currency}),
    )
}

pub fn delete_transaction(database: &Path, id: i64) -> Result<Value> {
    Connection::open(database)?.execute("DELETE FROM portfolio_transactions WHERE id=?", [id])?;
    Ok(json!({"data":{"deleted":true}}))
}

/// Compute real-time CNY holdings using the latest quote for each active asset.
pub fn holdings(database: &Path, category_filter: &str) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare("SELECT a.asset_id,a.category,a.market,a.symbol,a.name,a.currency,coalesce(sum(CASE WHEN t.type='buy' THEN t.amount ELSE -t.amount END),0),coalesce(sum(CASE WHEN t.type='buy' THEN t.total ELSE -t.total END),0) FROM portfolio_assets a LEFT JOIN portfolio_transactions t ON t.asset_id=a.asset_id GROUP BY a.asset_id")?;
    let mut rows = Vec::new();
    let mut total_value = 0.0;
    let mut total_cost = 0.0;
    let mut category_totals = Map::new();
    for result in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })? {
        let (asset_id, category, market, symbol, name, currency, quantity, native_cost) = result?;
        if quantity <= 0.0 || (category_filter != "全部" && category_filter != category) {
            continue;
        }
        let quote: Option<(f64, f64)> = connection.query_row("SELECT price,fx_to_cny FROM asset_price_history WHERE asset_id=? ORDER BY fetched_at DESC LIMIT 1", [&asset_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        let (price, fx_to_cny) = quote.unwrap_or((0.0, 1.0));
        let value_cny = quantity * price * fx_to_cny;
        // This intentionally follows the legacy calculation: native cost is
        // converted using the current FX rate at query time.
        let cost_cny = native_cost * fx_to_cny;
        let profit_cny = value_cny - cost_cny;
        total_value += value_cny;
        total_cost += cost_cny;
        category_totals.insert(
            category.clone(),
            json!({"total_value":value_cny,"total_cost":cost_cny,"total_profit":profit_cny}),
        );
        rows.push(json!({"asset_id":asset_id,"category":category,"market":market,"symbol":symbol,"name":name,"quantity":quantity,"avg_cost":native_cost/quantity,"price":price,"currency":currency,"fx_to_cny":fx_to_cny,"value_cny":value_cny,"cost_cny":cost_cny,"profit_cny":profit_cny,"profit_rate":if cost_cny==0.0{0.0}else{profit_cny/cost_cny*100.0}}));
    }
    let total_profit = total_value - total_cost;
    Ok(
        json!({"holdings":rows,"total_value":total_value,"total_cost":total_cost,"total_profit":total_profit,"total_profit_rate":if total_cost==0.0{0.0}else{total_profit/total_cost*100.0},"category_totals":category_totals}),
    )
}

/// Export the version-2 JSON format consumed by the previous desktop client.
pub fn export_json(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut assets = Map::new();
    let mut statement = connection
        .prepare("SELECT asset_id,category,market,symbol,name,currency FROM portfolio_assets")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })? {
        let (asset_id, category, market, symbol, name, currency) = row?;
        let mut transactions = connection.prepare("SELECT type,date,amount,price,total FROM portfolio_transactions WHERE asset_id=? ORDER BY date,id")?;
        let transactions = transactions.query_map([&asset_id], |row| Ok(json!({"type":row.get::<_,String>(0)?,"date":row.get::<_,String>(1)?,"amount":row.get::<_,f64>(2)?,"price":row.get::<_,f64>(3)?,"total":row.get::<_,f64>(4)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        assets.insert(asset_id, json!({"category":category,"market":market,"symbol":symbol,"name":name,"currency":currency,"transactions":transactions}));
    }
    Ok(json!({"version":2,"assets":assets}))
}

/// Import is idempotent: matching asset/type/date/amount/price orders are skipped.
pub fn import_json(database: &Path, portfolio: Value) -> Result<Value> {
    let assets = portfolio["assets"]
        .as_object()
        .context("portfolio.assets required")?;
    let (mut imported_assets, mut imported_transactions, mut skipped_transactions) = (0, 0, 0);
    for asset in assets.values() {
        let saved_asset = save_asset(database, None, asset)?;
        imported_assets += 1;
        for transaction in asset["transactions"].as_array().into_iter().flatten() {
            let mut transaction = transaction.clone();
            transaction["asset_id"] = saved_asset["asset_id"].clone();
            let connection = Connection::open(database)?;
            let duplicate: i64 = connection.query_row("SELECT count(*) FROM portfolio_transactions WHERE asset_id=? AND type=? AND date=? AND amount=? AND price=?", params![transaction["asset_id"].as_str(),transaction["type"].as_str(),transaction["date"].as_str(),transaction["amount"].as_f64(),transaction["price"].as_f64()], |row| row.get(0))?;
            if duplicate == 0 {
                save_transaction(database, None, &transaction)?;
                imported_transactions += 1;
            } else {
                skipped_transactions += 1;
            }
        }
    }
    Ok(
        json!({"assets_imported":imported_assets,"assets_updated":imported_assets,"transactions_imported":imported_transactions,"transactions_skipped":skipped_transactions}),
    )
}

/// Build a time series from captured price points and transactions effective at
/// each timestamp. This mirrors the legacy rule: remaining native cost is
/// converted with the FX rate stored alongside that historical quote.
pub fn profit_history(database: &Path, metric: &str) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut timestamp_statement = connection
        .prepare("SELECT DISTINCT fetched_at FROM asset_price_history ORDER BY fetched_at")?;
    let timestamps = timestamp_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut labels = Vec::with_capacity(timestamps.len());
    let mut total_series = Vec::with_capacity(timestamps.len());
    let mut category_series: HashMap<String, Vec<Value>> = HashMap::new();

    for (index, timestamp) in timestamps.iter().enumerate() {
        let mut quote_statement = connection.prepare(
            "SELECT asset_id,category,price_cny,fx_to_cny FROM asset_price_history WHERE fetched_at=?",
        )?;
        let quotes = quote_statement.query_map([timestamp], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;

        let mut total_value = 0.0;
        let mut total_cost = 0.0;
        let mut category_totals: HashMap<String, (f64, f64)> = HashMap::new();
        for quote in quotes {
            let (asset_id, category, price_cny, fx_to_cny) = quote?;
            let (quantity, native_cost): (f64, f64) = connection.query_row(
                "SELECT coalesce(sum(CASE WHEN type='buy' THEN amount ELSE -amount END),0),coalesce(sum(CASE WHEN type='buy' THEN total ELSE -total END),0) FROM portfolio_transactions WHERE asset_id=? AND date<=?",
                params![asset_id, timestamp],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if quantity <= 0.0 {
                continue;
            }
            let value_cny = quantity * price_cny;
            let cost_cny = native_cost * fx_to_cny;
            total_value += value_cny;
            total_cost += cost_cny;
            let total = category_totals.entry(category).or_insert((0.0, 0.0));
            total.0 += value_cny;
            total.1 += cost_cny;
        }

        labels.push(timestamp.clone());
        total_series.push(json!([
            index,
            metric_value(total_value - total_cost, total_cost, metric)
        ]));
        for (category, (value, cost)) in category_totals {
            category_series
                .entry(category)
                .or_default()
                .push(json!([index, metric_value(value - cost, cost, metric)]));
        }
    }

    let mut series = Map::new();
    series.insert("总资产".to_owned(), Value::Array(total_series));
    for (category, points) in category_series {
        series.insert(category, Value::Array(points));
    }
    Ok(
        json!({"labels":labels,"series":series,"all_series":series.clone(),"series_meta":{},"metric":metric,"source":"server"}),
    )
}

fn metric_value(profit: f64, cost: f64, metric: &str) -> f64 {
    if metric == "收益率" {
        if cost == 0.0 {
            0.0
        } else {
            profit / cost * 100.0
        }
    } else {
        profit
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
fn currency_for(category: &str, market: &str) -> &'static str {
    if market == "HK" {
        "HKD"
    } else if market == "SH" || market == "SZ" || category == "基金" {
        "CNY"
    } else {
        "USD"
    }
}
