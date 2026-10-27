//! Balance snapshots for `finances::weekly`, read from the logged journal events.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::finances::Snapshot;

// The balance sits under `Finance` in CarrierStats and at the top level in the
// other two. Event timestamps are stored as `YYYY-MM-DDTHH:MM:SSZ` text (see
// migration 0002), which sorts and compares as time does.
macro_rules! snapshots {
    ($rest:literal) => {
        concat!(
            "SELECT timestamp,
                    COALESCE((json_data->'Finance'->>'CarrierBalance')::bigint,
                             (json_data->>'CarrierBalance')::bigint) AS balance,
                    COALESCE((json_data->>'Deposit')::bigint, 0)
                      - COALESCE((json_data->>'Withdraw')::bigint, 0) AS transfer
             FROM carrier_events
             WHERE carrier = $1
               AND event_name IN ('CarrierStats', 'CarrierFinance', 'CarrierBankTransfer')
               AND ",
            $rest
        )
    };
}

type Row = (String, Option<i64>, i64);

/// Every balance reported since `since`, plus the last one before it so the
/// first week has something to measure against.
pub async fn snapshots(pool: &PgPool, carrier_id: i64, since: DateTime<Utc>) -> sqlx::Result<Vec<Snapshot>> {
    let since = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let before: Option<Row> = sqlx::query_as(snapshots!("timestamp < $2 ORDER BY timestamp DESC LIMIT 1"))
        .bind(carrier_id)
        .bind(&since)
        .fetch_optional(pool)
        .await?;
    let rows: Vec<Row> = sqlx::query_as(snapshots!("timestamp >= $2 ORDER BY timestamp"))
        .bind(carrier_id)
        .bind(&since)
        .fetch_all(pool)
        .await?;

    Ok(before
        .into_iter()
        .chain(rows)
        .filter_map(|(at, balance, transfer)| {
            Some(Snapshot { at: DateTime::parse_from_rfc3339(&at).ok()?.to_utc(), balance: balance?, transfer })
        })
        .collect())
}

/// The owner's transfers since `since`, newest first, for the finance page.
pub async fn transfers(pool: &PgPool, carrier_id: i64, since: DateTime<Utc>, limit: i64) -> sqlx::Result<Vec<Row>> {
    sqlx::query_as(
        "SELECT DISTINCT timestamp,
                (json_data->>'CarrierBalance')::bigint,
                COALESCE((json_data->>'Deposit')::bigint, 0) - COALESCE((json_data->>'Withdraw')::bigint, 0)
         FROM carrier_events
         WHERE carrier = $1 AND event_name = 'CarrierBankTransfer' AND timestamp >= $2
         ORDER BY timestamp DESC
         LIMIT $3",
    )
    .bind(carrier_id)
    .bind(since.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    .bind(limit)
    .fetch_all(pool)
    .await
}
