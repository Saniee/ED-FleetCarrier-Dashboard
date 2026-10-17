use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app_state::AppState,
    auth::{self, AuthUser, SESSION_TTL_SECS},
    db::users,
    error::{internal, internal_msg},
};

#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

fn valid_username(name: &str) -> bool {
    (3..=32).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Create an account. Does not log in; the client follows with `/login`.
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    if !state.registration_open {
        return Err((StatusCode::FORBIDDEN, "registration is closed".into()));
    }

    let username = body.username.trim();
    if !valid_username(username) {
        return Err((
            StatusCode::BAD_REQUEST,
            "username must be 3-32 characters of letters, digits, '_' or '-'".into(),
        ));
    }
    if !(8..=128).contains(&body.password.chars().count()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "password must be 8-128 characters".into(),
        ));
    }

    let hash = auth::hash_password(body.password)
        .await
        .map_err(|s| (s, "could not hash password".into()))?;

    let user = users::create(&state.db_pool, username, &hash)
        .await
        .map_err(internal_msg)?
        .ok_or((StatusCode::CONFLICT, "username already taken".into()))?;

    Ok((StatusCode::CREATED, Json(json!(user))))
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Json<Value>, StatusCode> {
    let found = users::find_for_login(&state.db_pool, body.username.trim())
        .await
        .map_err(internal)?;

    // Unknown user and wrong password are indistinguishable to the caller.
    let Some(login) = found else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if !auth::verify_password(body.password, login.password_hash).await {
        return Err(StatusCode::UNAUTHORIZED);
    }
    // Only after the password, so this doesn't reveal which accounts exist.
    if login.disabled {
        return Err(StatusCode::FORBIDDEN);
    }
    let user = login.user;

    let _ = users::purge_expired_sessions(&state.db_pool).await;

    let token = auth::new_token();
    let expires_at =
        users::create_session(&state.db_pool, &auth::hash_token(&token), user.id, SESSION_TTL_SECS)
            .await
            .map_err(internal)?;

    Ok(Json(json!({ "token": token, "expires_at": expires_at, "user": user })))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    _user: AuthUser,
) -> Result<StatusCode, StatusCode> {
    // `AuthUser` already proved the header is present and valid.
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        users::delete_session(&state.db_pool, &auth::hash_token(token.trim()))
            .await
            .map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!(user))
}

#[derive(Deserialize)]
pub struct NewToken {
    name: String,
}

/// Mint an ingest token. The secret is returned here once and never again.
pub async fn create_token(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<NewToken>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let name = body.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err((StatusCode::BAD_REQUEST, "name must be 1-64 characters".into()));
    }

    let secret = auth::new_token();
    let meta = users::create_api_token(&state.db_pool, user.id, name, &auth::hash_token(&secret))
        .await
        .map_err(internal_msg)?;

    Ok((StatusCode::CREATED, Json(json!({ "token": secret, "meta": meta }))))
}

#[derive(Deserialize)]
pub struct Reauth {
    password: String,
}

/// Replace a token's secret and return the new one, once. Secrets are stored
/// only as hashes, so a lost one can't be shown again; this is the way back.
/// The password is asked again so a left-open session can't mint secrets.
pub async fn regenerate_token(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<Reauth>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hash = users::find_for_login(&state.db_pool, &user.username)
        .await
        .map_err(internal_msg)?
        .map(|login| login.password_hash);
    // 403, not 401: the session is fine, and the client logs out on 401.
    let Some(hash) = hash else {
        return Err((StatusCode::FORBIDDEN, "wrong password".into()));
    };
    if !auth::verify_password(body.password, hash).await {
        return Err((StatusCode::FORBIDDEN, "wrong password".into()));
    }

    let secret = auth::new_token();
    let meta = users::regenerate_api_token(&state.db_pool, user.id, id, &auth::hash_token(&secret))
        .await
        .map_err(internal_msg)?
        .ok_or((StatusCode::NOT_FOUND, "no such token".into()))?;

    Ok(Json(json!({ "token": secret, "meta": meta })))
}

pub async fn list_tokens(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let tokens = users::list_api_tokens(&state.db_pool, user.id)
        .await
        .map_err(internal)?;
    Ok(Json(json!(tokens)))
}

pub async fn delete_token(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    match users::delete_api_token(&state.db_pool, user.id, id)
        .await
        .map_err(internal)?
    {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(StatusCode::NOT_FOUND),
    }
}
