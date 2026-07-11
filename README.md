# Portfolio Manager（Rust + Dioxus）

面向股票、基金和加密货币的本地投资组合管理器。项目以 Rust/Axum 服务端、SQLite 数据库和 Dioxus Desktop/Web 客户端实现；默认不要求登录、JWT 或首次密码初始化。

## 快速启动

需要 Rust stable；可选安装 Python 3.12+ 以运行行情适配器。

```powershell
# 终端 1：启动服务端（默认 http://127.0.0.1:8765）
cargo run -p portfolio-server

# 终端 2：启动 Desktop 客户端
cargo run -p portfolio-client
```

Web 模式需先安装 Dioxus CLI：

```powershell
cargo install dioxus-cli
dx serve -p portfolio-client --no-default-features --features web
```

## 客户端行为

客户端启动后会自动从服务端加载以下数据：

- 持仓：`GET /api/portfolio/holdings?category=全部`
- 资产目录：`GET /api/portfolio/assets`
- 交易记录：`GET /api/portfolio/transactions`
- 收益走势：`GET /api/portfolio/profit-history`

界面顶部可直接修改服务端地址。地址变更后，客户端会自动重新加载全部数据；每个页签仍保留手动刷新按钮。请求日志以 `[client]` 输出到启动客户端的终端，成功或失败信息显示在窗口底部状态栏。

当前可通过界面执行：

- 查询持仓
- 查看和新增资产
- 查看和新增买入/卖出交易
- 请求服务端组合导出
- 请求收益走势数据

## 数据库兼容

默认数据库路径为 `data/price_history.sqlite3`。

- 若当前数据库不存在，服务端复制旧项目的 `E:\fengkai\my_project\crypto_portfolio_manager\price_history.sqlite3`；不会修改原数据库。
- 启动时运行 SQLite `PRAGMA integrity_check`。
- 旧 `price_history` 表中的加密货币价格会以 `INSERT OR IGNORE` 迁移到 `asset_price_history`；旧表保留。
- 服务端支持旧 `portfolio.json` v2 导入和导出，导入会跳过相同资产、类型、日期、数量与价格的重复交易。

可选运行环境变量：

```ini
CPM_BIND=127.0.0.1:8765
CPM_DATABASE=data/price_history.sqlite3
CPM_SERVER_URL=http://127.0.0.1:8765
CPM_ADAPTER_URL=http://127.0.0.1:8786
```

## Python 行情适配器

适配器复用旧 Python 项目的东方财富、OKX、Binance、CoinGecko 和汇率查询逻辑，并默认只监听本机：

```powershell
pip install -r market_adapter/requirements.txt
python market_adapter/app.py
```

可使用 `CPM_LEGACY_ROOT` 指定旧项目位置，使用 `CPM_ADAPTER_BIND` 和 `CPM_ADAPTER_PORT` 修改监听地址。适配器会输出带时间戳的请求、搜索和报价批次日志；设置 `CPM_ADAPTER_LOG_LEVEL=DEBUG` 可查看更详细信息。

## 服务端接口

```text
GET    /api/health
GET    /api/symbols
GET    /api/prices/latest?symbols=BTC,ETH
GET    /api/prices/history?symbols=BTC,ETH&limit=2000
GET    /api/assets/latest?asset_ids=crypto:CRYPTO:BTC
GET    /api/assets/history?asset_ids=crypto:CRYPTO:BTC&full=1

GET    /api/portfolio/assets
POST   /api/portfolio/assets
PUT    /api/portfolio/assets/{asset_id}
DELETE /api/portfolio/assets/{asset_id}
GET    /api/portfolio/transactions
POST   /api/portfolio/transactions
PUT    /api/portfolio/transactions/{id}
DELETE /api/portfolio/transactions/{id}
GET    /api/portfolio/holdings?category=全部
GET    /api/portfolio/summary?category=全部
GET    /api/portfolio/profit-history
POST   /api/portfolio/import
GET    /api/portfolio/export
```

## 日志与验证

服务端默认打印启动、数据库迁移和 HTTP 请求日志。使用 `RUST_LOG=debug` 获得更详细日志：

```powershell
$env:RUST_LOG="debug"
cargo run -p portfolio-server

cargo fmt --check
cargo check --workspace
```

`data/` 和 `.env` 属于运行时本地文件，已忽略提交。当前 API 默认无认证，仅适合本地或受信任内网使用。
