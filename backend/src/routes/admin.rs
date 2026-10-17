//! `/api/admin`: user moderation. Every handler takes `AdminUser` (403 for
//! anyone else), and every change runs in one transaction with its audit log
//! entry, so neither exists without the other.
//!
//! Safeguards: an admin can't disable, delete or demote their own account, and
//! nobody can remove the last active admin.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::{
    app_state::AppState,
    auth::AdminUser,
    db::{
        admin::{self, Target},
        users::{ROLE_ADMIN, ROLE_USER},
    },
    error::{internal, internal_msg},
};

type ApiError = (StatusCode, String);

fn err(status: StatusCode, msg: &str) -> ApiError {
    (status, msg.to_string())
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
    25
}

pub async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = q.page.max(1);
    let per_page = q.per_page.clamp(1, 100);
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let (items, total) =
        admin::list_users(&state.db_pool, search, per_page, (page - 1).saturating_mul(per_page))
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

pub async fn get_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    admin::user_detail(&state.db_pool, id)
        .await
        .map_err(internal)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Lock the target account, or 404.
async fn target(conn: &mut PgConnection, id: i64) -> Result<Target, ApiError> {
    admin::lock_user(conn, id)
        .await
        .map_err(internal_msg)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "no such user"))
}

fn not_self(admin: &AdminUser, target: &Target) -> Result<(), ApiError> {
    if admin.0.id == target.id {
        return Err(err(StatusCode::BAD_REQUEST, "you can't do that to your own account"));
    }
    Ok(())
}

/// Refuse when taking `target` out of the active admins would leave none. The
/// acting admin is one too, so this only bites when two admins race to remove
/// each other; `lock_user` makes the second one see the first's change.
fn keep_an_admin(target: &Target) -> Result<(), ApiError> {
    if target.role == ROLE_ADMIN && !target.disabled && target.other_active_admins == 0 {
        return Err(err(StatusCode::CONFLICT, "this is the last active admin"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct DisableBody {
    reason: Option<String>,
}

pub async fn disable_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<DisableBody>,
) -> Result<StatusCode, ApiError> {
    let reason = body.reason.as_deref().map(str::trim).filter(|r| !r.is_empty());
    if reason.is_some_and(|r| r.chars().count() > 500) {
        return Err(err(StatusCode::BAD_REQUEST, "reason must be at most 500 characters"));
    }

    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;
    not_self(&admin, &target)?;
    if target.disabled {
        return Err(err(StatusCode::CONFLICT, "already disabled"));
    }
    keep_an_admin(&target)?;

    admin::disable(&mut tx, id, reason).await.map_err(internal_msg)?;
    admin::record(&mut tx, Some(admin.0.id), "user.disable", Some(id),
        json!({ "username": target.username, "reason": reason }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;
    if !target.disabled {
        return Err(err(StatusCode::CONFLICT, "not disabled"));
    }

    admin::enable(&mut tx, id).await.map_err(internal_msg)?;
    admin::record(&mut tx, Some(admin.0.id), "user.enable", Some(id),
        json!({ "username": target.username }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sign a user out everywhere. Allowed on yourself (it signs you out too).
pub async fn revoke_sessions(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;

    let revoked = admin::revoke_sessions(&mut tx, id).await.map_err(internal_msg)?;
    admin::record(&mut tx, Some(admin.0.id), "user.revoke_sessions", Some(id),
        json!({ "username": target.username, "count": revoked }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(Json(json!({ "revoked": revoked })))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((id, token_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;

    let name = admin::revoke_token(&mut tx, id, token_id)
        .await
        .map_err(internal_msg)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "no such token"))?;
    admin::record(&mut tx, Some(admin.0.id), "user.revoke_token", Some(id),
        json!({ "username": target.username, "token_id": token_id, "token_name": name }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    role: Role,
}

pub async fn update_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> Result<StatusCode, ApiError> {
    let role = match body.role {
        Role::User => ROLE_USER,
        Role::Admin => ROLE_ADMIN,
    };

    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;
    not_self(&admin, &target)?;
    if target.role == role {
        return Ok(StatusCode::NO_CONTENT);
    }
    if role == ROLE_USER {
        keep_an_admin(&target)?;
    }

    admin::set_role(&mut tx, id, role).await.map_err(internal_msg)?;
    admin::record(&mut tx, Some(admin.0.id), "user.set_role", Some(id),
        json!({ "username": target.username, "from": target.role, "to": role }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete an account. Its carriers stay, with their history, but unowned: the
/// real owner's plugin can claim them again.
pub async fn delete_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.db_pool.begin().await.map_err(internal_msg)?;
    let target = target(&mut tx, id).await?;
    not_self(&admin, &target)?;
    keep_an_admin(&target)?;

    let released = admin::delete_user(&mut tx, id).await.map_err(internal_msg)?;
    admin::record(&mut tx, Some(admin.0.id), "user.delete", Some(id),
        json!({ "username": target.username, "released_carriers": released }))
        .await
        .map_err(internal_msg)?;
    tx.commit().await.map_err(internal_msg)?;
    Ok(Json(json!({ "released_carriers": released })))
}
