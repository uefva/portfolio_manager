//! Read-only price-history queries shared by the legacy price endpoints.

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::Path;

/// Return latest multi-asset quotes, optionally filtered by IDs and categories.
pub fn latest_assets(
    database: &Path,
    asset_ids: &[String],
    categories: &[String],
) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut prices = Map::new();
    let mut statement = connection.prepare(
        "SELECT p.asset_id,p.category,p.market,p.symbol,p.name,p.currency,p.price,p.fx_to_cny,p.price_cny,p.source,p.fetched_at FROM asset_price_history p JOIN (SELECT asset_id,max(fetched_at) fetched_at FROM asset_price_history GROUP BY asset_id) latest ON latest.asset_id=p.asset_id AND latest.fetched_at=p.fetched_at",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "asset_id": row.get::<_, String>(0)?, "category": row.get::<_, String>(1)?,
            "market": row.get::<_, String>(2)?, "symbol": row.get::<_, String>(3)?,
            "name": row.get::<_, String>(4)?, "currency": row.get::<_, String>(5)?,
            "price": row.get::<_, f64>(6)?, "fx_to_cny": row.get::<_, f64>(7)?,
            "price_cny": row.get::<_, f64>(8)?, "source": row.get::<_, String>(9)?,
            "fetched_at": row.get::<_, String>(10)?,
        }))
    })?;
    for row in rows {
        let quote = row?;
        let id = quote["asset_id"].as_str().expect("asset id selected");
        let category = quote["category"].as_str().expect("category selected");
        if (asset_ids.is_empty() || asset_ids.iter().any(|item| item == id))
            && (categories.is_empty() || categories.iter().any(|item| item == category))
        {
            prices.insert(id.to_owned(), quote);
        }
    }
    Ok(json!({"prices": prices}))
}

/// Return grouped historical points. `limit=0` preserves the legacy unlimited
/// behaviour; any other positive value counts timestamps rather than row count.
pub fn asset_history(
    database: &Path,
    asset_ids: &[String],
    limit: Option<String>,
    full: bool,
) -> Result<Value> {
    let connection = Connection::open(database)?;
    let timestamp_limit = limit
        .as_deref()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5000);
    let mut points: Map<String, Value> = Map::new();
    let mut statement = connection.prepare(
        "SELECT asset_id,fetched_at,price,fx_to_cny,price_cny,source FROM asset_price_history ORDER BY fetched_at,asset_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (asset_id, timestamp, price, fx_to_cny, price_cny, source) = row?;
        if !asset_ids.is_empty() && !asset_ids.contains(&asset_id) {
            continue;
        }
        if !points.contains_key(&timestamp)
            && timestamp_limit > 0
            && points.len() >= timestamp_limit
        {
            break;
        }
        let point = points.entry(timestamp.clone()).or_insert_with(|| json!({
            "timestamp": timestamp, "price_cny": {}, "prices": {}, "fx_to_cny": {}, "sources": {},
        }));
        point["price_cny"][&asset_id] = json!(price_cny);
        if full {
            point["prices"][&asset_id] = json!(price);
            point["fx_to_cny"][&asset_id] = json!(fx_to_cny);
            point["sources"][&asset_id] = json!(source);
        }
    }
    if !full {
        for point in points.values_mut() {
            let object = point.as_object_mut().expect("point is an object");
            object.remove("prices");
            object.remove("fx_to_cny");
            object.remove("sources");
        }
    }
    Ok(json!({"points": points.into_values().collect::<Vec<_>>() }))
}

/// Compatibility response for `/api/prices/latest`, which predates asset IDs.
pub fn latest_crypto_prices(database: &Path, symbols: &[String]) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut prices = Map::new();
    for symbol in symbols {
        let quote = connection.query_row(
            "SELECT price,source,fetched_at FROM asset_price_history WHERE market='CRYPTO' AND symbol=? ORDER BY fetched_at DESC LIMIT 1",
            [symbol], |row| Ok(json!({"price": row.get::<_, f64>(0)?, "source": row.get::<_, String>(1)?, "fetched_at": row.get::<_, String>(2)?})),
        ).optional()?;
        if let Some(quote) = quote {
            prices.insert(symbol.clone(), quote);
        }
    }
    Ok(json!({"prices": prices}))
}

use rusqlite::OptionalExtension;
