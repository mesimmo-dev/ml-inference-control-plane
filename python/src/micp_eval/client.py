"""Minimal HTTP client for micp-api. Stdlib only."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from typing import Any


class EngineError(RuntimeError):
    def __init__(self, message: str, status: int | None = None, body: Any = None) -> None:
        super().__init__(message)
        self.status = status
        self.body = body


class MicpClient:
    """Synchronous JSON client for the Axum control-plane API."""

    def __init__(self, base_url: str, timeout_s: float = 15.0) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout_s = timeout_s

    def get(self, path: str) -> tuple[int, Any]:
        return self._request("GET", path, None)

    def post(self, path: str, body: dict[str, Any]) -> tuple[int, Any]:
        return self._request("POST", path, body)

    def _request(self, method: str, path: str, body: dict[str, Any] | None) -> tuple[int, Any]:
        url = f"{self.base_url}{path}"
        data = None if body is None else json.dumps(body).encode("utf-8")
        headers = {"Accept": "application/json"}
        if data is not None:
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout_s) as resp:
                raw = resp.read()
                payload: Any = json.loads(raw.decode("utf-8")) if raw else None
                return int(resp.status), payload
        except urllib.error.HTTPError as exc:
            raw = exc.read()
            try:
                payload = json.loads(raw.decode("utf-8")) if raw else {"error": str(exc)}
            except json.JSONDecodeError:
                payload = {"error": raw.decode("utf-8", errors="replace")}
            return int(exc.code), payload
        except urllib.error.URLError as exc:
            raise EngineError(f"failed to reach {url}: {exc}") from exc
