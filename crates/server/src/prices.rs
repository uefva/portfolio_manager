//! 只读历史价格查询，供旧版价格接口和资产历史接口复用。
//!
//! 本模块中的所有函数都是只读操作，不会修改数据库。
//! 数据由外部的行情适配器写入 asset_price_history 表。

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::Path;

/// 返回多资产的最新报价快照，可按 asset_id 和 category 过滤。
///
/// 使用子查询找到每个资产的最新 fetched_at，然后与主表 JOIN，
/// 一次性获取所有最新报价。比逐个资产查询效率更高。
///
/// # 参数
/// * `asset_ids` - 空 Vec 表示不过滤；非空则只返回匹配的资产
/// * `categories` - 空 Vec 表示不过滤；非空则只返回匹配的类别
///
/// # 返回
/// ```json
/// { "prices": { "crypto:CRYPTO:BTC": { "asset_id": "...", "price": 50000.0, ... }, ... } }
/// ```
pub fn latest_assets(
    database: &Path,
    asset_ids: &[String],
    categories: &[String],
) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut prices = Map::new();

    // 子查询 latest：获取每个 asset_id 的最新 fetched_at
    // 外层 JOIN：只取时间匹配的记录作为最新报价
    let mut statement = connection.prepare(
        "SELECT p.asset_id, p.category, p.market, p.symbol, p.name, p.currency, \
                p.price, p.fx_to_cny, p.price_cny, p.source, p.fetched_at \
         FROM asset_price_history p \
         JOIN ( \
           SELECT asset_id, max(fetched_at) AS fetched_at \
           FROM asset_price_history \
           GROUP BY asset_id \
         ) latest \
           ON latest.asset_id = p.asset_id \
           AND latest.fetched_at = p.fetched_at",
    )?;

    let rows = statement.query_map([], |row| {
        Ok(json!({
            "asset_id":   row.get::<_, String>(0)?,
            "category":   row.get::<_, String>(1)?,
            "market":     row.get::<_, String>(2)?,
            "symbol":     row.get::<_, String>(3)?,
            "name":       row.get::<_, String>(4)?,
            "currency":   row.get::<_, String>(5)?,
            "price":      row.get::<_, f64>(6)?,
            "fx_to_cny":  row.get::<_, f64>(7)?,
            "price_cny":  row.get::<_, f64>(8)?,
            "source":     row.get::<_, String>(9)?,
            "fetched_at": row.get::<_, String>(10)?,
        }))
    })?;

    for row in rows {
        let quote = row?;
        let id = quote["asset_id"].as_str().expect("asset_id 必须存在");
        let category = quote["category"].as_str().expect("category 必须存在");

        // 应用过滤条件
        if (asset_ids.is_empty() || asset_ids.iter().any(|item| item == id))
            && (categories.is_empty() || categories.iter().any(|item| item == category))
        {
            prices.insert(id.to_owned(), quote);
        }
    }

    Ok(json!({"prices": prices}))
}

/// 返回按时间聚合的历史价格数据。
///
/// # 参数
/// * `asset_ids` - 空 Vec 返回所有资产；非空则过滤
/// * `limit` - 限制不同时间戳的数量（不是行数）。0 表示不限制（兼容旧版行为）
/// * `full` - true 时返回 price、fx_to_cny、source 字段；false 时只返回 price_cny
///
/// # 返回
/// ```json
/// {
///   "points": [
///     { "timestamp": "...", "price_cny": {"asset_id": 123.45, ...}, "prices": {...}, ... },
///     ...
///   ]
/// }
/// ```
///
/// # 实现说明
/// 结果按 fetched_at 排序后按时间戳分组。使用 Map<String, Value> 作为分组键，
/// 因此同一时间戳下不同资产的报价被合并到同一个 point 对象中。
pub fn asset_history(
    database: &Path,
    asset_ids: &[String],
    limit: Option<String>,
    full: bool,
) -> Result<Value> {
    let connection = Connection::open(database)?;

    // 解析限制数量，默认 5000 个时间点
    let timestamp_limit = limit
        .as_deref()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5000);

    let mut points: Map<String, Value> = Map::new();

    let mut statement = connection.prepare(
        "SELECT asset_id, fetched_at, price, fx_to_cny, price_cny, source \
         FROM asset_price_history \
         ORDER BY fetched_at, asset_id",
    )?;

    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,  // asset_id
            row.get::<_, String>(1)?,  // fetched_at
            row.get::<_, f64>(2)?,     // price
            row.get::<_, f64>(3)?,     // fx_to_cny
            row.get::<_, f64>(4)?,     // price_cny
            row.get::<_, String>(5)?,  // source
        ))
    })?;

    for row in rows {
        let (asset_id, timestamp, price, fx_to_cny, price_cny, source) = row?;

        // 过滤不匹配的资产
        if !asset_ids.is_empty() && !asset_ids.contains(&asset_id) {
            continue;
        }

        // 达到时间戳数量上限时停止（以时间戳为计数单位，而非行数）
        if !points.contains_key(&timestamp)
            && timestamp_limit > 0
            && points.len() >= timestamp_limit
        {
            break;
        }

        // 同一时间戳下的数据合并到同一个 point
        let point = points
            .entry(timestamp.clone())
            .or_insert_with(|| {
                json!({
                    "timestamp": timestamp,
                    "price_cny": {},
                    "prices": {},
                    "fx_to_cny": {},
                    "sources": {},
                })
            });

        // 始终填充 price_cny（人民币价格）
        point["price_cny"][&asset_id] = json!(price_cny);

        // full 模式下额外填充原始价格、汇率和数据源
        if full {
            point["prices"][&asset_id] = json!(price);
            point["fx_to_cny"][&asset_id] = json!(fx_to_cny);
            point["sources"][&asset_id] = json!(source);
        }
    }

    // 非 full 模式下移除冗余字段，减小响应体
    if !full {
        for point in points.values_mut() {
            let object = point.as_object_mut().expect("point 必须是 object");
            object.remove("prices");
            object.remove("fx_to_cny");
            object.remove("sources");
        }
    }

    Ok(json!({
        "points": points.into_values().collect::<Vec<_>>()
    }))
}

/// 兼容 `/api/prices/latest` 旧版接口：按交易代码（symbol）查询最新价格。
///
/// 该接口先于 asset_id 体系存在，只支持加密货币（market='CRYPTO'）。
/// 当前保留以保持与旧版客户端的兼容性。
///
/// # 参数
/// * `symbols` - 交易代码列表，如 ["BTC", "ETH"]
pub fn latest_crypto_prices(database: &Path, symbols: &[String]) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut prices = Map::new();

    for symbol in symbols {
        let quote = connection
            .query_row(
                "SELECT price, source, fetched_at \
                 FROM asset_price_history \
                 WHERE market='CRYPTO' AND symbol=? \
                 ORDER BY fetched_at DESC LIMIT 1",
                [symbol],
                |row| {
                    Ok(json!({
                        "price":      row.get::<_, f64>(0)?,
                        "source":     row.get::<_, String>(1)?,
                        "fetched_at": row.get::<_, String>(2)?,
                    }))
                },
            )
            .optional()?;  // 无报价时返回 None 而非报错

        if let Some(quote) = quote {
            prices.insert(symbol.clone(), quote);
        }
    }

    Ok(json!({"prices": prices}))
}

use rusqlite::OptionalExtension;
