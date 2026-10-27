//! Owners' Discord webhooks: the settings endpoints, and the two calls ingest
//! makes around an event to find out what to announce.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app_state::AppState,
    auth::AuthUser,
    db::{carriers, commodities, webhooks},
    discord::{self, Carrier, Delivery, SendError},
    error::{internal, internal_msg},
    journal_definitions::{CarrierEvent, MarketEvent},
    notify::{self, Notice},
};

type ApiError = (StatusCode, String);

/// The carrier behind `callsign`, if `user` owns it.
pub(super) async fn owned(state: &AppState, callsign: &str, user_id: i64) -> Result<i64, StatusCode> {
    match super::carriers::resolve(state, callsign).await? {
        (id, Some(owner)) if owner == user_id => Ok(id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

pub async fn get(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let carrier_id = owned(&state, &callsign, user.id).await?;
    let hook = webhooks::get(&state.db_pool, carrier_id).await.map_err(internal)?;

    Ok(Json(match hook {
        None => json!({
            "configured": false,
            "events": notify::EVENTS,
            "low_fuel_threshold": 200,
            "low_balance_weeks": 4,
        }),
        Some(w) => json!({
            "configured": true,
            "url": discord::masked(&w.url),
            "events": w.events,
            "low_fuel_threshold": w.low_fuel_threshold,
            "low_balance_weeks": w.low_balance_weeks,
            "enabled": w.enabled,
            "last_sent_at": w.last_sent_at,
            "last_error": w.last_error,
            "last_error_at": w.last_error_at,
        }),
    }))
}

#[derive(Deserialize)]
pub struct Settings {
    /// Required the first time; left out, the stored URL is kept.
    url: Option<String>,
    events: Vec<String>,
    low_fuel_threshold: i32,
    /// Older clients leave it out; they get the default.
    #[serde(default = "default_low_balance_weeks")]
    low_balance_weeks: i32,
}

fn default_low_balance_weeks() -> i32 {
    4
}

pub async fn put(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<Settings>,
) -> Result<StatusCode, ApiError> {
    let carrier_id = owned(&state, &callsign, user.id).await.map_err(|s| (s, String::new()))?;

    let url = match body.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => Some(discord::webhook_url(u).ok_or((
            StatusCode::BAD_REQUEST,
            "not a Discord webhook URL".to_string(),
        ))?),
        None => None,
    };
    if let Some(bad) = body.events.iter().find(|e| !notify::EVENTS.contains(&e.as_str())) {
        return Err((StatusCode::BAD_REQUEST, format!("unknown event {bad:?}")));
    }
    if !(0..=1000).contains(&body.low_fuel_threshold) {
        return Err((StatusCode::BAD_REQUEST, "low_fuel_threshold must be 0-1000".to_string()));
    }
    if !(0..=52).contains(&body.low_balance_weeks) {
        return Err((StatusCode::BAD_REQUEST, "low_balance_weeks must be 0-52".to_string()));
    }
    let mut events = body.events;
    events.sort();
    events.dedup();

    let saved = webhooks::save(
        &state.db_pool,
        carrier_id,
        user.id,
        url.as_deref(),
        &events,
        body.low_fuel_threshold,
        body.low_balance_weeks,
    )
    .await
    .map_err(internal_msg)?;

    match saved {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err((StatusCode::BAD_REQUEST, "url is required".to_string())),
    }
}

pub async fn delete(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, StatusCode> {
    let carrier_id = owned(&state, &callsign, user.id).await?;
    match webhooks::delete(&state.db_pool, carrier_id).await.map_err(internal)? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(StatusCode::NOT_FOUND),
    }
}

/// Send a test message now and report how it went, so the owner knows the
/// channel is right before relying on it.
pub async fn test(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, ApiError> {
    let carrier_id = owned(&state, &callsign, user.id).await.map_err(|s| (s, String::new()))?;
    let hook = webhooks::get(&state.db_pool, carrier_id)
        .await
        .map_err(internal_msg)?
        .ok_or((StatusCode::NOT_FOUND, "no webhook set up".to_string()))?;
    let row = carriers::get(&state.db_pool, carrier_id)
        .await
        .map_err(internal_msg)?
        .unwrap_or(Value::Null);

    let body = discord::test_message(&Carrier::from_row(&row, state.public_origin.as_deref()));
    match discord::post(&state.http, &hook.url, &body).await {
        Ok(()) => {
            let _ = webhooks::record_sent(&state.db_pool, carrier_id, &hook.url).await;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(err) => {
            let gone = matches!(err, SendError::Gone);
            let _ = webhooks::record_error(&state.db_pool, carrier_id, &hook.url, &err.describe(), gone).await;
            let status = match err {
                SendError::Gone => StatusCode::BAD_REQUEST,
                SendError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
                SendError::Failed(_) => StatusCode::BAD_GATEWAY,
            };
            Err((status, err.describe()))
        }
    }
}

/// A carrier with a webhook to notify, and its row from before the event.
pub struct Watch {
    hook: webhooks::Active,
    before: Value,
}

/// Read what ingest compares against, before it applies an event. `None` (no
/// webhook, a stale event, or a failed read) means nothing is announced; a
/// notification problem never fails the ingest.
pub async fn watch_carrier(state: &AppState, carrier_id: Option<i64>, event: &CarrierEvent) -> Option<Watch> {
    let carrier_id = carrier_id?;
    if !notify::is_fresh(event.timestamp(), Utc::now()) {
        return None;
    }
    let hook = webhooks::active(&state.db_pool, carrier_id).await.inspect_err(log).ok()??;
    let before = carriers::get(&state.db_pool, carrier_id).await.inspect_err(log).ok()?.unwrap_or(json!({}));
    Some(Watch { hook, before })
}

/// Announce what an applied event changed. `after` is the carrier's row now.
pub fn carrier_changed(state: &AppState, watch: Watch, carrier_id: i64, event: &CarrierEvent, after: &Value) {
    let on = |event: &str, value: i32| if watch.hook.wants(event) { i64::from(value) } else { 0 };
    let alerts = notify::Alerts {
        low_fuel: on("low_fuel", watch.hook.low_fuel_threshold),
        low_balance_weeks: on("low_balance", watch.hook.low_balance_weeks),
    };
    let notices = notify::carrier_notices(event, &watch.before, after, alerts);
    send(state, &watch.hook, carrier_id, after, notices);
}

/// A market's commodities before a new read, with the carrier's open orders.
pub struct MarketWatch {
    hook: webhooks::Active,
    carrier: Value,
    before: Vec<Value>,
}

pub async fn watch_market(state: &AppState, carrier_id: i64, event: &MarketEvent) -> Option<MarketWatch> {
    if !notify::is_fresh(&event.timestamp, Utc::now()) {
        return None;
    }
    let hook = webhooks::active(&state.db_pool, carrier_id).await.inspect_err(log).ok()??;
    if !hook.wants("order_filled") {
        return None;
    }
    let carrier = carriers::get(&state.db_pool, carrier_id).await.inspect_err(log).ok()??;
    let before = commodities::list(&state.db_pool, carrier_id).await.inspect_err(log).ok()?;
    Some(MarketWatch { hook, carrier, before })
}

pub fn market_changed(state: &AppState, watch: MarketWatch, carrier_id: i64, event: &MarketEvent) {
    let orders = watch.carrier.get("trade_orders").cloned().unwrap_or(Value::Null);
    let notices = notify::market_notices(&watch.before, &event.items, &orders);
    send(state, &watch.hook, carrier_id, &watch.carrier, notices);
}

/// Announce something that isn't tied to one event (an operation finishing).
/// Best effort, like the rest.
pub async fn announce(state: &AppState, carrier_id: i64, notice: Notice) {
    let Some(hook) = webhooks::active(&state.db_pool, carrier_id).await.inspect_err(log).ok().flatten() else {
        return;
    };
    let row = carriers::get(&state.db_pool, carrier_id).await.inspect_err(log).ok().flatten().unwrap_or(Value::Null);
    send(state, &hook, carrier_id, &row, vec![notice]);
}

fn send(state: &AppState, hook: &webhooks::Active, carrier_id: i64, row: &Value, notices: Vec<Notice>) {
    let carrier = Carrier::from_row(row, state.public_origin.as_deref());
    for notice in notices.iter().filter(|n| hook.wants(n.event())) {
        discord::enqueue(
            &state.notify,
            Delivery { carrier_id, url: hook.url.clone(), body: discord::message(notice, &carrier) },
        );
    }
}

fn log(err: &sqlx::Error) {
    eprintln!("webhook lookup failed: {err}");
}
