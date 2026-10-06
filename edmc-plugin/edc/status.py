"""One-line status on the main EDMC window."""
from __future__ import annotations

import tkinter as tk
from typing import Any, Optional

from .common import PLUGIN_NAME, STATUS_EVENT
from .sender import Sender


def create(parent: tk.Frame, sender: Optional[Sender]) -> tuple[tk.Label, tk.Label]:
    """Build the (label, value) pair EDMC expects from plugin_app()."""
    label = tk.Label(parent, text=f"{PLUGIN_NAME}:")
    value = tk.Label(parent, text="idle", anchor=tk.W)

    def refresh(_event: Any = None) -> None:
        if sender:
            value["text"] = sender.status

    # Worker threads must not touch Tk; event_generate is the one allowed hop.
    value.bind(STATUS_EVENT, refresh)
    if sender:
        sender.on_status_change = lambda: value.event_generate(STATUS_EVENT, when="tail")
    return label, value
