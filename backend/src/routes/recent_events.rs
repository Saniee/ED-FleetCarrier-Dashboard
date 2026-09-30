use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::{app_state::AppState, db};

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64
}

fn default_limit() -> i64 {
    10
}

pub async fn get_events(
    Path(carrier_id): Path<i64>,
    Query(q): Query<EventsQuery>,
    State(state): State<AppState>
) -> Result<Json<Vec<Value>>, StatusCode> {
    let limit = q.limit.clamp(1, 100);
    db::log_event::recent(&state.db_pool, carrier_id, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}