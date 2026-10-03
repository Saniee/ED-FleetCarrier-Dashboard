"""
CarrierSync - EDMC plugin that forwards Fleet Carrier journal events to an HTTP endpoint.

Settings (EDMC > File > Settings > CarrierSync):
  * API URL - base URL of the backend, e.g. http://127.0.0.1:8080
              (events are POSTed to <API URL>/api/carrier/event)
  * Token   - sent in the X-Ingest-Token header (omit to send no header)

Events are queued and sent from a background thread, so a slow or offline
backend never blocks EDMC. Failed sends are retried with backoff; anything
still pending when EDMC exits is saved to pending.json and resent next launch.
"""
from __future__ import annotations

import json
import logging
import os
import queue
import threading
import tkinter as tk
from tkinter import ttk
from typing import Any, Optional

import requests

import myNotebook as nb  # type: ignore  # provided by EDMC
from config import appname, config  # type: ignore  # provided by EDMC

PLUGIN_NAME = "ed-commander"
PLUGIN_VERSION = "1.0.0"

logger = logging.getLogger(f"{appname}.{os.path.basename(os.path.dirname(__file__))}")

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #
CFG_URL = "carriersync_url"
CFG_TOKEN = "carriersync_token"
CFG_ENABLED = "carriersync_enabled"

DEFAULT_URL = "http://127.0.0.1:8080"
EVENT_PATH = "/api/carrier/event"
HEALTH_PATH = "/api/healthz"
TOKEN_HEADER = "X-Ingest-Token"

# Must match the backend's CarrierEvent enum.
CARRIER_EVENTS = frozenset({
    "CarrierJump", "CarrierBuy", "CarrierStats", "CarrierJumpRequest",
    "CarrierJumpCancelled", "CarrierBankTransfer", "CarrierDepositFuel",
    "CarrierCrewServices", "CarrierFinance", "CarrierShipPack",
    "CarrierModulePack", "CarrierTradeOrder", "CarrierDockingPermission",
    "CarrierNameChange", "CarrierLocation",
})

REQUEST_TIMEOUT = 10        # seconds
BACKOFF_START = 2.0         # seconds
BACKOFF_MAX = 120.0
MAX_PENDING = 1000          # cap on queued events
STATUS_EVENT = "<<CarrierSyncStatus>>"


# --------------------------------------------------------------------------- #
# Settings (cached so the worker thread never touches EDMC's config object)
# --------------------------------------------------------------------------- #
class Settings:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self.url = DEFAULT_URL
        self.token = ""
        self.enabled = True

    def load(self) -> None:
        with self._lock:
            self.url = normalise_url(config.get_str(CFG_URL, default=DEFAULT_URL) or DEFAULT_URL)
            self.token = (config.get_str(CFG_TOKEN, default="") or "").strip()
            self.enabled = config.get_bool(CFG_ENABLED, default=True)

    def snapshot(self) -> tuple[str, str, bool]:
        with self._lock:
            return self.url, self.token, self.enabled


def normalise_url(raw: str) -> str:
    """Accept either a base URL or the full endpoint; return the base URL."""
    url = raw.strip().rstrip("/")
    if url.endswith(EVENT_PATH):
        url = url[: -len(EVENT_PATH)]
    return url


