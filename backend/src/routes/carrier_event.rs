use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use crate::{
    app_state::AppState,
    db::{carriers, log_event},
    journal_definitions::CarrierEvent,
};

/// Ingest one journal event, apply it to the carrier table, and publish the
/// refreshed table to live subscribers.
pub async fn post(
    State(state): State<AppState>,
    Json(payload): Json<CarrierEvent>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    // 1. Apply the event to the carrier state table first. `carrier_events.carrier`
    //    is a foreign key onto `carriers`, so the carriers row has to exist before
    //    the history row that references it is written.
    let carrier_id = carriers::apply(&state.db_pool, &payload)
        .await
        .map_err(internal)?;

    // 2. Append the raw event to the history.
    let event_id = log_event::insert(&state.db_pool, &payload, carrier_id)
        .await
        .map_err(internal)?;

    // 3. Stamp which history row produced the current state, then publish the
    //    refreshed table. Events that could not be attributed to a carrier
    //    change no state, so there is nothing to publish.
    if let Some(id) = carrier_id {
        carriers::set_last_event(&state.db_pool, id, event_id)
            .await
            .map_err(internal)?;

        if let Some(snapshot) = carriers::get(&state.db_pool, id).await.map_err(internal)? {
            let _ = state.tx.send(snapshot);
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "event_id": event_id, "carrier_id": carrier_id })),
    ))
}

fn internal(err: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
