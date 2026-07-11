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
from crypto_portfolio.market_data import fetch_quotes_for_assets, search_asset_suggestions  # noqa: E402

TOKEN = os.getenv("CPM_ADAPTER_TOKEN", "")
logging.basicConfig(
    level=os.getenv("CPM_ADAPTER_LOG_LEVEL", "INFO").upper(),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
LOGGER = logging.getLogger("market_adapter")

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
        category = q.get("category", ["全部"])[0]
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
