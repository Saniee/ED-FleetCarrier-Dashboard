use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app_state::AppState,
    auth::{AuthUser, Viewer},
    db::carriers,
};

fn internal<E>(_: E) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

/// `(carrier_id, owner_id)` behind a callsign, or 404. For owner actions
/// (claim, release, settings), which do their own ownership check.
pub async fn resolve(state: &AppState, callsign: &str) -> Result<(i64, Option<i64>), StatusCode> {
    carriers::find_by_callsign(&state.db_pool, callsign)
        .await
        .map_err(internal)?
        .map(|(id, owner, _)| (id, owner))
        .ok_or(StatusCode::NOT_FOUND)
}

/// The `carrier_id` behind a callsign, if `viewer` may read it.
///
/// `owner_only` carriers answer 404 to everyone but their owner, so their
/// existence is not revealed.
pub async fn resolve_visible(
    state: &AppState,
    callsign: &str,
    viewer: &Viewer,
) -> Result<i64, StatusCode> {
    let (id, owner, visibility) = carriers::find_by_callsign(&state.db_pool, callsign)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let is_owner = matches!((owner, &viewer.0), (Some(o), Some(u)) if o == u.id);
    if visibility == "owner_only" && !is_owner {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(id)
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    /// Matches callsign, name or system.
    q: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    20
}

/// Paginated discovery list. Only `public` carriers are listed.
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

/// Claim an unowned carrier. Idempotent for its current owner.
pub async fn claim(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, StatusCode> {
    let (carrier_id, owner) = resolve(&state, &callsign).await?;
    match owner {
        Some(id) if id == user.id => Ok(StatusCode::NO_CONTENT),
        Some(_) => Err(StatusCode::CONFLICT),
        None => match carriers::claim(&state.db_pool, carrier_id, user.id)
            .await
            .map_err(internal)?
        {
            true => Ok(StatusCode::NO_CONTENT),
            // Lost a race with another claim.
            false => Err(StatusCode::CONFLICT),
        },
    }
}

/// Give up ownership.
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
        true => Ok(StatusCode::NO_CONTENT),
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
    visibility: Visibility,
}

/// Owner-only settings (currently just the visibility).
pub async fn update_settings(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<Settings>,
) -> Result<StatusCode, StatusCode> {
    let (carrier_id, _) = resolve(&state, &callsign).await?;
    match carriers::set_visibility(&state.db_pool, carrier_id, user.id, body.visibility.as_str())
        .await
        .map_err(internal)?
    {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(StatusCode::FORBIDDEN),
    }
}

/// Carriers owned by the logged-in user, whatever their visibility.
pub async fn mine(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    carriers::list_owned(&state.db_pool, user.id)
        .await
        .map(|rows| Json(json!(rows)))
        .map_err(internal)
}
