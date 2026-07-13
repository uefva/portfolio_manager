//! 投资组合持久化、校验、持仓计算与 JSON 导入导出。
//!
//! 本模块包含整个应用的核心业务逻辑：
//!   - 资产的增删改查
//!   - 交易记录的增删改查（含卖出校验）
//!   - 实时持仓计算（结合最新报价）
//!   - 盈亏历史走势
//!   - v2 JSON 格式的导入导出

use crate::utils::now_text;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, path::Path};

// ═══════════════════════════════════════════════════════════════
// 资产 CRUD
// ═══════════════════════════════════════════════════════════════

/// 返回完整的资产目录。即使某资产无交易记录也会包含在内。
///
/// # 参数
/// * `database` - SQLite 数据库文件路径
///
/// # 返回
/// 包含 `data` 数组的 JSON，每个元素为一条资产记录（含空的 transactions 字段保持兼容）
pub fn list_assets(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare(
        "SELECT asset_id, category, market, symbol, name, currency, created_at, updated_at \
         FROM portfolio_assets ORDER BY category, market, symbol",
    )?;
    let assets = statement
        .query_map([], |row| {
            Ok(json!({
                "asset_id":  row.get::<_, String>(0)?,
                "category":  row.get::<_, String>(1)?,
                "market":    row.get::<_, String>(2)?,
                "symbol":    row.get::<_, String>(3)?,
                "name":      row.get::<_, String>(4)?,
                "currency":  row.get::<_, String>(5)?,
                "created_at": row.get::<_, String>(6)?,
                "updated_at": row.get::<_, String>(7)?,
                "transactions": [],  // 保持旧版 JSON 结构兼容
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": assets}))
}

pub fn active_assets(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare(
        "SELECT a.asset_id, a.category, a.market, a.symbol, a.name, a.currency \
         FROM portfolio_assets a JOIN portfolio_transactions t ON t.asset_id=a.asset_id \
         GROUP BY a.asset_id HAVING sum(CASE WHEN t.type='buy' THEN t.amount ELSE -t.amount END) > 0",
    )?;
    let assets = statement
        .query_map([], |row| {
            Ok(json!({
                "asset_id": row.get::<_, String>(0)?, "category": row.get::<_, String>(1)?,
                "market": row.get::<_, String>(2)?, "symbol": row.get::<_, String>(3)?,
                "name": row.get::<_, String>(4)?, "currency": row.get::<_, String>(5)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": assets}))
}

/// 新增或更新一个资产。
///
/// # 参数
/// * `previous_id` - 如果是更新操作，传入原 asset_id；新增时传 None
/// * `input` - 请求体 JSON，可包含 category/market/symbol/name 字段
///
/// # 校验规则
///   - symbol 不能为空
///   - 如果资产已有交易记录，不允许修改 asset_id（防止数据引用断裂）
///   - 如果修改了 category/market/symbol 导致 asset_id 变化，需要有 0 条交易才能执行
pub fn save_asset(database: &Path, previous_id: Option<String>, input: &Value) -> Result<Value> {
    // 从输入中提取字段，使用合理的默认值
    let category = canonical_category(input["category"].as_str().unwrap_or("crypto"));
    let category = category.as_str();
    let market = input["market"].as_str().unwrap_or("CRYPTO");
    let symbol = input["symbol"].as_str().unwrap_or("").trim().to_uppercase();
    anyhow::ensure!(!symbol.is_empty(), "symbol 不能为空");

    // 构造全局唯一 asset_id：类型:市场:代码
    let asset_id = format!("{}:{}:{}", asset_kind(category), market, symbol);

    // 名称默认为代码本身
    let name = input["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&symbol);

    // 根据类别和市场推断结算币种
    let currency = currency_for(category, market);

    let connection = Connection::open(database)?;

    // 更新操作：检查是否可以修改 asset_id
    if let Some(previous_id) = previous_id {
        let transaction_count: i64 = connection.query_row(
            "SELECT count(*) FROM portfolio_transactions WHERE asset_id=?",
            [&previous_id],
            |row| row.get(0),
        )?;
        // 已有交易记录的资产不允许修改代码（asset_id 不能变）
        anyhow::ensure!(
            transaction_count == 0 || previous_id == asset_id,
            "资产已有交易记录，不能修改代码"
        );
        // 删除旧记录（如果 asset_id 变了，旧记录变成孤儿也无妨，上面已校验）
        connection.execute(
            "DELETE FROM portfolio_assets WHERE asset_id=?",
            [previous_id],
        )?;
    }

    // UPSERT：存在则更新 name 和 updated_at，不存在则插入
    connection.execute(
        "INSERT INTO portfolio_assets VALUES(?,?,?,?,?,?,?,?) \
         ON CONFLICT(asset_id) DO UPDATE SET \
           name=excluded.name, \
           updated_at=excluded.updated_at",
        params![
            asset_id,
            category,
            market,
            symbol,
            name,
            currency,
            now_text(), // created_at
            now_text(), // updated_at
        ],
    )?;

    Ok(json!({
        "asset_id": asset_id,
        "category": category,
        "market":   market,
        "symbol":   symbol,
        "name":     name,
        "currency": currency,
    }))
}

/// 删除资产。只允许删除没有任何关联交易的资产，保留交易历史是刻意为之。
///
/// # 错误
/// 如果资产有关联交易记录，返回错误信息。
pub fn delete_asset(database: &Path, asset_id: &str) -> Result<Value> {
    let connection = Connection::open(database)?;
    let transaction_count: i64 = connection.query_row(
        "SELECT count(*) FROM portfolio_transactions WHERE asset_id=?",
        [asset_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(transaction_count == 0, "资产有关联交易记录，无法删除");
    connection.execute("DELETE FROM portfolio_assets WHERE asset_id=?", [asset_id])?;
    Ok(json!({"data": {"deleted": true}}))
}

// ═══════════════════════════════════════════════════════════════
// 交易记录 CRUD
// ═══════════════════════════════════════════════════════════════

/// 返回所有交易记录，并关联资产的类别、市场、代码和名称。
/// 结果按日期和 ID 排序。
pub fn list_transactions(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare(
        "SELECT t.id, t.asset_id, t.type, t.date, t.amount, t.price, t.total, t.currency, \
                a.category, a.market, a.symbol, a.name \
         FROM portfolio_transactions t \
         JOIN portfolio_assets a ON a.asset_id = t.asset_id \
         ORDER BY t.date, t.id",
    )?;
    let transactions = statement
        .query_map([], |row| {
            Ok(json!({
                "id":       row.get::<_, i64>(0)?,
                "asset_id": row.get::<_, String>(1)?,
                "type":     row.get::<_, String>(2)?,
                "date":     row.get::<_, String>(3)?,
                "amount":   row.get::<_, f64>(4)?,
                "price":    row.get::<_, f64>(5)?,
                "total":    row.get::<_, f64>(6)?,
                "currency": row.get::<_, String>(7)?,
                "category": row.get::<_, String>(8)?,
                "market":   row.get::<_, String>(9)?,
                "symbol":   row.get::<_, String>(10)?,
                "name":     row.get::<_, String>(11)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": transactions}))
}

/// 保存（新增或更新）一笔交易记录。
///
/// # 校验规则
///   1. amount 必须 > 0，price 必须 >= 0
///   2. 如果是卖出（sell），当前持仓量必须 >= 卖出数量（不能卖空）
///   3. 如果请求中没有 asset_id，自动根据 category/market/symbol 创建资产
///
/// # 参数
/// * `transaction_id` - 更新时传入交易 ID；新增时传 None
/// * `input` - 包含交易字段的 JSON
pub fn save_transaction(
    database: &Path,
    transaction_id: Option<i64>,
    input: &Value,
) -> Result<Value> {
    // 如果没有提供 asset_id，先创建资产（传 category/market/symbol）
    let connection = Connection::open(database)?;
    let asset_id = match (transaction_id, input["asset_id"].as_str()) {
        (_, Some(id)) => id.to_owned(),
        (Some(id), None) => connection.query_row(
            "SELECT asset_id FROM portfolio_transactions WHERE id=?",
            [id],
            |row| row.get(0),
        )?,
        (None, None) => save_asset(database, None, input)?["asset_id"]
            .as_str()
            .expect("asset id 已创建")
            .to_owned(),
    };

    let transaction_type = input["type"].as_str().unwrap_or("buy");
    anyhow::ensure!(
        matches!(transaction_type, "buy" | "sell"),
        "交易类型必须是 buy 或 sell"
    );
    let amount = input["amount"].as_f64().context("amount 不能为空")?;
    let price = input["price"].as_f64().context("price 不能为空")?;
    anyhow::ensure!(amount > 0.0 && price >= 0.0, "数量或价格无效");

    // 卖出校验：计算当前持仓量（买入总和 - 卖出总和）
    if transaction_type == "sell" {
        let mut units: f64 = connection.query_row(
            "SELECT coalesce(sum(CASE WHEN type='buy' THEN amount ELSE -amount END), 0) \
             FROM portfolio_transactions WHERE asset_id=?",
            [&asset_id],
            |row| row.get(0),
        )?;
        if let Some(id) = transaction_id {
            let previous: Option<(String, f64)> = connection
                .query_row(
                    "SELECT type, amount FROM portfolio_transactions WHERE id=?",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((previous_type, previous_amount)) = previous {
                if previous_type == "sell" {
                    units += previous_amount;
                }
            }
        }
        anyhow::ensure!(units >= amount, "卖出数量超过当前持仓");
    }

    // 从资产表获取结算币种
    let currency: String = connection.query_row(
        "SELECT currency FROM portfolio_assets WHERE asset_id=?",
        [&asset_id],
        |row| row.get(0),
    )?;

    // 日期默认使用当前 UTC 时间
    let date = input["date"]
        .as_str()
        .unwrap_or(&now_text())
        .replace('T', " ");
    let total = amount * price;

    let id = if let Some(id) = transaction_id {
        // 更新已有交易记录
        connection.execute(
            "UPDATE portfolio_transactions \
             SET asset_id=?, type=?, date=?, amount=?, price=?, total=?, currency=?, updated_at=? \
             WHERE id=?",
            params![
                asset_id,
                transaction_type,
                date,
                amount,
                price,
                total,
                currency,
                now_text(),
                id
            ],
        )?;
        id
    } else {
        // 新增交易记录
        connection.execute(
            "INSERT INTO portfolio_transactions \
             (asset_id, type, date, amount, price, total, currency, created_at, updated_at) \
             VALUES(?,?,?,?,?,?,?,?,?)",
            params![
                asset_id,
                transaction_type,
                date,
                amount,
                price,
                total,
                currency,
                now_text(),
                now_text()
            ],
        )?;
        connection.last_insert_rowid()
    };

    Ok(json!({
        "id":       id,
        "asset_id": asset_id,
        "type":     transaction_type,
        "date":     date,
        "amount":   amount,
        "price":    price,
        "total":    total,
        "currency": currency,
    }))
}

/// 删除交易记录。
pub fn delete_transaction(database: &Path, id: i64) -> Result<Value> {
    Connection::open(database)?.execute("DELETE FROM portfolio_transactions WHERE id=?", [id])?;
    Ok(json!({"data": {"deleted": true}}))
}

pub fn create_snapshot(database: &Path, name: &str) -> Result<Value> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "快照名称不能为空");
    let portfolio = export_json(database)?;
    let created_at = now_text();
    let connection = Connection::open(database)?;
    connection.execute(
        "INSERT INTO portfolio_snapshots(name, portfolio_json, created_at) VALUES(?,?,?)",
        params![name, serde_json::to_string(&portfolio)?, created_at],
    )?;
    Ok(
        json!({"data": {"id": connection.last_insert_rowid(), "name": name, "created_at": created_at}}),
    )
}

pub fn list_snapshots(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare(
        "SELECT id, name, portfolio_json, created_at FROM portfolio_snapshots ORDER BY id DESC",
    )?;
    let snapshots = statement
        .query_map([], |row| {
            let serialized: String = row.get(2)?;
            let portfolio = serde_json::from_str::<Value>(&serialized).unwrap_or(Value::Null);
            Ok(json!({
                "id": row.get::<_, i64>(0)?, "name": row.get::<_, String>(1)?,
                "portfolio": portfolio, "created_at": row.get::<_, String>(3)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": snapshots}))
}

pub fn delete_snapshot(database: &Path, id: i64) -> Result<Value> {
    Connection::open(database)?.execute("DELETE FROM portfolio_snapshots WHERE id=?", [id])?;
    Ok(json!({"data": {"deleted": true}}))
}

pub fn save_holding_query_snapshot(database: &Path, holdings: &Value) -> Result<Value> {
    let queried_at = now_text();
    let total_value = holdings["total_value"].as_f64().unwrap_or_default();
    let total_profit = holdings["total_profit"].as_f64().unwrap_or_default();
    let connection = Connection::open(database)?;
    connection.execute(
        "INSERT INTO holding_query_snapshots(holdings_json,total_value,total_profit,queried_at) VALUES(?,?,?,?)",
        params![serde_json::to_string(holdings)?, total_value, total_profit, queried_at],
    )?;
    Ok(
        json!({"id": connection.last_insert_rowid(), "queried_at": queried_at, "total_value": total_value, "total_profit": total_profit}),
    )
}

pub fn list_holding_query_snapshots(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut statement = connection.prepare(
        "SELECT id, total_value, total_profit, queried_at FROM holding_query_snapshots ORDER BY id DESC",
    )?;
    let items = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?, "total_value": row.get::<_, f64>(1)?,
                "total_profit": row.get::<_, f64>(2)?, "queried_at": row.get::<_, String>(3)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"data": items}))
}

pub fn holding_query_snapshot(database: &Path, id: i64) -> Result<Value> {
    let connection = Connection::open(database)?;
    let serialized: String = connection.query_row(
        "SELECT holdings_json FROM holding_query_snapshots WHERE id=?",
        [id],
        |row| row.get(0),
    )?;
    Ok(json!({"data": serde_json::from_str::<Value>(&serialized)?}))
}

// ═══════════════════════════════════════════════════════════════
// 持仓计算
// ═══════════════════════════════════════════════════════════════

/// 使用每个活跃资产的最新报价，实时计算人民币计价的持仓。
///
/// # 计算逻辑
///   1. 汇总每个资产的买入量 - 卖出量，得到净持仓数量
///   2. 对数量 > 0 的资产，查询 asset_price_history 中的最新一条报价
///   3. 人民币市值 = 数量 × 价格 × fx_to_cny
///   4. 人民币成本 = 原始成本 × fx_to_cny（使用查询时刻的最新汇率，这是有意沿用旧版逻辑）
///   5. 盈亏 = 市值 - 成本
///
/// # 参数
/// * `category_filter` - 类别过滤，"全部" 表示不过滤
pub fn holdings(database: &Path, category_filter: &str) -> Result<Value> {
    let connection = Connection::open(database)?;
    let category_filter = category_filter_code(category_filter);

    // 查询每个资产的净持仓量和原始币种成本
    let mut statement = connection.prepare(
        "SELECT a.asset_id, a.category, a.market, a.symbol, a.name, a.currency, \
                coalesce(sum(CASE WHEN t.type='buy' THEN t.amount ELSE -t.amount END), 0), \
                coalesce(sum(CASE WHEN t.type='buy' THEN t.total  ELSE -t.total  END), 0) \
         FROM portfolio_assets a \
         LEFT JOIN portfolio_transactions t ON t.asset_id = a.asset_id \
         GROUP BY a.asset_id",
    )?;

    let mut rows = Vec::new();
    let mut total_value = 0.0; // 总人民币市值
    let mut total_cost = 0.0; // 总人民币成本
    let mut category_totals = Map::new(); // 各类别的汇总

    for result in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?, // asset_id
            row.get::<_, String>(1)?, // category
            row.get::<_, String>(2)?, // market
            row.get::<_, String>(3)?, // symbol
            row.get::<_, String>(4)?, // name
            row.get::<_, String>(5)?, // currency
            row.get::<_, f64>(6)?,    // quantity（净持仓量）
            row.get::<_, f64>(7)?,    // native_cost（原始币种成本）
        ))
    })? {
        let (asset_id, _category, market, symbol, name, currency, quantity, native_cost) = result?;

        // 跳过零持仓和不属于目标类别的资产
        let kind = asset_id_kind(&asset_id);
        if quantity <= 0.0 || category_filter.is_some_and(|filter| filter != kind) {
            continue;
        }

        // 查询该资产的最新报价
        let quote: Option<(f64, f64)> = connection
            .query_row(
                "SELECT price, fx_to_cny FROM asset_price_history \
                 WHERE asset_id=? ORDER BY fetched_at DESC LIMIT 1",
                [&asset_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        // 无报价时默认价格为 0
        let (price, fx_to_cny) = quote.unwrap_or((0.0, 1.0));

        // 计算人民币计价的市值和成本
        let value_cny = quantity * price * fx_to_cny;
        // 此处刻意沿用旧版计算方式：原始币种成本 × 当前汇率 = 人民币成本
        let cost_cny = native_cost * fx_to_cny;
        let profit_cny = value_cny - cost_cny;

        total_value += value_cny;
        total_cost += cost_cny;

        // 更新类别汇总
        category_totals.insert(
            kind.to_owned(),
            json!({
                "total_value":  value_cny,
                "total_cost":   cost_cny,
                "total_profit": profit_cny,
            }),
        );

        // 构建单条持仓明细
        rows.push(json!({
            "asset_id":    asset_id,
            "category":    canonical_category(kind),
            "market":      market,
            "symbol":      symbol,
            "name":        name,
            "quantity":    quantity,
            "avg_cost":    native_cost / quantity,  // 均价 = 总成本 / 数量
            "price":       price,
            "currency":    currency,
            "fx_to_cny":   fx_to_cny,
            "value_cny":   value_cny,
            "cost_cny":    cost_cny,
            "profit_cny":  profit_cny,
            "profit_rate": if cost_cny == 0.0 { 0.0 } else { profit_cny / cost_cny * 100.0 },
        }));
    }

    let total_profit = total_value - total_cost;
    Ok(json!({
        "holdings":          rows,
        "total_value":       total_value,
        "total_cost":        total_cost,
        "total_profit":      total_profit,
        "total_profit_rate": if total_cost == 0.0 { 0.0 }
                             else { total_profit / total_cost * 100.0 },
        "category_totals":   category_totals,
    }))
}

// ═══════════════════════════════════════════════════════════════
// v2 JSON 导入导出
// ═══════════════════════════════════════════════════════════════

/// 导出为旧版桌面客户端使用的 v2 JSON 格式。
///
/// 输出结构：
/// ```json
/// { "version": 2, "assets": { "<asset_id>": { ... "transactions": [...] } } }
/// ```
pub fn export_json(database: &Path) -> Result<Value> {
    let connection = Connection::open(database)?;
    let mut assets = Map::new();

    let mut statement = connection.prepare(
        "SELECT asset_id, category, market, symbol, name, currency FROM portfolio_assets",
    )?;

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

        // 查询该资产的所有交易记录
        let mut transactions = connection.prepare(
            "SELECT type, date, amount, price, total \
             FROM portfolio_transactions WHERE asset_id=? ORDER BY date, id",
        )?;
        let transactions = transactions
            .query_map([&asset_id], |row| {
                Ok(json!({
                    "type":   row.get::<_, String>(0)?,
                    "date":   row.get::<_, String>(1)?,
                    "amount": row.get::<_, f64>(2)?,
                    "price":  row.get::<_, f64>(3)?,
                    "total":  row.get::<_, f64>(4)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        assets.insert(
            asset_id,
            json!({
                "category":     category,
                "market":       market,
                "symbol":       symbol,
                "name":         name,
                "currency":     currency,
                "transactions": transactions,
            }),
        );
    }

    Ok(json!({"version": 2, "assets": assets}))
}

/// 从 v2 JSON 格式导入投资组合。
///
/// 导入是幂等的：匹配到相同 asset/type/date/amount/price 的交易会被跳过。
/// 这保证了重复导入同一份数据不会产生重复记录。
///
/// # 参数
/// * `portfolio` - v2 JSON 对象的 "assets" 部分
pub fn import_json(database: &Path, portfolio: Value) -> Result<Value> {
    let assets = portfolio["assets"]
        .as_object()
        .context("缺少 portfolio.assets 字段")?;

    let (mut imported_assets, mut imported_transactions, mut skipped_transactions) = (0, 0, 0);

    for asset in assets.values() {
        // 先保存资产（如果已存在则更新）
        let saved_asset = save_asset(database, None, asset)?;
        imported_assets += 1;

        // 逐笔处理交易
        for transaction in asset["transactions"].as_array().into_iter().flatten() {
            let mut transaction = transaction.clone();
            transaction["asset_id"] = saved_asset["asset_id"].clone();

            // 检查是否为重复交易（五要素完全匹配则跳过）
            let connection = Connection::open(database)?;
            let duplicate: i64 = connection.query_row(
                "SELECT count(*) FROM portfolio_transactions \
                 WHERE asset_id=? AND type=? AND date=? AND amount=? AND price=?",
                params![
                    transaction["asset_id"].as_str(),
                    transaction["type"].as_str(),
                    transaction["date"].as_str(),
                    transaction["amount"].as_f64(),
                    transaction["price"].as_f64(),
                ],
                |row| row.get(0),
            )?;

            if duplicate == 0 {
                save_transaction(database, None, &transaction)?;
                imported_transactions += 1;
            } else {
                skipped_transactions += 1;
            }
        }
    }

    Ok(json!({
        "assets_imported":       imported_assets,
        "assets_updated":        imported_assets,
        "transactions_imported": imported_transactions,
        "transactions_skipped":  skipped_transactions,
    }))
}

// ═══════════════════════════════════════════════════════════════
// 盈亏历史走势
// ═══════════════════════════════════════════════════════════════

/// 根据历史价格快照和交易记录，构建盈亏时间序列。
///
/// # 算法
///   对 asset_price_history 表中的每个去重时间点：
///   1. 获取该时刻所有资产的人民币报价
///   2. 累计该时刻之前（含当日）的净持仓量
///   3. 人民币市值 = 持仓量 × 该时刻人民币价格
///   4. 人民币成本 = 原始成本 × 该时刻汇率（与旧版逻辑一致）
///   5. 盈亏 = 市值 - 成本
///
/// # 参数
/// * `metric` - "收益金额" 或 "收益率"，控制 Y 轴数值类型
///
/// # 返回
/// ```json
/// {
///   "labels": ["2025-01-01 12:00:00", ...],
///   "series": { "总资产": [[0, 1000.0], ...], "加密货币": [[0, 500.0], ...] },
///   "metric": "收益金额"
/// }
/// ```
pub fn profit_history(
    database: &Path,
    metric: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Value> {
    let connection = Connection::open(database)?;
    let from = from.map(|value| {
        if value.len() == 10 {
            format!("{value} 00:00:00")
        } else {
            value.to_owned()
        }
    });
    let to = to.map(|value| {
        if value.len() == 10 {
            format!("{value} 23:59:59")
        } else {
            value.to_owned()
        }
    });

    // 第一步：获取所有去重的报价时间点（按时间升序）
    let mut timestamp_statement = connection
        .prepare("SELECT DISTINCT fetched_at FROM asset_price_history ORDER BY fetched_at")?;
    let timestamps = timestamp_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut labels = Vec::with_capacity(timestamps.len());
    let mut total_series = Vec::with_capacity(timestamps.len()); // 总资产走势
    let mut category_series: HashMap<String, Vec<Value>> = HashMap::new(); // 各类别走势

    // 第二步：逐时间点计算
    for timestamp in &timestamps {
        if from
            .as_deref()
            .is_some_and(|value| timestamp.as_str() < value)
            || to
                .as_deref()
                .is_some_and(|value| timestamp.as_str() > value)
        {
            continue;
        }
        // 获取该时间点的所有资产报价
        let mut quote_statement = connection.prepare(
            "SELECT asset_id, category, price_cny, fx_to_cny \
             FROM asset_price_history WHERE fetched_at=?",
        )?;
        let quotes = quote_statement.query_map([timestamp], |row| {
            Ok((
                row.get::<_, String>(0)?, // asset_id
                row.get::<_, String>(1)?, // category
                row.get::<_, f64>(2)?,    // price_cny（人民币价格）
                row.get::<_, f64>(3)?,    // fx_to_cny（汇率）
            ))
        })?;

        let mut total_value = 0.0;
        let mut total_cost = 0.0;
        let mut category_totals: HashMap<String, (f64, f64)> = HashMap::new();

        // 第三步：对每个有报价的资产，计算截至该时间点的持仓价值
        for quote in quotes {
            let (asset_id, _category, price_cny, fx_to_cny) = quote?;

            // 累计该时间点之前（含当日）的净持仓量
            let (quantity, native_cost): (f64, f64) = connection.query_row(
                "SELECT coalesce(sum(CASE WHEN type='buy' THEN amount ELSE -amount END), 0), \
                        coalesce(sum(CASE WHEN type='buy' THEN total  ELSE -total  END), 0) \
                 FROM portfolio_transactions \
                 WHERE asset_id=? AND date<=?",
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

            // 按类别汇总
            let total = category_totals
                .entry(asset_id_kind(&asset_id).to_owned())
                .or_insert((0.0, 0.0));
            total.0 += value_cny;
            total.1 += cost_cny;
        }

        // 第四步：将计算结果推入时间序列
        labels.push(timestamp.clone());
        total_series.push(json!([
            labels.len() - 1,
            metric_value(total_value - total_cost, total_cost, metric)
        ]));

        for (category, (value, cost)) in category_totals {
            category_series.entry(category).or_default().push(json!([
                labels.len() - 1,
                metric_value(value - cost, cost, metric)
            ]));
        }
    }

    // 组装返回结构
    let mut series = Map::new();
    series.insert("total".to_owned(), Value::Array(total_series));
    for (category, points) in category_series {
        series.insert(category, Value::Array(points));
    }

    Ok(json!({
        "labels":     labels,
        "series":     series,
        "all_series": series.clone(),
        "series_meta": {},
        "metric":     metric,
        "source":     "server",
    }))
}

/// 根据指标类型转换 Y 轴数值。
/// - "收益率" 模式：返回盈亏百分比（profit / cost * 100）
/// - 默认模式：返回绝对值（profit）
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

// ═══════════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════════

/// 根据资产类别，返回 asset_id 中的类型前缀。
/// - 加密货币 → crypto
/// - 基金     → fund
/// - 其他     → stock（股票）
fn asset_kind(category: &str) -> &'static str {
    category_code_from_text(category).unwrap_or("stock")
}

/// 根据资产类别和交易市场，推断结算币种。
/// - 港股（HK）    → HKD
/// - A 股/基金     → CNY
/// - 其他（美股等）→ USD
fn currency_for(category: &str, market: &str) -> &'static str {
    if market == "HK" {
        "HKD"
    } else if market == "SH" || market == "SZ" || asset_kind(category) == "fund" {
        "CNY"
    } else {
        "USD"
    }
}

fn canonical_category(category: &str) -> String {
    match category_code_from_text(category).unwrap_or("crypto") {
        "crypto" => "加密货币".to_owned(),
        "fund" => "基金".to_owned(),
        "stock" => "股票".to_owned(),
        _ => "加密货币".to_owned(),
    }
}

fn category_filter_code(category: &str) -> Option<&'static str> {
    let value = category.trim().to_ascii_lowercase();
    if value.is_empty() || matches!(value.as_str(), "all" | "全部") {
        None
    } else {
        category_code_from_text(category)
    }
}

fn category_code_from_text(category: &str) -> Option<&'static str> {
    let value = category.trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "全部" => None,
        "crypto" | "coin" | "加密货币" | "鍔犲瘑璐у竵" => Some("crypto"),
        "fund" | "基金" | "鍩洪噾" => Some("fund"),
        "stock" | "股票" | "鑲＄エ" => Some("stock"),
        _ => None,
    }
}

fn asset_id_kind(asset_id: &str) -> &str {
    match asset_id.split(':').next().unwrap_or_default() {
        "crypto" => "crypto",
        "fund" => "fund",
        "stock" => "stock",
        _ => "stock",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use std::{fs, path::PathBuf};

    fn database_path() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "portfolio-manager-test-{}-{}.sqlite3",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::File::create(&path).unwrap();
        database::initialize(&path).unwrap();
        path
    }

    #[test]
    fn asset_transaction_holdings_export_import_and_snapshots_work() {
        let database = database_path();
        let asset = save_asset(
            &database,
            None,
            &json!({
                "category":"加密货币", "market":"CRYPTO", "symbol":"btc", "name":"Bitcoin"
            }),
        )
        .unwrap();
        let asset_id = asset["asset_id"].as_str().unwrap();
        let buy = save_transaction(
            &database,
            None,
            &json!({
                "asset_id":asset_id, "type":"buy", "amount":2.0, "price":100.0, "date":"2026-01-01"
            }),
        )
        .unwrap();
        let sell = save_transaction(
            &database,
            None,
            &json!({
                "asset_id":asset_id, "type":"sell", "amount":0.5, "price":150.0, "date":"2026-01-02"
            }),
        )
        .unwrap();
        save_transaction(
            &database,
            Some(sell["id"].as_i64().unwrap()),
            &json!({
                "type":"sell", "amount":1.0, "price":160.0, "date":"2026-01-03"
            }),
        )
        .unwrap();
        assert!(save_transaction(
            &database,
            None,
            &json!({
                "asset_id":asset_id, "type":"sell", "amount":2.0, "price":1.0
            })
        )
        .is_err());

        Connection::open(&database).unwrap().execute(
            "INSERT INTO asset_price_history(asset_id,category,market,symbol,name,currency,price,fx_to_cny,price_cny,source,fetched_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            params![asset_id, "加密货币", "CRYPTO", "BTC", "Bitcoin", "USD", 200.0, 7.0, 1400.0, "test", "2026-01-04 00:00:00"],
        ).unwrap();
        let holdings = holdings(&database, "全部").unwrap();
        assert_eq!(holdings["holdings"][0]["quantity"], 1.0);
        assert_eq!(holdings["holdings"][0]["value_cny"], 1400.0);
        let first_snapshot = save_holding_query_snapshot(&database, &holdings).unwrap();
        let second_snapshot = save_holding_query_snapshot(&database, &holdings).unwrap();
        assert_ne!(first_snapshot["id"], second_snapshot["id"]);
        let history = list_holding_query_snapshots(&database).unwrap();
        assert_eq!(history["data"].as_array().unwrap().len(), 2);
        let restored =
            holding_query_snapshot(&database, first_snapshot["id"].as_i64().unwrap()).unwrap();
        assert_eq!(restored["data"], holdings);

        let exported = export_json(&database).unwrap();
        assert!(exported["assets"].is_object());
        create_snapshot(&database, "before import").unwrap();
        assert_eq!(
            list_snapshots(&database).unwrap()["data"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let imported = import_json(&database, exported.clone()).unwrap();
        assert_eq!(imported["transactions_imported"], 0);
        delete_transaction(&database, buy["id"].as_i64().unwrap()).unwrap();
        fs::remove_file(database).unwrap();
    }

    #[test]
    fn asset_cannot_be_deleted_when_transactions_exist() {
        let database = database_path();
        let asset = save_asset(&database, None, &json!({"symbol":"ETH"})).unwrap();
        let id = asset["asset_id"].as_str().unwrap();
        save_transaction(
            &database,
            None,
            &json!({"asset_id":id,"type":"buy","amount":1.0,"price":1.0}),
        )
        .unwrap();
        assert!(delete_asset(&database, id).is_err());
        fs::remove_file(database).unwrap();
    }

    #[test]
    fn profit_history_honors_inclusive_date_range() {
        let database = database_path();
        let asset = save_asset(&database, None, &json!({"symbol":"BTC"})).unwrap();
        let id = asset["asset_id"].as_str().unwrap();
        save_transaction(
            &database,
            None,
            &json!({"asset_id":id,"type":"buy","amount":1.0,"price":10.0,"date":"2026-01-01"}),
        )
        .unwrap();
        let connection = Connection::open(&database).unwrap();
        for (time, price) in [("2026-01-01 00:00:00", 10.0), ("2026-01-10 00:00:00", 20.0)] {
            connection.execute("INSERT INTO asset_price_history(asset_id,category,market,symbol,name,currency,price,fx_to_cny,price_cny,source,fetched_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)", params![id, "加密货币", "CRYPTO", "BTC", "BTC", "USD", price, 1.0, price, "test", time]).unwrap();
        }
        let chart = profit_history(
            &database,
            "收益金额",
            Some("2026-01-05"),
            Some("2026-01-10"),
        )
        .unwrap();
        assert_eq!(chart["labels"].as_array().unwrap().len(), 1);
        assert_eq!(chart["labels"][0], "2026-01-10 00:00:00");
        drop(connection);
        fs::remove_file(database).unwrap();
    }
}