# --------------------------------------------------------------------------- #
# Background sender
# --------------------------------------------------------------------------- #
class Sender:
    def __init__(self, settings: Settings, pending_file: str) -> None:
        self.settings = settings
        self.pending_file = pending_file
        self.q: "queue.Queue[dict[str, Any]]" = queue.Queue(maxsize=MAX_PENDING)
        self._stop = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._session = requests.Session()
        self._session.headers["User-Agent"] = f"EDMC-{PLUGIN_NAME}/{PLUGIN_VERSION}"
        self._status_lock = threading.Lock()
        self._status = "idle"
        self.on_status_change: Optional[Any] = None  # called from worker thread
        self._in_flight: Optional[dict[str, Any]] = None

    # -- status ------------------------------------------------------------
    @property
    def status(self) -> str:
        with self._status_lock:
            return self._status

    def _set_status(self, text: str) -> None:
        with self._status_lock:
            self._status = text
        if self.on_status_change:
            try:
                self.on_status_change()
            except Exception:  # UI may already be gone during shutdown
                pass

    # -- lifecycle ---------------------------------------------------------
    def start(self) -> None:
        self._load_pending()
        self._thread = threading.Thread(target=self._run, name=f"{PLUGIN_NAME}-sender", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=REQUEST_TIMEOUT + 2)
        self._save_pending()

    def submit(self, entry: dict[str, Any]) -> None:
        try:
            self.q.put_nowait(entry)
        except queue.Full:
            logger.warning("Queue full (%d); dropping %s", MAX_PENDING, entry.get("event"))
            return
        self._set_status(f"queued ({self.q.qsize()})")

    # -- persistence -------------------------------------------------------
    def _load_pending(self) -> None:
        try:
            with open(self.pending_file, encoding="utf-8") as f:
                items = json.load(f)
            for item in items[:MAX_PENDING]:
                self.q.put_nowait(item)
            if items:
                logger.info("Restored %d pending event(s)", len(items))
            os.remove(self.pending_file)
        except FileNotFoundError:
            pass
        except Exception:
            logger.exception("Could not restore pending events")

    def _save_pending(self) -> None:
        items = [self._in_flight] if self._in_flight else []
        while True:
            try:
                items.append(self.q.get_nowait())
            except queue.Empty:
                break
        if not items:
            return
        try:
            with open(self.pending_file, "w", encoding="utf-8") as f:
                json.dump(items, f)
            logger.info("Saved %d pending event(s)", len(items))
        except Exception:
            logger.exception("Could not save pending events")

    # -- worker ------------------------------------------------------------
    def _run(self) -> None:
        backoff = BACKOFF_START
        while not self._stop.is_set():
            if self._in_flight is None:
                try:
                    self._in_flight = self.q.get(timeout=0.5)
                except queue.Empty:
                    continue

            url, token, enabled = self.settings.snapshot()
            if not enabled or not url:
                # Paused: leave the event in flight and check again shortly.
                self._set_status("disabled")
                self._stop.wait(1.0)
                continue

            outcome, detail = self._post(url, token, self._in_flight)

            if outcome == "ok":
                self._in_flight = None
                backoff = BACKOFF_START
                self._set_status("ok" if self.q.empty() else f"sending ({self.q.qsize()} left)")
            elif outcome == "drop":
                logger.error("Server rejected %s (%s); dropping", self._in_flight.get("event"), detail)
                self._in_flight = None
                self._set_status(f"rejected: {detail}")
            else:  # "retry" - network error, 5xx, or auth problem the user can fix
                self._set_status(f"{detail} - retrying in {int(backoff)}s")
                self._stop.wait(backoff)
                backoff = min(backoff * 2, BACKOFF_MAX)

    def _post(self, base_url: str, token: str, entry: dict[str, Any]) -> tuple[str, str]:
        headers = {TOKEN_HEADER: token} if token else {}
        try:
            r = self._session.post(base_url + EVENT_PATH, json=entry, headers=headers, timeout=REQUEST_TIMEOUT)
        except requests.RequestException as e:
            logger.warning("POST failed: %s", e)
            return "retry", "offline"

        if r.ok:
            return "ok", str(r.status_code)
        if r.status_code in (401, 403):
            return "retry", f"auth failed ({r.status_code}) - check token"
        if r.status_code in (408, 425, 429) or r.status_code >= 500:
            return "retry", f"server error {r.status_code}"
        return "drop", f"HTTP {r.status_code}"


# --------------------------------------------------------------------------- #
# Plugin state
# --------------------------------------------------------------------------- #
settings = Settings()
sender: Optional[Sender] = None
_status_label: Optional[tk.Label] = None

# Prefs-window widgets (tk variables)
_url_var: Optional[tk.StringVar] = None
_token_var: Optional[tk.StringVar] = None
_enabled_var: Optional[tk.IntVar] = None
_test_result: Optional[tk.StringVar] = None


# --------------------------------------------------------------------------- #
# EDMC hooks
# --------------------------------------------------------------------------- #
def plugin_start3(plugin_dir: str) -> str:
    global sender
    settings.load()
    sender = Sender(settings, os.path.join(plugin_dir, "pending.json"))
    sender.start()
    logger.info("%s %s started", PLUGIN_NAME, PLUGIN_VERSION)
    return PLUGIN_NAME


def plugin_stop() -> None:
    if sender:
        sender.stop()
    logger.info("%s stopped", PLUGIN_NAME)


