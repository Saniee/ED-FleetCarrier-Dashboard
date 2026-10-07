#![allow(dead_code)]

use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::db::carriers::CarrierSnapshot;

/// The `{ event, commodities }` envelope for a carrier's market, as published to
/// subscribers and served by the REST read.
pub type MarketSnapshot = Value;

/// One message on the dashboard stream.
///
/// Both domains share a single channel so the frontend needs one connection: the
/// stream tags each message with its own event name (`carrier` / `market`), and
/// a subscriber that only cares about one can ignore the other.
#[derive(Clone, Debug)]
pub enum Update {
    /// A carrier's table after an applied carrier event.
    /// (carrier_id, snapshot); the id lets per-carrier streams filter.
    Carrier(i64, CarrierSnapshot),
    /// A carrier's market after an applied `Market` event.
    Market(i64, MarketSnapshot),
}

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    /// Shared HTTP client, currently only for the EDSM body-name lookup. Built
    /// once so its connection pool and TLS state are reused.
    pub http: reqwest::Client,
    /// Fan-out of dashboard updates to live subscribers. Every applied event
    /// publishes a freshly read snapshot here.
    pub tx: broadcast::Sender<Update>,
    /// Legacy shared ingest secret from the `TOKEN` env var. Per-user API
    /// tokens are checked first; this is the fallback for existing installs.
    /// `None` leaves ingest open to token-less requests (dev only).
    pub legacy_token: Option<String>,
}
