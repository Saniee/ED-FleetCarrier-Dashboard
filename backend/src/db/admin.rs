use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use super::users::User;

/// What an audit entry is about.
#[derive(Clone, Copy)]
pub enum Subject {
    User(i64),
    /// A `carrier_id`.
    Carrier(i64),
}

impl Subject {
    fn parts(self) -> (&'static str, i64) {
        match self {
            Subject::User(id) => ("user", id),
            Subject::Carrier(id) => ("carrier", id),
        }
    }
}

/// Append to the audit log. Takes a connection so callers write it in the same
/// transaction as the action it records. `admin_id` is `None` for the command
/// line.
pub async fn record(
    conn: &mut PgConnection,
    admin_id: Option<i64>,
    action: &str,
    subject: Option<Subject>,
    detail: Value,
) -> sqlx::Result<()> {
    let (kind, id) = subject.map(Subject::parts).unzip();
    sqlx::query(
        "INSERT INTO admin_actions (admin_id, action, target_kind, target_id, detail)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(admin_id)
    .bind(action)
    .bind(kind)
    .bind(id)
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

/// One page of the audit log, newest first, plus the total. `subject` narrows
/// it: to an account (including actions on carriers it owned or was given), or
/// to a carrier. Names are the current ones where the account or carrier still
/// exists, else the one recorded at the time; `admin` is null for the command
/// line.
pub async fn list_actions(
    pool: &PgPool,
    subject: Option<Subject>,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<Value>, i64)> {
    let (kind, id) = subject.map(Subject::parts).unzip();
    macro_rules! filter {
        () => {
            "($1::text IS NULL
              OR (a.target_kind = $1 AND a.target_id = $2)
              OR ($1 = 'user' AND a.target_kind = 'carrier'
                  AND $2 IN ((a.detail->>'owner_id')::bigint, (a.detail->>'to_owner_id')::bigint)))"
        };
    }

    let total: i64 = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM admin_actions a WHERE ",
        filter!()
    ))
    .bind(kind)
    .bind(id)
    .fetch_one(pool)
    .await?;

    let items = sqlx::query_scalar(concat!(
        "SELECT jsonb_build_object(
             'id', a.id, 'action', a.action, 'created_at', a.created_at,
             'admin_id', a.admin_id, 'admin', ad.username,
             'target_kind', a.target_kind, 'target_id', a.target_id,
             'target', CASE a.target_kind
                 WHEN 'user' THEN coalesce(t.username, a.detail->>'username')
                 WHEN 'carrier' THEN coalesce(c.callsign, a.detail->>'callsign', a.target_id::text)
             END,
             'detail', a.detail)
         FROM admin_actions a
         LEFT JOIN users ad ON ad.id = a.admin_id
         LEFT JOIN users t ON a.target_kind = 'user' AND t.id = a.target_id
         LEFT JOIN carriers c ON a.target_kind = 'carrier' AND c.carrier_id = a.target_id
         WHERE ",
        filter!(),
        " ORDER BY a.id DESC LIMIT $3 OFFSET $4"
    ))
    .bind(kind)
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((items, total))
}

/// A carrier an admin action is aimed at, locked for the transaction.
#[derive(sqlx::FromRow)]
pub struct CarrierTarget {
    pub callsign: Option<String>,
    pub owner_id: Option<i64>,
    /// The owner's username.
    pub owner: Option<String>,
    pub hidden: bool,
}

pub async fn lock_carrier(conn: &mut PgConnection, carrier_id: i64) -> sqlx::Result<Option<CarrierTarget>> {
    sqlx::query_as(
        "SELECT c.callsign, c.owner_id, u.username AS owner,
                c.moderation_hidden_at IS NOT NULL AS hidden
         FROM carriers c LEFT JOIN users u ON u.id = c.owner_id
         WHERE c.carrier_id = $1 FOR UPDATE OF c",
    )
    .bind(carrier_id)
    .fetch_optional(conn)
    .await
}

/// `(id, username)` of an account by name, case-insensitively.
pub async fn find_user(conn: &mut PgConnection, username: &str) -> sqlx::Result<Option<(i64, String)>> {
    sqlx::query_as("SELECT id, username FROM users WHERE lower(username) = lower($1)")
        .bind(username)
        .fetch_optional(conn)
        .await
}

