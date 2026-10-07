"""The settings tab (File > Settings > ed-commander) and its connection test."""
from __future__ import annotations

import threading
import tkinter as tk
from tkinter import ttk
from typing import Any

import requests

import myNotebook as nb  # type: ignore  # provided by EDMC
from config import config  # type: ignore  # provided by EDMC

from .common import (EVENT_PATH, HEALTH_PATH, MARKET_PATH, PLUGIN_NAME, TEST_EVENT, TOKEN_HEADER,
                     __version__)
from .settings import Settings, normalise_url

# myNotebook doesn't export the same widgets in every EDMC version (6.1.2 has no
# `Entry`, only `EntryMenu`), so fall back to the plain ttk widget rather than
# letting an AttributeError take the whole settings tab down.
_Entry = getattr(nb, "EntryMenu", None) or getattr(nb, "Entry", None) or ttk.Entry
_Button = getattr(nb, "Button", None) or ttk.Button


def error_frame(parent: nb.Notebook) -> nb.Frame:
    """Shown instead of the settings if building them failed (EDMC drops the tab silently otherwise)."""
    frame = nb.Frame(parent)
    nb.Label(frame, text=f"{PLUGIN_NAME}: settings failed to load.\n"
                         "See EDMarketConnector.log for details.").grid(padx=10, pady=10)
    return frame


class PrefsPane:
    def __init__(self, parent: nb.Notebook, settings: Settings) -> None:
        self.settings = settings
        url, token, enabled = settings.snapshot()
        self.url_var = tk.StringVar(value=url)
        self.token_var = tk.StringVar(value=token)
        self.enabled_var = tk.IntVar(value=int(enabled))
        self.test_result = tk.StringVar(value="")

        self._test_lock = threading.Lock()
        self._test_running = False
        self._test_outcome = ""

        self.frame = nb.Frame(parent)
        self.frame.bind(TEST_EVENT, self._on_test_done)
        self._build()

    # -- layout ------------------------------------------------------------
    def _build(self) -> None:
        f = self.frame
        f.columnconfigure(1, weight=1)
        pad = {"padx": 10, "pady": 4}

        nb.Label(f, text=f"{PLUGIN_NAME} {__version__}").grid(row=0, column=0, columnspan=2, sticky=tk.W, **pad)
        ttk.Separator(f, orient=tk.HORIZONTAL).grid(row=1, column=0, columnspan=2, sticky=tk.EW, padx=10, pady=4)

        nb.Checkbutton(f, text="Send carrier events", variable=self.enabled_var).grid(
            row=2, column=0, columnspan=2, sticky=tk.W, **pad)

        nb.Label(f, text="API URL").grid(row=3, column=0, sticky=tk.W, **pad)
        _Entry(f, textvariable=self.url_var).grid(row=3, column=1, sticky=tk.EW, **pad)

        nb.Label(f, text="Token").grid(row=4, column=0, sticky=tk.W, **pad)
        _Entry(f, textvariable=self.token_var, show="*").grid(row=4, column=1, sticky=tk.EW, **pad)

        _Button(f, text="Test connection", command=self.run_test).grid(row=5, column=0, sticky=tk.W, **pad)
        nb.Label(f, textvariable=self.test_result).grid(row=5, column=1, sticky=tk.W, **pad)

        nb.Label(f, text=f"Carrier events -> <API URL>{EVENT_PATH}\n"
                         f"Carrier market  -> <API URL>{MARKET_PATH}\n"
                         f"The token is sent in the {TOKEN_HEADER} header.").grid(
            row=6, column=0, columnspan=2, sticky=tk.W, **pad)

    # -- saving ------------------------------------------------------------
    def apply(self) -> None:
        """EDMC's prefs_changed: persist, and the worker picks the values up on its next attempt."""
        self.settings.save(self.url_var.get(), self.token_var.get(), bool(self.enabled_var.get()))

    # -- connection test ---------------------------------------------------
    def run_test(self) -> None:
        """Button handler (main thread): read the widgets, then test off-thread."""
        with self._test_lock:
            if self._test_running:
                return
            self._test_running = True
        self.test_result.set("Testing...")
        threading.Thread(
            target=self._test_worker,
            args=(normalise_url(self.url_var.get()), self.token_var.get().strip()),
            name=f"{PLUGIN_NAME}-test",
            daemon=True,
        ).start()

    def _test_worker(self, base: str, token: str) -> None:
        """Runs in a thread - no Tk access except the final event_generate.

        Checks reachability (healthz), then the token by POSTing an invalid body:
        a bad token should be answered 401/403, a valid one gets past auth and is
        rejected for the empty body (400/422), so nothing is ever stored.
        """
        headers = {TOKEN_HEADER: token} if token else {}
        try:
            h = requests.get(base + HEALTH_PATH, timeout=5)
            if not h.ok:
                outcome = f"Server reachable but healthz returned {h.status_code}"
            else:
                r = requests.post(base + EVENT_PATH, json={}, headers=headers, timeout=5)
                if r.status_code in (401, 403):
                    outcome = "Server OK, but token was rejected"
                elif r.status_code in (400, 415, 422):
                    outcome = "OK - server reachable, token accepted"
                else:
                    outcome = f"Server reachable (event endpoint returned {r.status_code})"
        except requests.RequestException as e:
            outcome = f"Failed: {type(e).__name__}"

        with self._test_lock:
            self._test_outcome = outcome
            self._test_running = False

        if not config.shutting_down:
            try:
                self.frame.event_generate(TEST_EVENT, when="tail")
            except (tk.TclError, RuntimeError):  # settings window closed / Tk not running
                pass

    def _on_test_done(self, _event: Any = None) -> None:
        """Main thread: show the result."""
        with self._test_lock:
            self.test_result.set(self._test_outcome)
