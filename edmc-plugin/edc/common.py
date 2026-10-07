"""Constants shared across the plugin, and the plugin's logger."""
from __future__ import annotations

import logging
import os

from config import appname  # type: ignore  # provided by EDMC

PLUGIN_NAME = "ed-commander"
__version__ = "1.2.0"

# EDMC logs a plugin under "<appname>.<plugin folder>"; use the same name so our
# lines are attributed to this plugin in EDMarketConnector.log.
_PLUGIN_FOLDER = os.path.basename(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
logger = logging.getLogger(f"{appname}.{_PLUGIN_FOLDER}")

# --- EDMC config keys (unique prefix so we never clash with other plugins) ---
CFG_URL = "edcommander_url"
CFG_TOKEN = "edcommander_token"
CFG_ENABLED = "edcommander_enabled"

# --- Backend ---
DEFAULT_URL = "http://127.0.0.1:8080"
EVENT_PATH = "/api/carrier/event"
MARKET_PATH = "/api/market/event"
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

# The backend only tracks the carrier's own market and accepts-and-drops the
# rest, so by default we don't send (or read Market.json for) station markets.
# Set to False to forward every market the commander opens.
ONLY_CARRIER_MARKETS = True
FLEET_CARRIER_STATION_TYPE = "FleetCarrier"
MARKET_READ_ATTEMPTS = 6    # Market.json may lag the journal line slightly
MARKET_READ_DELAY = 0.5     # seconds between attempts

# --- Sender tuning ---
REQUEST_TIMEOUT = 10        # seconds
BACKOFF_START = 2.0         # seconds
BACKOFF_MAX = 120.0
MAX_PENDING = 1000          # cap on queued events

# --- Tk virtual events: the only safe way for a worker thread to reach the UI ---
STATUS_EVENT = "<<EDCommanderStatus>>"
TEST_EVENT = "<<EDCommanderTestDone>>"
