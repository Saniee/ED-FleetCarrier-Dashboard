"""
ed-commander - EDMC plugin that forwards Fleet Carrier data to an HTTP endpoint.

  edc/common.py    constants and the logger
  edc/settings.py  API URL / token / enabled, cached for worker threads
  edc/sender.py    queue, retry with backoff, pending.json persistence
  edc/market.py    merges the journal's Market line with Market.json
  edc/prefs.py     the settings tab and the Test connection button
  edc/status.py    the status line on the main window

  * Carrier journal events -> POST <API URL>/api/carrier/event
  * The carrier's market   -> POST <API URL>/api/market/event
"""
from __future__ import annotations

import os
import tkinter as tk
from typing import Any, Optional

import myNotebook as nb  # type: ignore  # provided by EDMC

from .edc import market, prefs, status
from .edc.common import (CARRIER_EVENTS, EVENT_PATH, FLEET_CARRIER_STATION_TYPE,
                         ONLY_CARRIER_MARKETS, PLUGIN_NAME, __version__, logger)
from .edc.prefs import PrefsPane
from .edc.sender import Sender
from .edc.settings import Settings

settings = Settings()
sender: Optional[Sender] = None
_prefs: Optional[PrefsPane] = None


def plugin_start3(plugin_dir: str) -> str:
    global sender
    settings.load()
    sender = Sender(settings, os.path.join(plugin_dir, "pending.json"))
    sender.start()
    logger.info("%s %s started", PLUGIN_NAME, __version__)
    return PLUGIN_NAME


def plugin_stop() -> None:
    if sender:
        sender.stop()
    logger.info("%s stopped", PLUGIN_NAME)


def plugin_app(parent: tk.Frame) -> tuple[tk.Label, tk.Label]:
    return status.create(parent, sender)


def plugin_prefs(parent: nb.Notebook, cmdr: str, is_beta: bool) -> nb.Frame:
    global _prefs
    try:
        _prefs = PrefsPane(parent, settings)
        return _prefs.frame
    except Exception:
        logger.exception("Failed to build settings tab")
        _prefs = None
        return prefs.error_frame(parent)


def prefs_changed(cmdr: str, is_beta: bool) -> None:
    if _prefs is not None:
        _prefs.apply()


def journal_entry(cmdr: str, is_beta: bool, system: str, station: str,
                  entry: dict[str, Any], state: dict[str, Any]) -> None:
    if is_beta or sender is None:
        return  # don't mix beta-game data into live carrier state
    event = entry.get("event")
    if event in CARRIER_EVENTS:
        sender.submit(EVENT_PATH, dict(entry))
    elif event == "Market":
        if ONLY_CARRIER_MARKETS and entry.get("StationType") != FLEET_CARRIER_STATION_TYPE:
            return
        sender.submit_market(dict(entry), market.journal_dir())
