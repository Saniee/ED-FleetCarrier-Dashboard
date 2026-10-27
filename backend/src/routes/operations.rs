//! Hauling operations: the ingest for deliveries and depot reports, and the
//! endpoints to run operations. Anyone who may read a carrier may read its
//! operations; only its owner manages them.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Value, json};

use super::carrier_event::IngestResult;
use crate::{
    app_state::AppState,
    auth::{AuthUser, Ingest, Viewer},
    db::{
        carriers::{self, Ownership},
        operations::{self, NewTarget, Operation},
        stats::Kind,
    },
    error::{internal, internal_msg},
    journal_definitions::{HaulingEvent, commodity_key},
    notify::Notice,
};

type ApiError = (StatusCode, String);

fn bad(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.into())
}

const MAX_TARGETS: usize = 60;

// ---------- ingest ----------

pub async fn post_event(
    State(state): State<AppState>,
    ingest: Ingest,
    Json(payload): Json<HaulingEvent>,
) -> IngestResult {
    let result = handle(&state, ingest, payload).await;
    super::carrier_event::count(&state, ingest, Kind::Hauling, &result).await;
    result
}

async fn handle(state: &AppState, ingest: Ingest, payload: HaulingEvent) -> IngestResult {
    // Deliveries are credited to an account; the legacy token has none.
    let Ingest::User(user_id) = ingest else {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "stored": false, "reason": "hauling needs a user token" })),
        ));
    };
    let at = DateTime::parse_from_rfc3339(payload.timestamp())
        .map_err(|_| bad("timestamp is not RFC 3339"))?
        .to_utc();
    let pool = &state.db_pool;

    let stored = match &payload {
        HaulingEvent::ColonisationConstructionDepot(e) => {
            let stored = operations::upsert_depot(pool, e, at).await.map_err(internal_msg)?;
            if stored && e.construction_complete {
                for op in operations::active_touching(pool, None, Some(e.market_id)).await.map_err(internal_msg)? {
                    finish(state, &op).await;
                }
            }
            stored
        }
        HaulingEvent::ColonisationContribution(e) => {
            let mut any = false;
            for c in e.contributions.iter().filter(|c| c.amount > 0) {
                let key = commodity_key(&c.name);
                let name = c.name_localised.as_deref().unwrap_or(&key);
                any |= operations::record_delivery(pool, user_id, "depot", e.market_id, &key, Some(name), c.amount, at)
                    .await
                    .map_err(internal_msg)?;
            }
            any
        }
        HaulingEvent::MarketSell(e) => {
            // Only sales to a carrier we track; selling at a station is nobody's
            // business here.
            if matches!(carriers::ownership(pool, e.market_id).await.map_err(internal_msg)?, Ownership::Missing)
                || e.count <= 0
            {
                false
            } else {
                let key = commodity_key(&e.commodity);
                let name = e.commodity_localised.as_deref().unwrap_or(&key);
                let stored = operations::record_delivery(pool, user_id, "carrier", e.market_id, &key, Some(name), e.count, at)
                    .await
                    .map_err(internal_msg)?;
                if stored {
                    check_manual(state, e.market_id).await;
                }
                stored
            }
        }
    };

    Ok((StatusCode::ACCEPTED, Json(json!({ "stored": stored }))))
}

/// Finish a carrier's operations without a depot whose targets are now met.
async fn check_manual(state: &AppState, carrier_id: i64) {
    let ops = match operations::active_touching(&state.db_pool, Some(carrier_id), None).await {
        Ok(ops) => ops,
        Err(err) => return eprintln!("operations lookup failed: {err}"),
    };
    for op in ops.iter().filter(|op| op.depot_market_id.is_none()) {
        match operations::progress(&state.db_pool, op).await {
            Ok((p, _)) if p.met => finish(state, op).await,
            Ok(_) => {}
            Err(err) => eprintln!("operation {} progress failed: {err}", op.id),
        }
    }
}

async fn finish(state: &AppState, op: &Operation) {
    match operations::complete(&state.db_pool, op.id).await {
        Ok(true) => {
            super::webhooks::announce(state, op.carrier_id, Notice::OperationComplete { name: op.name.clone() }).await;
        }
        Ok(false) => {}
        Err(err) => eprintln!("could not complete operation {}: {err}", op.id),
    }
}

// ---------- reads ----------

/// The operation behind `id`, if `viewer` may read its carrier; and whether they
/// own it.
async fn visible(state: &AppState, id: i64, viewer: &Viewer) -> Result<(Operation, Value, bool), StatusCode> {
    let op = operations::get(&state.db_pool, id).await.map_err(internal)?.ok_or(StatusCode::NOT_FOUND)?;
    let row = carriers::get(&state.db_pool, op.carrier_id).await.map_err(internal)?.ok_or(StatusCode::NOT_FOUND)?;
    let callsign = row["callsign"].as_str().ok_or(StatusCode::NOT_FOUND)?;
    super::carriers::resolve_visible(state, callsign, viewer).await?;
    let owns = matches!((row["owner_id"].as_i64(), &viewer.0), (Some(o), Some(u)) if o == u.id);
    Ok((op, row, owns))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    archived: bool,
}

