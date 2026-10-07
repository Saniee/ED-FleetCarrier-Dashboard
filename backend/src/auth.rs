//! Credentials and request authentication.
//!
//! Two kinds of bearer secret exist, both random and stored only as a SHA-256:
//! a *session token* (dashboard, expires) and an *API token* (EDMC plugin,
//! revocable, sent in `X-Ingest-Token`).

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
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(AuthUser)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// The logged-in user if the request carries a valid session, else `None`.
/// Never rejects for a missing or bad token: anonymous viewers are normal on
/// public pages.
///
/// `EventSource` cannot set headers, so the `/stream` routes also accept the
/// session token as an `access_token` query parameter.
pub struct Viewer(pub Option<User>);

impl FromRequestParts<AppState> for Viewer {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, StatusCode> {
        let token = bearer(parts).map(str::to_owned).or_else(|| {
            if !parts.uri.path().ends_with("/stream") {
                return None;
            }
            parts
                .uri
                .query()?
                .split('&')
                .find_map(|pair| pair.strip_prefix("access_token="))
                .map(str::to_owned)
        });

        let Some(token) = token else {
            return Ok(Viewer(None));
        };

        let user = users::user_for_session(&state.db_pool, &hash_token(&token))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Viewer(user))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Ingest {
    User(i64),
    /// The legacy shared `TOKEN`, or no auth at all when `TOKEN` is unset.
    Legacy,
}

pub enum Access {
    Allow,
    Skip,
}

impl Ingest {
    /// Decide whether this caller may write to `carrier_id`.
    ///
    /// `visitor` marks events anyone docked at a carrier can generate, not just
    /// its owner (`CarrierJump`, `CarrierLocation`, `Market`). Any user may
    /// submit those for any carrier already in the database: more reporters keep
    /// the data fresher. A carrier we have never seen is skipped, not created.
    /// Owner-only management events are stored unless another user owns the
    /// carrier, which is a 403. An event that cannot be attributed to a carrier
    /// (an on-foot `CarrierJump`) is skipped: attributing it means guessing by
    /// system, which is not safe with several tenants. The legacy token keeps
    /// full access.
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
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(user_id) = found {
                return Ok(Ingest::User(user_id));
            }
        }

        match (&state.legacy_token, presented) {
            (Some(expected), Some(got)) if constant_eq(expected, got) => Ok(Ingest::Legacy),
            (Some(_), _) => Err(StatusCode::UNAUTHORIZED),
            // No legacy token configured and nothing valid presented. A
            // presented-but-unknown token is still a failure; only a request
            // with no token at all falls through to the dev-mode open ingest.
            (None, Some(_)) => Err(StatusCode::UNAUTHORIZED),
            (None, None) => Ok(Ingest::Legacy),
        }
    }
}

fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (hash_token(a), hash_token(b));
    a.iter().zip(&b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
