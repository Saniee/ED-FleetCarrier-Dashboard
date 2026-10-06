"""Fleet carrier market: merge the journal's `Market` line with Market.json.

The journal line carries only the header (MarketID, StationName, ...); the
commodity list is written to Market.json in the journal folder.
"""
from __future__ import annotations

import json
import threading
from pathlib import Path
from typing import Any, Optional

from config import config  # type: ignore  # provided by EDMC

from .common import MARKET_READ_ATTEMPTS, MARKET_READ_DELAY, logger

try:
    from monitor import monitor  # type: ignore  # provided by EDMC
except ImportError:  # keep the plugin loadable if EDMC moves it
    monitor = None


def journal_dir() -> Optional[str]:
    """The journal folder EDMC is watching (where Market.json is written)."""
    try:
        d = getattr(monitor, "currentdir", None) if monitor else None
        d = d or config.get_str("journaldir") or getattr(config, "default_journal_dir", None)
    except Exception:
        logger.exception("Could not determine journal folder")
        return None
    return str(d) if d else None


def read_market(entry: dict[str, Any], folder: Optional[str],
                stop: threading.Event) -> Optional[dict[str, Any]]:
    """Return the full Market event (header + `Items`), or None if unavailable.

    Blocks for up to MARKET_READ_ATTEMPTS * MARKET_READ_DELAY, so call it off
    the main thread. Market.json is overwritten whenever any market is opened,
    so it is only used if it is for this MarketID and not older than the
    journal line. An empty `Items` list is valid (a carrier with no orders) and
    is returned as such so the backend clears the stored market.
    """
    if not folder:
        logger.warning("Journal folder unknown; cannot read Market.json")
        return None
    path = Path(folder) / "Market.json"
    for _ in range(MARKET_READ_ATTEMPTS):
        try:
            with path.open("rb") as f:
                data = json.load(f)
        except (OSError, ValueError):  # not there yet / half-written
            data = None
        if (isinstance(data, dict)
                and data.get("MarketID") == entry.get("MarketID")
                and str(data.get("timestamp", "")) >= str(entry.get("timestamp", ""))):
            body = {**entry, **data}  # Market.json repeats the header and adds Items
            body["Items"] = data.get("Items") or []
            return body
        if stop.wait(MARKET_READ_DELAY):  # shutting down
            return None
    logger.warning("Market.json never matched MarketID %s; skipping this market", entry.get("MarketID"))
    return None
