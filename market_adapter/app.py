"""Private Python market-data adapter.

It deliberately reuses the proven legacy facade rather than exposing it to the
public internet. Run behind the Rust server or bind it to loopback only.
"""
import json
import logging
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

LEGACY_ROOT = Path(os.getenv("CPM_LEGACY_ROOT", r"E:\fengkai\my_project\crypto_portfolio_manager"))
sys.path.insert(0, str(LEGACY_ROOT))
import crypto_portfolio.market_data as market_data  # noqa: E402
from crypto_portfolio.market_data import fetch_quotes_for_assets, search_asset_suggestions  # noqa: E402

TOKEN = os.getenv("CPM_ADAPTER_TOKEN", "")
logging.basicConfig(
    level=os.getenv("CPM_ADAPTER_LOG_LEVEL", "INFO").upper(),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
LOGGER = logging.getLogger("market_adapter")


def fetch_fx_to_cny_online(currency, allow_default=False):
    """Fetch FX from a live service and fail instead of using default estimates."""
    currency = str(currency or "CNY").upper()
    if currency == "CNY":
        return 1.0, "人民币", False

    cache = getattr(market_data, "_FX_CACHE", {})
    cached = cache.get(currency)
    if cached:
        import time

        ttl = getattr(market_data, "FX_CACHE_SECONDS", 600)
        if time.time() - cached["cached_at"] <= ttl:
            return cached["rate"], cached["source"], cached["estimated"]

    try:
        rate, source = market_data.fetch_fx_from_open_er(currency)
    except Exception as exc:
        raise ValueError(f"无法联网获取 {currency}/CNY 汇率: {exc}") from exc

    import time

    cache[currency] = {
        "rate": rate,
        "source": source,
        "estimated": False,
        "cached_at": time.time(),
    }
    market_data._FX_CACHE = cache
    return rate, source, False


market_data.fetch_fx_to_cny = fetch_fx_to_cny_online


def category_for_adapter(category):
    """Translate public API category codes into the legacy adapter vocabulary."""
    value = str(category or "all").strip().lower()
    mapping = {
        "all": getattr(market_data, "CATEGORY_ALL", "全部"),
        "crypto": getattr(market_data, "CATEGORY_CRYPTO", "加密货币"),
        "fund": getattr(market_data, "CATEGORY_FUND", "基金"),
        "stock": getattr(market_data, "CATEGORY_STOCK", "股票"),
    }
    return mapping.get(value, category)


def normalize_asset(asset):
    """Keep asset IDs stable while letting callers use English category codes."""
    if not isinstance(asset, dict):
        return asset
    normalized = dict(asset)
    asset_id = str(normalized.get("asset_id") or "")
    kind = asset_id.split(":", 1)[0]
    normalized["category"] = category_for_adapter(kind or normalized.get("category"))
    return normalized

class Handler(BaseHTTPRequestHandler):
    """Loopback-only adapter endpoints consumed by the Rust service."""

    def _authorized(self):
        """Allow unauthenticated development or validate the shared adapter token."""
        return not TOKEN or self.headers.get("X-Adapter-Token") == TOKEN

    def _json(self, body, status=200):
        """Write one UTF-8 JSON response with a correct content length."""
        raw = json.dumps(body, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers(); self.wfile.write(raw)

    def do_POST(self):
        """Fetch all requested quotes as one batch to preserve collector atomicity."""
        if not self._authorized():
            LOGGER.warning("rejected unauthorized %s request", self.path)
            return self._json({"error":"unauthorized"}, 401)
        if self.path != "/v1/quotes": return self._json({"error":"not found"}, 404)
        try:
            size = int(self.headers.get("Content-Length", 0))
            assets = json.loads(self.rfile.read(size) or b"{}").get("assets", [])
            assets = [normalize_asset(asset) for asset in assets]
            quotes, errors = fetch_quotes_for_assets(assets, max_workers=24)
            LOGGER.info("quote batch completed: requested=%d succeeded=%d failed=%d", len(assets), len(quotes), len(errors))
            self._json({"quotes": quotes, "errors": errors})
        except Exception as exc:
            LOGGER.exception("quote batch failed")
            self._json({"error": str(exc)}, 500)

    def do_GET(self):
        """Proxy legacy asset search without exposing the legacy project publicly."""
        if not self._authorized():
            LOGGER.warning("rejected unauthorized %s request", self.path)
            return self._json({"error":"unauthorized"}, 401)
        if not self.path.startswith("/v1/search"): return self._json({"error":"not found"}, 404)
        from urllib.parse import parse_qs, urlparse
        q = parse_qs(urlparse(self.path).query)
        query = q.get("query", [""])[0]
        category = category_for_adapter(q.get("category", ["all"])[0])
        market = q.get("market", [None])[0]
        items = search_asset_suggestions(query, category, market)
        LOGGER.info("asset search completed: query=%r results=%d", query, len(items))
        self._json({"items": items})

    def log_message(self, fmt, *args):
        """Route standard HTTP access logs through the structured logger."""
        LOGGER.info("%s - %s", self.address_string(), fmt % args)

if __name__ == "__main__":
    # Bind to loopback by default so only the Rust server can reach this adapter.
    host = os.getenv("CPM_ADAPTER_BIND", "127.0.0.1")
    port = int(os.getenv("CPM_ADAPTER_PORT", "8786"))
    LOGGER.info("market adapter listening on http://%s:%d", host, port)
    ThreadingHTTPServer((host, port), Handler).serve_forever()
