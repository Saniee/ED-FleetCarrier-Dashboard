use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

/// A registered user, without the password hash.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
}

/// An API token's metadata. The secret itself is never stored.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Insert a user. `Ok(None)` when the username is already taken.
pub async fn create(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> sqlx::Result<Option<User>> {
    sqlx::query_as(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2)
         ON CONFLICT DO NOTHING
         RETURNING id, username",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// A user and their password hash, looked up case-insensitively by username.
pub async fn find_with_hash(
    pool: &PgPool,
    username: &str,
) -> sqlx::Result<Option<(User, String)>> {
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT id, username, password_hash FROM users WHERE lower(username) = lower($1)",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, username, hash)| (User { id, username }, hash)))
}

pub async fn create_session(
    pool: &PgPool,
    token_hash: &[u8],
    user_id: i64,
    ttl_secs: i64,
) -> sqlx::Result<DateTime<Utc>> {
    sqlx::query_scalar(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES ($1, $2, now() + make_interval(secs => $3))
         RETURNING expires_at",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(ttl_secs as f64)
    .fetch_one(pool)
    .await
}

/// The user behind a live (unexpired) session token.
pub async fn user_for_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<User>> {
    sqlx::query_as(
        "SELECT u.id, u.username FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn delete_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Drop expired sessions. Run opportunistically on login.
pub async fn purge_expired_sessions(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_api_token(
    pool: &PgPool,
    user_id: i64,
    name: &str,
    token_hash: &[u8],
) -> sqlx::Result<ApiToken> {
    sqlx::query_as(
        "INSERT INTO api_tokens (user_id, name, token_hash) VALUES ($1, $2, $3)
         RETURNING id, name, created_at, last_used_at",
    )
    .bind(user_id)
    .bind(name)
    .bind(token_hash)
    .fetch_one(pool)
    .await
}

pub async fn list_api_tokens(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<ApiToken>> {
    sqlx::query_as(
        "SELECT id, name, created_at, last_used_at FROM api_tokens
         WHERE user_id = $1 ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Revoke a token. `false` when it does not exist or belongs to someone else.
pub async fn delete_api_token(pool: &PgPool, user_id: i64, id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM api_tokens WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Resolve an ingest token to its owner, stamping `last_used_at`.
pub async fn user_for_api_token(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "UPDATE api_tokens SET last_used_at = now() WHERE token_hash = $1 RETURNING user_id",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}
