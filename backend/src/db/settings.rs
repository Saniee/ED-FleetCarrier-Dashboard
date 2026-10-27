//! Settings changed from the admin dashboard (`app_settings`). A stored value
//! wins over the matching environment variable; with none stored, the
//! variable applies.

use serde_json::Value;
use sqlx::{PgConnection, PgPool};

/// Whether `POST /api/auth/register` creates accounts.
pub const REGISTRATION_OPEN: &str = "registration_open";

pub async fn get_bool(pool: &PgPool, key: &str) -> sqlx::Result<Option<bool>> {
    let value: Option<Value> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(value.and_then(|v| v.as_bool()))
}

/// Store `value`, or with `None` remove the setting so the environment applies
/// again. Locks the row first and returns what was stored before, for the audit
/// log.
pub async fn set_bool(conn: &mut PgConnection, key: &str, value: Option<bool>) -> sqlx::Result<Option<bool>> {
    let before: Option<Value> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1 FOR UPDATE")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
    match value {
        Some(v) => {
            sqlx::query(
                "INSERT INTO app_settings (key, value) VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
            )
            .bind(key)
            .bind(Value::Bool(v))
            .execute(conn)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM app_settings WHERE key = $1")
                .bind(key)
                .execute(conn)
                .await?;
        }
    }
    Ok(before.and_then(|v| v.as_bool()))
}
