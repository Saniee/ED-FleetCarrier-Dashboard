use sqlx::{PgPool, types::Json};

use crate::journal_definitions::CarrierEvent;

/// Append one raw event to `carrier_events`, returning its `id` (stamped on the
/// carrier as `last_event_id`). `carrier_id` is resolved by the caller so the
/// foreign key holds.
pub async fn insert(
    pool: &PgPool,
    event: &CarrierEvent,
    carrier_id: Option<i64>,
) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "
        INSERT INTO carrier_events (timestamp, event_name, json_data, carrier)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        ",
    )
    .bind(event.timestamp())
    .bind(event.name())
    .bind(Json(event))
    .bind(carrier_id)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn recent(
    pool: &PgPool,
    carrier_id: i64,
    limit: i64
) -> sqlx::Result<Vec<serde_json::Value>> {
    sqlx::query_scalar(
        "
        SELECT to_jsonb(ce)
        FROM carrier_events ce
        WHERE ce.carrier = $1
        ORDER BY ce.id DESC
        LIMIT $2
        "
    )
    .bind(carrier_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}