pub async fn list(
    Path(callsign): Path<String>,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Value>, StatusCode> {
    let carrier_id = super::carriers::resolve_visible(&state, &callsign, &viewer).await?;
    let ops = operations::list(&state.db_pool, carrier_id, q.archived).await.map_err(internal)?;

    let mut items = Vec::with_capacity(ops.len());
    for op in ops {
        let (p, depot) = operations::progress(&state.db_pool, &op).await.map_err(internal)?;
        items.push(json!({
            "operation": op,
            "depot": depot.map(|d| json!({ "star_system": d["star_system"], "station_name": d["station_name"] })),
            "required": p.required,
            "provided": p.provided,
            "fraction": p.fraction,
        }));
    }
    Ok(Json(json!(items)))
}

pub async fn get(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Value>, StatusCode> {
    let (op, row, owns) = visible(&state, id, &viewer).await?;
    let (progress, depot) = operations::progress(&state.db_pool, &op).await.map_err(internal)?;
    Ok(Json(json!({
        "operation": op,
        "carrier": { "callsign": row["callsign"], "name": row["name"] },
        "depot": depot,
        "progress": progress,
        "can_manage": owns,
    })))
}

#[derive(Deserialize)]
pub struct DepotQuery {
    q: Option<String>,
}

/// Depots the plugins have reported, to start an operation from.
pub async fn depots(
    Query(q): Query<DepotQuery>,
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    operations::search_depots(&state.db_pool, search, 20)
        .await
        .map(|rows| Json(json!(rows)))
        .map_err(internal)
}

pub async fn commodities(State(state): State<AppState>, AuthUser(_): AuthUser) -> Result<Json<Value>, StatusCode> {
    operations::known_commodities(&state.db_pool)
        .await
        .map(|rows| Json(json!(rows)))
        .map_err(internal)
}

// ---------- owner ----------

#[derive(Deserialize)]
pub struct TargetBody {
    commodity: String,
    name: Option<String>,
    required: i64,
}

fn targets(body: Vec<TargetBody>) -> Result<Vec<NewTarget>, ApiError> {
    if body.len() > MAX_TARGETS {
        return Err(bad(format!("at most {MAX_TARGETS} targets")));
    }
    body.into_iter()
        .map(|t| {
            let commodity = commodity_key(&t.commodity);
            if commodity.is_empty() {
                return Err(bad("a target has no commodity"));
            }
            let required = i32::try_from(t.required)
                .ok()
                .filter(|r| (1..=10_000_000).contains(r))
                .ok_or_else(|| bad(format!("{commodity}: required must be 1-10000000")))?;
            let name = t.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| commodity.clone());
            Ok(NewTarget { commodity, name, required })
        })
        .collect()
}

fn name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim();
    match name.chars().count() {
        1..=100 => Ok(name.to_string()),
        _ => Err(bad("name must be 1-100 characters")),
    }
}

#[derive(Deserialize)]
pub struct CreateBody {
    name: String,
    depot_market_id: Option<i64>,
    #[serde(default)]
    targets: Vec<TargetBody>,
}

pub async fn create(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let carrier_id = match super::carriers::resolve(&state, &callsign).await.map_err(|s| (s, String::new()))? {
        (id, Some(owner)) if owner == user.id => id,
        _ => return Err((StatusCode::FORBIDDEN, String::new())),
    };
    let name = name(&body.name)?;
    if let Some(depot) = body.depot_market_id
        && operations::depot(&state.db_pool, depot).await.map_err(internal_msg)?.is_none()
    {
        return Err(bad("unknown depot: dock at it with the plugin running first"));
    }
    // A depot lists its own needs.
    let targets = match body.depot_market_id {
        Some(_) => Vec::new(),
        None => targets(body.targets)?,
    };

    let id = operations::create(&state.db_pool, carrier_id, user.id, &name, body.depot_market_id, &targets)
        .await
        .map_err(internal_msg)?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Complete,
    Archived,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    name: Option<String>,
    status: Option<Status>,
    targets: Option<Vec<TargetBody>>,
}

pub async fn update(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<UpdateBody>,
) -> Result<StatusCode, ApiError> {
    let op = owned(&state, id, user.id).await.map_err(|s| (s, String::new()))?;
    let name = body.name.as_deref().map(name).transpose()?;
    let targets = match body.targets {
        Some(_) if op.depot_market_id.is_some() => return Err(bad("a depot operation's targets come from the depot")),
        Some(t) => Some(targets(t)?),
        None => None,
    };
    let status = body.status.map(|s| match s {
        Status::Active => "active",
        Status::Complete => "complete",
        Status::Archived => "archived",
    });

    operations::update(&state.db_pool, id, name.as_deref(), status, targets.as_deref())
        .await
        .map_err(internal_msg)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, StatusCode> {
    owned(&state, id, user.id).await?;
    operations::delete(&state.db_pool, id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The operation, if `user_id` owns its carrier. 404 for one that doesn't
/// exist, 403 for someone else's.
async fn owned(state: &AppState, id: i64, user_id: i64) -> Result<Operation, StatusCode> {
    let op = operations::get(&state.db_pool, id).await.map_err(internal)?.ok_or(StatusCode::NOT_FOUND)?;
    match carriers::ownership(&state.db_pool, op.carrier_id).await.map_err(internal)? {
        Ownership::Owned(owner) if owner == user_id => Ok(op),
        _ => Err(StatusCode::FORBIDDEN),
    }
}
