use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use crate::{
    app_state::{AppState, Update},
    auth::{Access, Ingest},
    db::{body_names, carriers, log_event},
    edsm,
    journal_definitions::{CarrierEvent, CarrierLocation},
};

/// Ingest one journal event, apply it to the carrier table, and publish the
/// refreshed table to live subscribers.
pub async fn post(
    State(state): State<AppState>,
    ingest: Ingest,
    Json(payload): Json<CarrierEvent>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    // 0. A user token may only write to its own (or an unclaimed) carrier.
    if let Access::Skip = ingest
        .check(&state.db_pool, payload.carrier_id(), is_visitor_event(&payload))
        .await
        .map_err(|s| (s, "carrier belongs to another user".to_string()))?
    {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "stored": false, "reason": "carrier not tracked, or event not attributable to one" })),
        ));
    }

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

    // CarrierStats only ever comes from the owner's own carrier management, so
    // it is the one event that proves ownership: the first user token to post it
    // for an unclaimed carrier claims it. (CarrierBuy is not used: players often
    // start the plugin after buying.) The legacy token never claims.
    if let (Ingest::User(user_id), Some(id), CarrierEvent::CarrierStats(_)) =
        (ingest, carrier_id, &payload)
    {
        carriers::claim(&state.db_pool, id, user_id)
            .await
            .map_err(internal)?;
    }

    // Location events from anyone but the owner are flagged so clients can mark
    // the location as unconfirmed; the owner's own event clears the flag.
    if let (Some(id), true) = (carrier_id, is_visitor_event(&payload)) {
        let reporter = match ingest {
            Ingest::User(user_id) => Some(user_id),
            Ingest::Legacy => None,
        };
        carriers::set_location_unverified(&state.db_pool, id, reporter)
            .await
            .map_err(internal)?;
    }

    // 3. Stamp which history row produced the current state, then publish the
    //    refreshed table. Events that could not be attributed to a carrier
    //    change no state, so there is nothing to publish.
    if let Some(id) = carrier_id {
        // A location carries only a `BodyID`, and `carriers::apply` has just
        // blanked any name that no longer matches it, so fill the name back in
        // before the snapshot goes out.
        if let CarrierEvent::CarrierLocation(location) = &payload {
            resolve_body_name(&state, id, location).await;
        }

        carriers::set_last_event(&state.db_pool, id, event_id)
            .await
            .map_err(internal)?;

        if let Some(snapshot) = carriers::get(&state.db_pool, id).await.map_err(internal)? {
            let _ = state.tx.send(Update::Carrier(id, snapshot));
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "event_id": event_id, "carrier_id": carrier_id })),
    ))
}

/// Events a commander generates merely by being docked at a carrier, so they say
/// nothing about who owns it. Any user may report them for any carrier. Everything
/// else comes from carrier management, which only the owner has.
fn is_visitor_event(event: &CarrierEvent) -> bool {
    matches!(event, CarrierEvent::CarrierJump(_) | CarrierEvent::CarrierLocation(_))
}

/// Give a carrier its body name back after a `CarrierLocation`.
///
/// EDSM is asked at most once per system: the answer is cached in `body_names`,
/// including the empty answer for a system it has never seen. Every failure path
/// leaves the name blank rather than wrong — `carriers::apply` has already
/// cleared the stale one.
async fn resolve_body_name(state: &AppState, carrier_id: i64, location: &CarrierLocation) {
    let (system_address, body_id) = (location.system_address, location.body_id);

    match body_names::get(&state.db_pool, system_address, body_id).await {
        Ok(body_names::Cached::Named { name, body_type }) => {
            set_body(state, carrier_id, Some(&name), body_type.as_deref()).await;
        }
        // Already asked; EDSM has no such body.
        Ok(body_names::Cached::Absent) => {}
        Ok(body_names::Cached::Unknown) => {
            fetch_system(state, carrier_id, system_address, body_id).await;
        }
        Err(err) => eprintln!("body_names lookup failed: {err}"),
    }
}

/// Ask EDSM for the whole system, cache every body it returns, and apply the one
/// this carrier is at.
async fn fetch_system(state: &AppState, carrier_id: i64, system_address: i64, body_id: i64) {
    let bodies = match edsm::fetch_bodies(&state.http, system_address).await {
        Ok(bodies) => bodies,
        Err(err) => {
            // Offline, or EDSM is unhappy. Nothing is cached, so the next
            // location event for this body tries again.
            eprintln!("EDSM lookup for system {system_address} failed: {err}");
            return;
        }
    };

    // Cache the whole system, not just the wanted body: a carrier moves between
    // bodies of one system, and the system is a single call either way.
    for body in &bodies {
        let _ = body_names::insert(
            &state.db_pool,
            system_address,
            body.body_id,
            Some(&body.name),
            body.body_type.as_deref(),
        )
        .await;
    }

    match bodies.iter().find(|b| b.body_id == body_id) {
        Some(body) => set_body(state, carrier_id, Some(&body.name), body.body_type.as_deref()).await,
        // EDSM has never seen this system, so it will never have this body.
        // Record the miss so the next jump through here does not ask again.
        None => {
            let _ = body_names::insert(&state.db_pool, system_address, body_id, None, None).await;
        }
    }
}

async fn set_body(state: &AppState, carrier_id: i64, name: Option<&str>, body_type: Option<&str>) {
    if let Err(err) = carriers::set_body(&state.db_pool, carrier_id, name, body_type).await {
        eprintln!("could not store body name: {err}");
    }
}

fn internal(err: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
