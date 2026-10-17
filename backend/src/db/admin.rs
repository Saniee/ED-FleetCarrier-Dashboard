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

/// The account an admin action is aimed at, locked for the transaction.
pub struct Target {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub disabled: bool,
    /// Active (not disabled) admins besides this account.
    pub other_active_admins: usize,
}

/// Lock the target account together with every admin row, in id order. One
/// statement in one order means two admins acting on each other can't
/// deadlock, and the second waits for the first to commit and then counts the
/// admins with its change (a demoted row no longer matches `role = 'admin'`).
pub async fn lock_user(conn: &mut PgConnection, id: i64) -> sqlx::Result<Option<Target>> {
    let rows: Vec<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT id, username, role, disabled_at IS NOT NULL FROM users
         WHERE id = $1 OR role = 'admin' ORDER BY id FOR UPDATE",
    )
    .bind(id)
    .fetch_all(conn)
    .await?;

    let other_active_admins = rows
        .iter()
        .filter(|(row_id, _, role, disabled)| *row_id != id && role == "admin" && !disabled)
        .count();
    Ok(rows
        .into_iter()
        .find(|(row_id, ..)| *row_id == id)
        .map(|(id, username, role, disabled)| Target { id, username, role, disabled, other_active_admins }))
}

pub async fn set_role(conn: &mut PgConnection, id: i64, role: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET role = $2 WHERE id = $1")
        .bind(id)
        .bind(role)
        .execute(conn)
        .await?;
    Ok(())
}

/// Disable an account and end its sessions. Its tokens stay but stop resolving
/// while it is disabled (see `users::user_for_api_token`).
pub async fn disable(conn: &mut PgConnection, id: i64, reason: Option<&str>) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET disabled_at = now(), disabled_reason = $2 WHERE id = $1")
        .bind(id)
        .bind(reason)
        .execute(&mut *conn)
        .await?;
    revoke_sessions(conn, id).await?;
    Ok(())
}

pub async fn enable(conn: &mut PgConnection, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET disabled_at = NULL, disabled_reason = NULL WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// End every session of a user. Returns how many there were.
pub async fn revoke_sessions(conn: &mut PgConnection, id: i64) -> sqlx::Result<u64> {
    let done = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected())
}

/// Revoke one of a user's tokens. Returns its name, or `None` when the user has
/// no such token.
pub async fn revoke_token(
    conn: &mut PgConnection,
    user_id: i64,
    token_id: i64,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("DELETE FROM api_tokens WHERE id = $1 AND user_id = $2 RETURNING name")
        .bind(token_id)
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// Delete an account. Sessions and tokens go with it; its carriers stay, with
/// their history, but become unowned. Returns the released callsigns.
pub async fn delete_user(conn: &mut PgConnection, id: i64) -> sqlx::Result<Vec<String>> {
    let released: Vec<String> = sqlx::query_scalar(
        "UPDATE carriers SET owner_id = NULL WHERE owner_id = $1
         RETURNING coalesce(callsign, carrier_id::text)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(released)
}

/// The per-user fields of the admin list and detail views, as
/// `jsonb_build_object` arguments over `users u`. A macro so the queries stay
/// string literals.
macro_rules! user_fields {
    () => {
        "'id', u.id, 'username', u.username, 'role', u.role, 'created_at', u.created_at,
         'disabled_at', u.disabled_at, 'disabled_reason', u.disabled_reason,
         'carriers', (SELECT count(*) FROM carriers c WHERE c.owner_id = u.id),
         'tokens', (SELECT count(*) FROM api_tokens t WHERE t.user_id = u.id),
         'last_seen', GREATEST(
             (SELECT max(s.created_at) FROM sessions s WHERE s.user_id = u.id),
             (SELECT max(t.last_used_at) FROM api_tokens t WHERE t.user_id = u.id))"
    };
}

/// One page of users, oldest first, plus the total. `search` matches the
/// username.
pub async fn list_users(
    pool: &PgPool,
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<Value>, i64)> {
    let pattern = search.map(|s| {
        let escaped = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users u WHERE $1::text IS NULL OR u.username ILIKE $1",
    )
    .bind(&pattern)
    .fetch_one(pool)
    .await?;

    let items = sqlx::query_scalar(concat!(
        "SELECT jsonb_build_object(",
        user_fields!(),
        ") FROM users u
         WHERE $1::text IS NULL OR u.username ILIKE $1
         ORDER BY u.id LIMIT $2 OFFSET $3"
    ))
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((items, total))
}

/// One user with their carriers, token metadata (never secrets) and how many
/// sessions are live.
pub async fn user_detail(pool: &PgPool, id: i64) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar(concat!(
        "SELECT jsonb_build_object(",
        user_fields!(),
        ",
         'active_sessions', (SELECT count(*) FROM sessions s
                             WHERE s.user_id = u.id AND s.expires_at > now()),
         'carrier_list', coalesce((SELECT jsonb_agg(jsonb_build_object(
                 'carrier_id', c.carrier_id, 'callsign', c.callsign, 'name', c.name,
                 'visibility', c.visibility, 'updated_at', c.updated_at)
             ORDER BY c.updated_at DESC) FROM carriers c WHERE c.owner_id = u.id), '[]'),
         'token_list', coalesce((SELECT jsonb_agg(jsonb_build_object(
                 'id', t.id, 'name', t.name, 'created_at', t.created_at,
                 'last_used_at', t.last_used_at)
             ORDER BY t.id) FROM api_tokens t WHERE t.user_id = u.id), '[]'))
         FROM users u WHERE u.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}
