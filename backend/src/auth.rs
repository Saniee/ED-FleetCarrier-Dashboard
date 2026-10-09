//! Credentials and request authentication. Session tokens (dashboard, expiring)
//! and API tokens (EDMC plugin, revocable, `X-Ingest-Token`) are random and
//! stored only as a SHA-256.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString,
};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, header, request::Parts},
};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::{
    app_state::AppState,
    error::internal,
    db::{
        carriers::Ownership,
        users::{self, User},
    },
};

pub const INGEST_HEADER: &str = "X-Ingest-Token";
pub const SESSION_TTL_SECS: i64 = 7 * 24 * 60 * 60;

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Argon2 is deliberately slow, so keep it off the async workers.
pub async fn hash_password(password: String) -> Result<String, StatusCode> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(internal)?
    .map_err(internal)
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, StatusCode> {
        let token = bearer(parts).ok_or(StatusCode::UNAUTHORIZED)?;
        users::user_for_session(&state.db_pool, &hash_token(token))
            .await
            .map_err(internal)?
            .map(AuthUser)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// The logged-in user, or `None`: anonymous viewers are normal on public pages,
/// so this never rejects. The token is read from the `Authorization` header only.
pub struct Viewer(pub Option<User>);

impl FromRequestParts<AppState> for Viewer {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, StatusCode> {
        let Some(token) = bearer(parts) else {
            return Ok(Viewer(None));
        };

        let user = users::user_for_session(&state.db_pool, &hash_token(token))
            .await
            .map_err(internal)?;
        Ok(Viewer(user))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Ingest {
    User(i64),
    /// The legacy shared `TOKEN`, or no auth at all with `ALLOW_ANON_INGEST`.
    Legacy,
}

pub enum Access {
    Allow,
    Skip,
}

impl Ingest {
    /// Decide whether this caller may write to `carrier_id`. `visitor` events
    /// (`CarrierJump`, `CarrierLocation`, `Market`) can come from anyone docked at
    /// the carrier, so any user may report them for an existing carrier but never
    /// create one. Other events are owner-only (403 if another user owns it),
    /// events with no carrier are skipped, and the legacy token keeps full access.
    pub async fn check(
        self,
        pool: &sqlx::PgPool,
        carrier_id: Option<i64>,
        visitor: bool,
    ) -> Result<Access, StatusCode> {
        let Ingest::User(user_id) = self else {
            return Ok(Access::Allow);
        };
        let Some(carrier_id) = carrier_id else {
            return Ok(Access::Skip);
        };

        let ownership = crate::db::carriers::ownership(pool, carrier_id)
            .await
            .map_err(internal)?;

        match (ownership, visitor) {
            (Ownership::Owned(owner), _) if owner == user_id => Ok(Access::Allow),
            // Never create a carrier from a visitor's report: with enough
            // plugin users that would flood the database with strangers.
            (Ownership::Missing, true) => Ok(Access::Skip),
            (_, true) => Ok(Access::Allow),
            (Ownership::Owned(_), false) => Err(StatusCode::FORBIDDEN),
            (_, false) => Ok(Access::Allow),
        }
    }
}

impl FromRequestParts<AppState> for Ingest {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, StatusCode> {
        let presented = parts
            .headers
            .get(INGEST_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|t| !t.is_empty());

        if let Some(token) = presented {
            let found = users::user_for_api_token(&state.db_pool, &hash_token(token))
                .await
                .map_err(internal)?;
            if let Some(user_id) = found {
                return Ok(Ingest::User(user_id));
            }
        }

        match (&state.legacy_token, presented) {
            (Some(expected), Some(got)) if constant_eq(expected, got) => Ok(Ingest::Legacy),
            (Some(_), _) => Err(StatusCode::UNAUTHORIZED),
            // No legacy token configured. Only a request with no token at all,
            // and only with `ALLOW_ANON_INGEST` on, falls through to open ingest.
            (None, None) if state.anon_ingest => Ok(Ingest::Legacy),
            (None, _) => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (hash_token(a), hash_token(b));
    a.iter().zip(&b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