/// Give a carrier to `owner`, or with `None` leave it unowned.
pub async fn set_carrier_owner(conn: &mut PgConnection, carrier_id: i64, owner: Option<i64>) -> sqlx::Result<()> {
    sqlx::query("UPDATE carriers SET owner_id = $2 WHERE carrier_id = $1")
        .bind(carrier_id)
        .bind(owner)
        .execute(conn)
        .await?;
    Ok(())
}

/// Hide a carrier (with `Some`) or show it again (with `None`).
pub async fn set_carrier_hidden(
    conn: &mut PgConnection,
    carrier_id: i64,
    hidden: Option<Option<&str>>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE carriers
         SET moderation_hidden_at = CASE WHEN $2 THEN now() END,
             moderation_reason = CASE WHEN $2 THEN $3 END
         WHERE carrier_id = $1",
    )
    .bind(carrier_id)
    .bind(hidden.is_some())
    .bind(hidden.flatten())
    .execute(conn)
    .await?;
    Ok(())
}

/// Delete a carrier with its history and market. Returns how many events went.
pub async fn delete_carrier(conn: &mut PgConnection, carrier_id: i64) -> sqlx::Result<i64> {
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM carrier_events WHERE carrier = $1")
        .bind(carrier_id)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM carriers WHERE carrier_id = $1")
        .bind(carrier_id)
        .execute(conn)
        .await?;
    Ok(events)
}

/// The per-carrier fields of the admin carrier list and detail, as
/// `jsonb_build_object` arguments over `carriers c LEFT JOIN users u` (the owner).
macro_rules! carrier_fields {
    () => {
        "'carrier_id', c.carrier_id, 'callsign', c.callsign, 'name', c.name,
         'star_system', c.star_system, 'visibility', c.visibility, 'updated_at', c.updated_at,
         'owner_id', c.owner_id, 'owner', u.username, 'owner_disabled', u.disabled_at IS NOT NULL,
         'moderation_hidden_at', c.moderation_hidden_at, 'moderation_reason', c.moderation_reason"
    };
}

/// Which carriers the admin list shows.
#[derive(Clone, Copy)]
pub enum CarrierFilter {
    All,
    Hidden,
    Unowned,
}

/// One page of every carrier, most recently active first, plus the total.
/// `search` matches callsign, name, system or the owner's name.
pub async fn list_carriers(
    pool: &PgPool,
    search: Option<&str>,
    filter: CarrierFilter,
    limit: i64,
    offset: i64,
) -> sqlx::Result<(Vec<Value>, i64)> {
    let pattern = search.map(|s| {
        let escaped = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });
    let filter = match filter {
        CarrierFilter::All => "all",
        CarrierFilter::Hidden => "hidden",
        CarrierFilter::Unowned => "unowned",
    };
    macro_rules! filter {
        () => {
            " FROM carriers c LEFT JOIN users u ON u.id = c.owner_id
             WHERE ($1::text IS NULL OR c.callsign ILIKE $1 OR c.name ILIKE $1
                    OR c.star_system ILIKE $1 OR u.username ILIKE $1)
               AND ($2 = 'all'
                    OR ($2 = 'hidden' AND c.moderation_hidden_at IS NOT NULL)
                    OR ($2 = 'unowned' AND c.owner_id IS NULL))"
        };
    }

    let total: i64 = sqlx::query_scalar(concat!("SELECT count(*)", filter!()))
        .bind(&pattern)
        .bind(filter)
        .fetch_one(pool)
        .await?;

    let items = sqlx::query_scalar(concat!(
        "SELECT jsonb_build_object(",
        carrier_fields!(),
        ")",
        filter!(),
        " ORDER BY c.updated_at DESC NULLS LAST, c.carrier_id LIMIT $3 OFFSET $4"
    ))
    .bind(&pattern)
    .bind(filter)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((items, total))
}

/// One carrier with how much it stores.
pub async fn carrier_detail(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar(concat!(
        "SELECT jsonb_build_object(",
        carrier_fields!(),
        ",
         'events', (SELECT count(*) FROM carrier_events e WHERE e.carrier = c.carrier_id),
         'commodities', (SELECT count(*) FROM market_commodities m WHERE m.carrier_id = c.carrier_id))
         FROM carriers c LEFT JOIN users u ON u.id = c.owner_id
         WHERE c.carrier_id = $1"
    ))
    .bind(carrier_id)
    .fetch_optional(pool)
    .await
}
