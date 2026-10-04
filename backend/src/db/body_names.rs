use sqlx::PgPool;

/// What the cache knows about one body.
pub enum Cached {
    /// Never looked up.
    Unknown,
    /// Looked up, and EDSM had no such body.
    Absent,
    /// Looked up and found.
    Named {
        name: String,
        body_type: Option<String>,
    },
}

/// The cached name for a body.
///
/// This module owns `body_names` and nothing else.
pub async fn get(pool: &PgPool, system_address: i64, body_id: i64) -> sqlx::Result<Cached> {
    // Decoded as a tuple rather than a struct: a missing row and a row holding
    // NULLs mean different things here, and `fetch_optional` keeps them apart.
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT name, body_type FROM body_names WHERE system_address = $1 AND body_id = $2",
    )
    .bind(system_address)
    .bind(body_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        // Never asked.
        None => Cached::Unknown,
        // Asked, and EDSM had nothing.
        Some((None, _)) => Cached::Absent,
        Some((Some(name), body_type)) => Cached::Named { name, body_type },
    })
}

/// Remember the answer for a body. A `name` of `None` records a miss, so the
/// system is not re-fetched on the next jump.
pub async fn insert(
    pool: &PgPool,
    system_address: i64,
    body_id: i64,
    name: Option<&str>,
    body_type: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "
        INSERT INTO body_names (system_address, body_id, name, body_type, resolved_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (system_address, body_id) DO UPDATE SET
            name = EXCLUDED.name,
            body_type = EXCLUDED.body_type,
            resolved_at = now()
        ",
    )
    .bind(system_address)
    .bind(body_id)
    .bind(name)
    .bind(body_type)
    .execute(pool)
    .await?;

    Ok(())
}
