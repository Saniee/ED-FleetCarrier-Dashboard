"""User settings, cached so worker threads never touch EDMC's config object."""
from __future__ import annotations

import threading

from config import config  # type: ignore  # provided by EDMC

from .common import CFG_ENABLED, CFG_TOKEN, CFG_URL, DEFAULT_URL, EVENT_PATH, MARKET_PATH, logger


def normalise_url(raw: str) -> str:
    """Accept either a base URL or a full endpoint; return the base URL."""
    url = raw.strip().rstrip("/")
    for path in (EVENT_PATH, MARKET_PATH):
        if url.endswith(path):
            url = url[: -len(path)]
    return url


class Settings:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self.url = DEFAULT_URL
        self.token = ""
        self.enabled = True

    def load(self) -> None:
        """Read from EDMC's config. A bad stored value must never stop the plugin loading."""
        try:
            url = normalise_url(config.get_str(CFG_URL, default=DEFAULT_URL) or DEFAULT_URL)
            token = (config.get_str(CFG_TOKEN, default="") or "").strip()
            enabled = config.get_bool(CFG_ENABLED, default=True)
        except Exception:
            logger.exception("Could not read settings; using defaults")
            url, token, enabled = DEFAULT_URL, "", True
        with self._lock:
            self.url, self.token, self.enabled = url, token, enabled

    def save(self, url: str, token: str, enabled: bool) -> None:
        """Write to EDMC's config, then refresh the cache the worker reads."""
        config.set(CFG_URL, normalise_url(url) or DEFAULT_URL)
        config.set(CFG_TOKEN, token.strip())
        config.set(CFG_ENABLED, bool(enabled))
        self.load()

    def snapshot(self) -> tuple[str, str, bool]:
        with self._lock:
            return self.url, self.token, self.enabled
