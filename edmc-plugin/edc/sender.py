"""Background sender: queue, retry with backoff, and persistence across restarts.

Nothing here touches Tk or EDMC's config, so it is safe to run off the main
thread; the only UI hook is the `on_status_change` callback.
"""
from __future__ import annotations

import json
import os
import queue
import threading
from tkinter import TclError
from typing import Any, Callable, Optional

import requests

import timeout_session  # type: ignore  # provided by EDMC
from config import config  # type: ignore  # provided by EDMC

from . import market
from .common import (BACKOFF_MAX, BACKOFF_START, EVENT_PATH, MARKET_PATH, MAX_PENDING,
                     PLUGIN_NAME, REQUEST_TIMEOUT, TOKEN_HEADER, logger)
from .settings import Settings


class Sender:
    def __init__(self, settings: Settings, pending_file: str) -> None:
        self.settings = settings
        self.pending_file = pending_file
        # Each job is {"path": <endpoint path>, "body": <JSON payload>}.
        self.q: "queue.Queue[dict[str, Any]]" = queue.Queue(maxsize=MAX_PENDING)
        self._stop = threading.Event()
        self._thread: Optional[threading.Thread] = None
        self._session = timeout_session.new_session()
        self._in_flight: Optional[dict[str, Any]] = None
        self._status_lock = threading.Lock()
        self._status = "idle"
        self.on_status_change: Optional[Callable[[], None]] = None  # called from worker threads

    # -- status ------------------------------------------------------------
    @property
    def status(self) -> str:
        with self._status_lock:
            return self._status

    def _set_status(self, text: str) -> None:
        with self._status_lock:
            self._status = text
        cb = self.on_status_change
        if cb and not config.shutting_down:  # property, not a function
            try:
                cb()
            except (TclError, RuntimeError):  # widget gone / Tk not running
                pass

    # -- lifecycle ---------------------------------------------------------
    def start(self) -> None:
        self._load_pending()
        self._thread = threading.Thread(target=self._run, name=f"{PLUGIN_NAME}-sender", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self.on_status_change = None  # no UI callbacks once we're shutting down
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=REQUEST_TIMEOUT + 2)
        self._save_pending()

    # -- submitting --------------------------------------------------------
    def submit(self, path: str, body: dict[str, Any]) -> None:
        try:
            self.q.put_nowait({"path": path, "body": body})
        except queue.Full:
            logger.warning("Queue full (%d); dropping %s", MAX_PENDING, body.get("event"))
            return
        self._set_status(f"queued ({self.q.qsize()})")

    def submit_market(self, entry: dict[str, Any], journal_folder: Optional[str]) -> None:
        """Queue a Market event once its commodity list has been merged in (off-thread)."""
        threading.Thread(target=self._merge_and_submit_market, args=(entry, journal_folder),
                         name=f"{PLUGIN_NAME}-market", daemon=True).start()

    def _merge_and_submit_market(self, entry: dict[str, Any], journal_folder: Optional[str]) -> None:
        body = market.read_market(entry, journal_folder, self._stop)
        if body is not None:
            self.submit(MARKET_PATH, body)
        elif not self._stop.is_set():
            self._set_status("market: Market.json not ready")

    # -- persistence -------------------------------------------------------
    def _load_pending(self) -> None:
        try:
            with open(self.pending_file, encoding="utf-8") as f:
                items = json.load(f)
            for item in items[:MAX_PENDING]:
                if "path" not in item:  # saved by v1.0.x: a bare carrier event
                    item = {"path": EVENT_PATH, "body": item}
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
                logger.error("Server rejected %s (%s); dropping", self._in_flight["body"].get("event"), detail)
                self._in_flight = None
                self._set_status(f"rejected: {detail}")
            else:  # "retry" - network error, 5xx, or auth problem the user can fix
                self._set_status(f"{detail} - retrying in {int(backoff)}s")
                self._stop.wait(backoff)
                backoff = min(backoff * 2, BACKOFF_MAX)

    def _post(self, base_url: str, token: str, job: dict[str, Any]) -> tuple[str, str]:
        headers = {TOKEN_HEADER: token} if token else {}
        try:
            r = self._session.post(base_url + job["path"], json=job["body"], headers=headers,
                                   timeout=REQUEST_TIMEOUT)
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
