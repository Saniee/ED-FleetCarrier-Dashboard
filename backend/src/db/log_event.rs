use sqlx::{PgPool, types::Json};

use crate::journal_definitions::CarrierEvent;

/// Append one raw event to the `carrier_events` history.
///
/// This module owns the `carrier_events` table and nothing else. `carrier_id`
/// is resolved by the caller (`db::carriers`) so that the foreign key on
/// `carrier_events.carrier` is satisfied before the row is written.
///
/// Returns the new row's `id`, which callers stamp onto the carrier as
/// `last_event_id`.
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
