#![allow(dead_code)]

use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::db::carriers::CarrierSnapshot;

/// The `{ event, commodities }` envelope for a carrier's market, as published to
/// subscribers and served by the REST read.
pub type MarketSnapshot = Value;

/// One message on the dashboard stream. Carrier and market updates share a
/// channel, tagged `carrier` / `market`, so the frontend needs one connection.
#[derive(Clone, Debug)]
pub enum Update {
    /// (carrier_id, snapshot); the id lets per-carrier streams filter.
    Carrier(i64, CarrierSnapshot),
    Market(i64, MarketSnapshot),
}

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    /// Shared HTTP client, currently only for the EDSM body-name lookup. Built
    /// once so its connection pool and TLS state are reused.
    pub http: reqwest::Client,
    pub tx: broadcast::Sender<Update>,
    /// Legacy shared ingest secret from the `TOKEN` env var. Per-user API
    /// tokens are checked first; this is the fallback for existing installs.
    /// `None` disables the legacy fallback.
    pub legacy_token: Option<String>,
    /// Accept ingest with no token at all (`ALLOW_ANON_INGEST=1`). Dev only.
    pub anon_ingest: bool,
    /// Whether `POST /api/auth/register` creates accounts (`ALLOW_REGISTRATION`,
    /// on unless set to `0` / `false`).
    pub registration_open: bool,
    /// Per-IP request limits (`RATE_LIMIT`, on unless set to `0` / `false`).
    pub rate_limit: bool,
}
