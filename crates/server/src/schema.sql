-- ============================================================
-- Portfolio Manager 数据库 Schema
-- ============================================================
-- 本文件在服务端启动时通过 database::initialize 自动执行。
-- 使用 CREATE TABLE IF NOT EXISTS，确保多次执行安全。

-- -----------------------------------------------------------
-- 资产历史价格表：存储每次拉取到的报价快照
-- -----------------------------------------------------------
-- 每条记录对应某个资产在某个时刻的一次报价。
-- asset_id + fetched_at 组合唯一，防止同一时刻重复写入。
CREATE TABLE IF NOT EXISTS asset_price_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id TEXT NOT NULL,       -- 资产唯一标识，格式：类型:市场:代码（如 crypto:CRYPTO:BTC）
    category TEXT NOT NULL,       -- 资产类别：加密货币/股票/基金
    market TEXT NOT NULL,         -- 交易市场：CRYPTO/HK/SH/SZ/NASDAQ
    symbol TEXT NOT NULL,         -- 交易代码（大写），如 BTC、ETH、0700
    name TEXT NOT NULL,           -- 资产名称，如 Bitcoin、腾讯控股
    currency TEXT NOT NULL,       -- 原始报价币种：USD/HKD/CNY
    price REAL NOT NULL,          -- 原始币种下的单价
    fx_to_cny REAL NOT NULL,      -- 该时刻原始币种兑人民币汇率
    price_cny REAL NOT NULL,      -- 折算人民币单价 = price × fx_to_cny
    source TEXT NOT NULL,         -- 数据来源：eastmoney/binance/coingecko 等
    fetched_at TEXT NOT NULL,     -- 报价抓取时间（UTC），格式 YYYY-MM-DD HH:MM:SS
    UNIQUE(asset_id, fetched_at)  -- 同一资产同一时刻只保留一条记录
);

-- -----------------------------------------------------------
-- 投资组合资产目录表：记录用户关注的资产
-- -----------------------------------------------------------
-- 每个资产一条记录，asset_id 为全局唯一主键。
CREATE TABLE IF NOT EXISTS portfolio_assets (
    asset_id TEXT PRIMARY KEY,    -- 资产唯一标识，格式：类型:市场:代码
    category TEXT NOT NULL,       -- 资产类别
    market TEXT NOT NULL,         -- 交易市场
    symbol TEXT NOT NULL,         -- 交易代码
    name TEXT NOT NULL,           -- 资产名称
    currency TEXT NOT NULL,       -- 交易结算币种
    created_at TEXT NOT NULL,     -- 创建时间（UTC）
    updated_at TEXT NOT NULL      -- 最后更新时间（UTC）
);

-- -----------------------------------------------------------
-- 投资组合交易记录表：买入/卖出操作
-- -----------------------------------------------------------
-- 每笔交易关联一个 portfolio_assets 中的资产。
-- type 字段：buy = 买入，sell = 卖出
CREATE TABLE IF NOT EXISTS portfolio_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id TEXT NOT NULL,       -- 关联的资产 ID
    type TEXT NOT NULL,           -- 交易类型：buy 或 sell
    date TEXT NOT NULL,           -- 交易日期（YYYY-MM-DD 或完整时间戳）
    amount REAL NOT NULL,         -- 交易数量（买入为正，卖出时通过 type 区分）
    price REAL NOT NULL,          -- 成交单价（原始币种）
    total REAL NOT NULL,          -- 成交总额 = amount × price
    currency TEXT NOT NULL,       -- 结算币种
    created_at TEXT NOT NULL,     -- 记录创建时间
    updated_at TEXT NOT NULL      -- 记录最后更新时间
);

-- -----------------------------------------------------------
-- 投资组合元数据表：存储键值对配置
-- -----------------------------------------------------------
CREATE TABLE IF NOT EXISTS portfolio_meta (
    key TEXT PRIMARY KEY,         -- 配置键
    value TEXT NOT NULL,          -- 配置值
    updated_at TEXT NOT NULL      -- 最后更新时间
);

-- -----------------------------------------------------------
-- 索引：加速常用查询
-- -----------------------------------------------------------
-- 加速按资产和时间范围查询历史价格（持仓计算和走势图依赖此索引）
CREATE INDEX IF NOT EXISTS idx_asset_price_history_asset_time
    ON asset_price_history(asset_id, fetched_at);

-- 加速按资产和日期聚合交易记录（盈亏历史计算依赖此索引）
CREATE INDEX IF NOT EXISTS idx_portfolio_transactions_asset_date
    ON portfolio_transactions(asset_id, date);
