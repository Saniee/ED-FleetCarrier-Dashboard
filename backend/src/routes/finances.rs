//! Owner finances: balance history by upkeep week, and system coordinates for
//! the route tritium estimate. History is the owner's alone; the current
//! balance and upkeep are on the carrier row like the rest of its state.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app_state::AppState,
    auth::AuthUser,
    db::finances as db,
    edsm,
    error::internal,
    finances,
};

const DEFAULT_WEEKS: usize = 12;
const MAX_WEEKS: usize = 52;
const MAX_TRANSFERS: i64 = 50;

#[derive(Deserialize)]
pub struct HistoryQuery {
    weeks: Option<usize>,
}

pub async fn history(
    Path(callsign): Path<String>,
    Query(q): Query<HistoryQuery>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let carrier_id = super::webhooks::owned(&state, &callsign, user.id).await?;
    let weeks = q.weeks.unwrap_or(DEFAULT_WEEKS).clamp(1, MAX_WEEKS);
    let now = Utc::now();
    let since = finances::week_start(now) - Duration::weeks(weeks as i64 - 1);

    let snapshots = db::snapshots(&state.db_pool, carrier_id, since).await.map_err(internal)?;
    let transfers = db::transfers(&state.db_pool, carrier_id, since, MAX_TRANSFERS).await.map_err(internal)?;

    Ok(Json(json!({
        "weeks": finances::weekly(snapshots, now, weeks),
        "next_upkeep": finances::next_upkeep(now),
        "transfers": transfers
            .into_iter()
            .map(|(at, balance, amount)| json!({ "at": at, "amount": amount, "balance": balance }))
            .collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct SystemQuery {
    name: String,
}

/// Where a system is, from EDSM. Signed-in only, so the site isn't an open
/// EDSM proxy.
pub async fn system(
    Query(q): Query<SystemQuery>,
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let name = q.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    match edsm::fetch_coords(&state.http, name).await {
        Ok(Some((name, coords))) => Ok(Json(json!({ "name": name, "coords": coords }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(err) => {
            eprintln!("EDSM system lookup failed: {err}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}
