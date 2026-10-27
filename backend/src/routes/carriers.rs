use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app_state::{AppState, Update},
    auth::{AuthUser, Viewer},
    db::carriers,
    error::internal,
};

/// `(carrier_id, owner_id)` behind a callsign, or 404. For owner actions
/// (claim, release, settings), which do their own ownership check.
pub async fn resolve(state: &AppState, callsign: &str) -> Result<(i64, Option<i64>), StatusCode> {
    carriers::find_by_callsign(&state.db_pool, callsign)
        .await
        .map_err(internal)?
        .map(|c| (c.carrier_id, c.owner_id))
        .ok_or(StatusCode::NOT_FOUND)
}

/// The `carrier_id` behind a callsign, if `viewer` may read it. `owner_only`
/// carriers answer 404 to everyone but the owner, so they aren't revealed; a
/// disabled account's carriers answer 404 to everyone but admins (moderation
/// may be about the carrier itself); and a carrier an admin hid answers 404 to
/// everyone but its owner, who sees why, and admins.
pub async fn resolve_visible(
    state: &AppState,
    callsign: &str,
    viewer: &Viewer,
) -> Result<i64, StatusCode> {
    let c = carriers::find_by_callsign(&state.db_pool, callsign)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let is_owner = matches!((c.owner_id, &viewer.0), (Some(o), Some(u)) if o == u.id);
    let is_admin = viewer.0.as_ref().is_some_and(|u| u.is_admin());
    if c.owner_disabled && !is_admin {
        return Err(StatusCode::NOT_FOUND);
    }
    if c.hidden && !is_owner && !is_admin {
        return Err(StatusCode::NOT_FOUND);
    }
    if c.visibility == "owner_only" && !is_owner {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(c.carrier_id)
}

/// Send a carrier's current row to its open streams after a change no ingest
/// announces (visibility, owner, moderation), so they update or, if the viewer
/// may no longer read it, end. Best effort: a failure is only logged.
pub async fn publish(state: &AppState, carrier_id: i64) {
    match carriers::get(&state.db_pool, carrier_id).await {
        Ok(Some(snapshot)) => {
            let _ = state.tx.send(Update::Carrier(carrier_id, snapshot));
        }
        Ok(None) => {}
        Err(err) => eprintln!("could not publish carrier {carrier_id}: {err}"),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    q: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    20
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = q.page.max(1);
    let per_page = q.per_page.clamp(1, 100);
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let (items, total) = carriers::list_public(
        &state.db_pool,
        search,
        per_page,
        (page - 1).saturating_mul(per_page),
    )
    .await
    .map_err(internal)?;

    Ok(Json(json!({
        "items": items,
        "page": page,
        "per_page": per_page,
        "total": total,
        "total_pages": (total + per_page - 1) / per_page,
    })))
}

/// A carrier's table by callsign. `private` carriers are reachable by link;
/// `owner_only` ones only by their owner.
pub async fn get(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Value>, StatusCode> {
    let carrier_id = resolve_visible(&state, &callsign, &viewer).await?;
    carriers::get(&state.db_pool, carrier_id)
        .await
        .map_err(internal)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn release(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, StatusCode> {
    let (carrier_id, _) = resolve(&state, &callsign).await?;
    match carriers::release(&state.db_pool, carrier_id, user.id)
        .await
        .map_err(internal)?
    {
        true => {
            publish(&state, carrier_id).await;
            Ok(StatusCode::NO_CONTENT)
        }
        false => Err(StatusCode::FORBIDDEN),
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Listed, readable by anyone.
    Public,
    /// Not listed, readable by anyone with the link.
    Private,
    /// Readable only by the owner.
    OwnerOnly,
}

impl Visibility {
    fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::OwnerOnly => "owner_only",
        }
    }
}

#[derive(Deserialize)]
pub struct Settings {
    visibility: Option<Visibility>,
    /// Owner's designation of this carrier as their squadron's carrier.
    is_squadron: Option<bool>,
    /// Weekly upkeep in credits, from the in-game finance screen; 0 clears it.
    weekly_upkeep: Option<i64>,
}

const MAX_UPKEEP: i64 = 1_000_000_000;

pub async fn update_settings(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<Settings>,
) -> Result<StatusCode, StatusCode> {
    let (carrier_id, _) = resolve(&state, &callsign).await?;
    if body.weekly_upkeep.is_some_and(|u| !(0..=MAX_UPKEEP).contains(&u)) {
        return Err(StatusCode::BAD_REQUEST);
    }
    match carriers::set_settings(
        &state.db_pool,
        carrier_id,
        user.id,
        body.visibility.map(Visibility::as_str),
        body.is_squadron,
        body.weekly_upkeep,
    )
    .await
    .map_err(internal)?
    {
        true => {
            publish(&state, carrier_id).await;
            Ok(StatusCode::NO_CONTENT)
        }
        false => Err(StatusCode::FORBIDDEN),
    }
}

pub async fn mine(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    carriers::list_owned(&state.db_pool, user.id)
        .await
        .map(|rows| Json(json!(rows)))
        .map_err(internal)
}
