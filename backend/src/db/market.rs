use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use crate::journal_definitions::MarketEvent;

/// Upsert the header of a carrier's market — the `event` half of a `Market`
/// payload.
///
/// Takes a connection rather than a pool so the caller can run this and
/// `commodities::replace` inside one transaction: the header and the commodity
/// list are two halves of the same snapshot and must never disagree.
///
/// This module owns `carrier_markets` and nothing else.
pub async fn upsert(conn: &mut PgConnection, e: &MarketEvent) -> sqlx::Result<i64> {
    // For a fleet carrier the journal's `MarketID` equals its `CarrierID`, so the
    // same number keys both this row and the `carriers` row it hangs off.
    sqlx::query_scalar(
        "
        INSERT INTO carrier_markets (
            carrier_id, market_id, station_name, station_type, star_system,
            carrier_docking_access, timestamp, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            market_id = EXCLUDED.market_id,
            station_name = EXCLUDED.station_name,
            station_type = EXCLUDED.station_type,
            star_system = EXCLUDED.star_system,
            carrier_docking_access = EXCLUDED.carrier_docking_access,
            timestamp = EXCLUDED.timestamp,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.market_id)
    .bind(e.market_id)
    .bind(&e.station_name)
    .bind(&e.station_type)
    .bind(&e.star_system)
    .bind(&e.carrier_docking_access)
    .bind(&e.timestamp)
    .fetch_one(&mut *conn)
    .await
}

/// The market header for a carrier, as JSON. `None` when no `Market` event has
/// been seen for it yet.
pub async fn get(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar("SELECT to_jsonb(m) FROM carrier_markets m WHERE carrier_id = $1")
        .bind(carrier_id)
        .fetch_optional(pool)
        .await
}
