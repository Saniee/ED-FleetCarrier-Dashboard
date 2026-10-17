use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use super::users::User;

/// Append to the audit log. Takes a connection so callers write it in the same
/// transaction as the action it records. `admin_id` is `None` for the command
/// line.
pub async fn record(
    conn: &mut PgConnection,
    admin_id: Option<i64>,
    action: &str,
    target_id: Option<i64>,
    detail: Value,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO admin_actions (admin_id, action, target_id, detail) VALUES ($1, $2, $3, $4)",
    )
    .bind(admin_id)
    .bind(action)
    .bind(target_id)
    .bind(detail)
    .execute(conn)
    .await?;
    Ok(())
}

/// Set a user's role by username. `None` when there is no such user.
pub async fn set_role_by_username(
    conn: &mut PgConnection,
    username: &str,
    role: &str,
) -> sqlx::Result<Option<User>> {
    sqlx::query_as(
        "UPDATE users SET role = $2 WHERE lower(username) = lower($1)
         RETURNING id, username, role",
    )
    .bind(username)
    .bind(role)
    .fetch_optional(conn)
    .await
}

pub async fn list_admins(pool: &PgPool) -> sqlx::Result<Vec<User>> {
    sqlx::query_as("SELECT id, username, role FROM users WHERE role = 'admin' ORDER BY id")
        .fetch_all(pool)
        .await
}
