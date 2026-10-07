use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::{app_state::AppState, auth::Viewer, db};

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64
}

fn default_limit() -> i64 {
    10
}

/// Recent events for a carrier addressed by callsign.
pub async fn get_events_by_callsign(
    Path(callsign): Path<String>,
    Query(q): Query<EventsQuery>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let carrier_id = super::carriers::resolve_visible(&state, &callsign, &viewer).await?;
    let limit = q.limit.clamp(1, 100);
    db::log_event::recent(&state.db_pool, carrier_id, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
