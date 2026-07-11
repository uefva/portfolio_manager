CREATE TABLE IF NOT EXISTS asset_price_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT, asset_id TEXT NOT NULL,
    category TEXT NOT NULL, market TEXT NOT NULL, symbol TEXT NOT NULL,
    name TEXT NOT NULL, currency TEXT NOT NULL, price REAL NOT NULL,
    fx_to_cny REAL NOT NULL, price_cny REAL NOT NULL, source TEXT NOT NULL,
    fetched_at TEXT NOT NULL, UNIQUE(asset_id, fetched_at)
);
CREATE TABLE IF NOT EXISTS portfolio_assets (
    asset_id TEXT PRIMARY KEY, category TEXT NOT NULL, market TEXT NOT NULL,
    symbol TEXT NOT NULL, name TEXT NOT NULL, currency TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS portfolio_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT, asset_id TEXT NOT NULL, type TEXT NOT NULL,
    date TEXT NOT NULL, amount REAL NOT NULL, price REAL NOT NULL, total REAL NOT NULL,
    currency TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS portfolio_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_asset_price_history_asset_time ON asset_price_history(asset_id, fetched_at);
CREATE INDEX IF NOT EXISTS idx_portfolio_transactions_asset_date ON portfolio_transactions(asset_id, date);
