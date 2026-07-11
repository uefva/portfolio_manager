//! SQLite 数据库初始化与旧版历史价格迁移。
//!
//! 启动流程：
//!   1. 如果目标数据库不存在，从旧项目复制
//!   2. 创建目录和连接
//!   3. 执行 PRAGMA integrity_check 校验完整性
//!   4. 运行 schema.sql 确保表结构存在
//!   5. 将旧 price_history 表数据迁移到 asset_price_history

use anyhow::Result;
use rusqlite::Connection;
use std::{fs, path::Path};

/// 源数据库只读：首次启动时将其复制到新项目目录。
/// 硬编码路径是因为旧项目位置固定，且这是唯一的数据迁移来源。
const LEGACY_DATABASE: &str =
    r"E:\fengkai\my_project\crypto_portfolio_manager\price_history.sqlite3";

/// 创建兼容的数据库 schema，不修改任何旧业务表。
///
/// # 参数
/// * `database_path` - SQLite 数据库文件的目标路径
///
/// # 返回
/// 成功返回 Ok(())，失败返回带上下文的 anyhow::Error
pub fn initialize(database_path: &Path) -> Result<()> {
    // 第一步：如果目标数据库不存在且旧数据库存在，直接复制
    if !database_path.exists() && Path::new(LEGACY_DATABASE).exists() {
        fs::create_dir_all(database_path.parent().unwrap_or(Path::new(".")))?;
        fs::copy(LEGACY_DATABASE, database_path)?;
        tracing::info!(
            source = LEGACY_DATABASE,
            target = %database_path.display(),
            "已复制旧版 SQLite 数据库"
        );
    }

    // 第二步：确保父目录存在（如果路径中包含尚未创建的目录）
    fs::create_dir_all(database_path.parent().unwrap_or(Path::new(".")))?;
    let connection = Connection::open(database_path)?;

    // 第三步：数据库完整性校验
    let integrity: String =
        connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    anyhow::ensure!(integrity == "ok", "SQLite 完整性校验失败");
    tracing::debug!(%integrity, "SQLite 完整性校验通过");

    // 第四步：执行 schema.sql，确保所有表和索引存在
    // include_str! 在编译时将 SQL 文件内容嵌入二进制
    connection.execute_batch(include_str!("schema.sql"))?;
    tracing::debug!("已确保 SQLite schema 就绪");

    // 第五步：迁移旧版加密货币价格数据
    migrate_legacy_crypto_prices(&connection)?;
    Ok(())
}

/// 旧版项目只在 `price_history` 表中存储加密货币价格。
/// 保持该表不动，将其数据行插入到多资产通用的 `asset_price_history` 表。
///
/// 迁移规则：
///   - asset_id 生成为 "crypto:CRYPTO:{大写符号}"
///   - 原始价格为 USD 计价
///   - fx_to_cny 使用固定汇率 7.2（旧数据无历史汇率记录）
///   - 使用 INSERT OR IGNORE 防止重复迁移
fn migrate_legacy_crypto_prices(connection: &Connection) -> Result<()> {
    // 先检查旧表是否存在
    let has_legacy_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='price_history')",
        [],
        |row| row.get(0),
    )?;

    if has_legacy_table {
        let migrated = connection.execute(
            "INSERT OR IGNORE INTO asset_price_history \
             (asset_id, category, market, symbol, name, currency, price, fx_to_cny, price_cny, source, fetched_at) \
             SELECT \
               'crypto:CRYPTO:' || upper(symbol), \
               '加密货币', \
               'CRYPTO', \
               upper(symbol), \
               upper(symbol), \
               'USD', \
               price, \
               7.2, \
               price * 7.2, \
               coalesce(source, 'legacy'), \
               fetched_at \
             FROM price_history",
            [],
        )?;
        tracing::info!(migrated, "已迁移旧版加密货币价格记录");
    }
    Ok(())
}
