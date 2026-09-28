use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;

use crate::{app_state::AppState, db};

/// The current carrier table.
pub async fn get_current(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    db::carriers::get_current(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// The carrier table for a specific `carrier_id`.
pub async fn get_by_id(
    Path(carrier_id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    db::carriers::get(&state.db_pool, carrier_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
