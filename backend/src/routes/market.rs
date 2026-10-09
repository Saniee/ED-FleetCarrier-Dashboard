use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    app_state::{AppState, Update},
    auth::{Access, Ingest, Viewer},
    db::{carriers, commodities, market},
    error::{internal, internal_msg},
    journal_definitions::MarketEvent,
};

/// The `{ event, commodities }` envelope for a carrier's market, or `None` if no
/// `Market` event has been seen. Assembled here because the two halves live in
/// separate tables owned by separate modules.
pub async fn snapshot(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<Value>> {
    let Some(event) = market::get(pool, carrier_id).await? else {
        return Ok(None);
    };

    let commodities = commodities::list(pool, carrier_id).await?;

    Ok(Some(json!({ "event": event, "commodities": commodities })))
}

/// Ingest one `Market` event and publish the refreshed market. Other stations'
/// markets are accepted and dropped, not rejected: the plugin forwards every
/// market the commander opens.
pub async fn post(
    State(state): State<AppState>,
    ingest: Ingest,
    Json(payload): Json<MarketEvent>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    if payload.event != "Market" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("expected a Market event, got {:?}", payload.event),
        ));
    }

    if !payload.is_fleet_carrier() {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "stored": false,
                "reason": "not a fleet carrier market",
                "station_type": payload.station_type,
            })),
        ));
    }

    // For a fleet carrier the journal's `MarketID` is its `CarrierID`.
    let carrier_id = payload.market_id;

    // Anyone docked at a carrier gets a `Market` event, and any user may report
    // one: more reporters keep the market fresher.
    if let Access::Skip = ingest
        .check(&state.db_pool, Some(carrier_id), true)
        .await
        .map_err(|s| (s, "forbidden".to_string()))?
    {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "stored": false, "reason": "carrier not tracked" })),
        ));
    }

    // The market's foreign key points at `carriers`, so make sure a row exists —
    // the market can be the first event we ever see for a carrier.
    carriers::ensure(&state.db_pool, carrier_id)
        .await
        .map_err(internal_msg)?;

    // Header and commodity list are two halves of one snapshot: write them
    // together so a failure cannot leave them disagreeing.
    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    market::upsert(&mut tx, &payload).await.map_err(internal_msg)?;
    commodities::replace(&mut tx, carrier_id, &payload.items)
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;

    if let Some(snapshot) = snapshot(&state.db_pool, carrier_id).await.map_err(internal_msg)? {
        let _ = state.tx.send(Update::Market(carrier_id, snapshot));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "stored": true,
            "carrier_id": carrier_id,
            "commodities": payload.items.len(),
        })),
    ))
}

pub async fn get_by_callsign(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Value>, StatusCode> {
    let carrier_id = super::carriers::resolve_visible(&state, &callsign, &viewer).await?;
    snapshot(&state.db_pool, carrier_id)
        .await
        .map_err(internal)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