def plugin_app(parent: tk.Frame) -> tk.Widget:
    """Small status line on the main EDMC window."""
    global _status_label
    frame = tk.Frame(parent)
    tk.Label(frame, text=f"{PLUGIN_NAME}:").grid(row=0, column=0, sticky=tk.W)
    _status_label = tk.Label(frame, text="idle", anchor=tk.W)
    _status_label.grid(row=0, column=1, sticky=tk.W)

    def refresh(_event: Any = None) -> None:
        if sender and _status_label:
            _status_label["text"] = sender.status

    # Worker thread must not touch Tk directly; a virtual event is the safe hop.
    frame.bind(STATUS_EVENT, refresh)
    if sender:
        sender.on_status_change = lambda: frame.event_generate(STATUS_EVENT, when="tail")
    return frame


def journal_entry(cmdr: str, is_beta: bool, system: str, station: str,
                  entry: dict[str, Any], state: dict[str, Any]) -> None:
    if is_beta or sender is None:
        return  # don't mix beta-game data into live carrier state
    if entry.get("event") in CARRIER_EVENTS:
        sender.submit(dict(entry))


# --------------------------------------------------------------------------- #
# Settings UI
# --------------------------------------------------------------------------- #
def plugin_prefs(parent: nb.Notebook, cmdr: str, is_beta: bool) -> nb.Frame:
    global _url_var, _token_var, _enabled_var, _test_result
    url, token, enabled = settings.snapshot()
    _url_var = tk.StringVar(value=url)
    _token_var = tk.StringVar(value=token)
    _enabled_var = tk.IntVar(value=int(enabled))
    _test_result = tk.StringVar(value="")

    frame = nb.Frame(parent)
    frame.columnconfigure(1, weight=1)
    pad = {"padx": 10, "pady": 4}

    nb.Label(frame, text=f"{PLUGIN_NAME} {PLUGIN_VERSION}").grid(row=0, column=0, columnspan=2, sticky=tk.W, **pad)
    ttk.Separator(frame, orient=tk.HORIZONTAL).grid(row=1, column=0, columnspan=2, sticky=tk.EW, padx=10, pady=4)

    nb.Checkbutton(frame, text="Send carrier events", variable=_enabled_var).grid(
        row=2, column=0, columnspan=2, sticky=tk.W, **pad)

    nb.Label(frame, text="API URL").grid(row=3, column=0, sticky=tk.W, **pad)
    nb.Entry(frame, textvariable=_url_var).grid(row=3, column=1, sticky=tk.EW, **pad)

    nb.Label(frame, text="Token").grid(row=4, column=0, sticky=tk.W, **pad)
    nb.Entry(frame, textvariable=_token_var, show="*").grid(row=4, column=1, sticky=tk.EW, **pad)

    nb.Button(frame, text="Test connection", command=_run_test).grid(row=5, column=0, sticky=tk.W, **pad)
    nb.Label(frame, textvariable=_test_result).grid(row=5, column=1, sticky=tk.W, **pad)

    nb.Label(frame, text=f"Events are POSTed to <API URL>{EVENT_PATH}\n"
                         f"The token is sent in the {TOKEN_HEADER} header.").grid(
        row=6, column=0, columnspan=2, sticky=tk.W, **pad)
    return frame


def prefs_changed(cmdr: str, is_beta: bool) -> None:
    if _url_var is None or _token_var is None or _enabled_var is None:
        return
    config.set(CFG_URL, normalise_url(_url_var.get()) or DEFAULT_URL)
    config.set(CFG_TOKEN, _token_var.get().strip())
    config.set(CFG_ENABLED, bool(_enabled_var.get()))
    settings.load()  # worker picks up the new values on its next attempt


def _run_test() -> None:
    """Check reachability (healthz), then the token by POSTing an invalid body.

    A bad token should be answered 401/403; a valid one gets past auth and is
    rejected for the empty body (400/422), so nothing is ever stored.
    """
    if _url_var is None or _token_var is None or _test_result is None:
        return
    base = normalise_url(_url_var.get())
    token = _token_var.get().strip()
    headers = {TOKEN_HEADER: token} if token else {}
    try:
        h = requests.get(base + HEALTH_PATH, timeout=5)
        if not h.ok:
            _test_result.set(f"Server reachable but healthz returned {h.status_code}")
            return
        r = requests.post(base + EVENT_PATH, json={}, headers=headers, timeout=5)
    except requests.RequestException as e:
        _test_result.set(f"Failed: {type(e).__name__}")
        return

    if r.status_code in (401, 403):
        _test_result.set("Server OK, but token was rejected")
    elif r.status_code in (400, 415, 422):
        _test_result.set("OK - server reachable, token accepted")
    else:
        _test_result.set(f"Server reachable (event endpoint returned {r.status_code})")