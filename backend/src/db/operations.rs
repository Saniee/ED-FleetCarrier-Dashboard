//! Hauling operations, construction depots and the deliveries that count
//! towards them. See migration 0018.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};

use crate::{
    hauling::{self, Delivered, Progress, Target},
    journal_definitions::{ConstructionDepot, commodity_key},
};

#[derive(sqlx::FromRow, Serialize, Clone, Debug)]
pub struct Operation {
    pub id: i64,
    pub carrier_id: i64,
    pub name: String,
    pub depot_market_id: Option<i64>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A target the owner entered: `commodity` is already a key (`steel`).
pub struct NewTarget {
    pub commodity: String,
    pub name: String,
    pub required: i32,
}

/// The `Operation` columns, as a literal so queries stay static strings.
macro_rules! columns {
    () => {
        "id, carrier_id, name, depot_market_id, status, created_at, completed_at"
    };
}

/// Store a depot report unless a newer one is already stored. `true` when it was
/// written.
pub async fn upsert_depot(pool: &PgPool, e: &ConstructionDepot, at: DateTime<Utc>) -> sqlx::Result<bool> {
    let resources: Vec<Value> = e
        .resources_required
        .iter()
        .map(|r| {
            json!({
                "commodity": commodity_key(&r.name),
                "name": r.name_localised.clone().unwrap_or_else(|| commodity_key(&r.name)),
                "required": r.required_amount,
                "provided": r.provided_amount,
                "payment": r.payment,
            })
        })
        .collect();

    let done = sqlx::query(
        "INSERT INTO construction_depots
             (market_id, star_system, station_name, progress, complete, failed, resources, reported_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (market_id) DO UPDATE SET
             star_system = COALESCE(EXCLUDED.star_system, construction_depots.star_system),
             station_name = COALESCE(EXCLUDED.station_name, construction_depots.station_name),
             progress = EXCLUDED.progress,
             complete = EXCLUDED.complete,
             failed = EXCLUDED.failed,
             resources = EXCLUDED.resources,
             reported_at = EXCLUDED.reported_at,
             updated_at = now()
         WHERE construction_depots.reported_at <= EXCLUDED.reported_at",
    )
    .bind(e.market_id)
    .bind(&e.star_system)
    .bind(&e.station_name)
    .bind(e.construction_progress)
    .bind(e.construction_complete)
    .bind(e.construction_failed)
    .bind(Json(resources))
    .bind(at)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn depot(pool: &PgPool, market_id: i64) -> sqlx::Result<Option<Value>> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'market_id', market_id, 'star_system', star_system, 'station_name', station_name,
             'progress', progress, 'complete', complete, 'failed', failed,
             'resources', resources, 'updated_at', updated_at)
         FROM construction_depots WHERE market_id = $1",
    )
    .bind(market_id)
    .fetch_optional(pool)
    .await
}

/// Depots to pick from when starting an operation, most recently reported
/// first. `search` matches the system or station.
pub async fn search_depots(pool: &PgPool, search: Option<&str>, limit: i64) -> sqlx::Result<Vec<Value>> {
    let pattern = search.map(|s| {
        let escaped = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'market_id', market_id, 'star_system', star_system, 'station_name', station_name,
             'progress', progress, 'complete', complete, 'updated_at', updated_at)
         FROM construction_depots
         WHERE NOT complete AND NOT failed AND
               ($1::text IS NULL OR star_system ILIKE $1 OR station_name ILIKE $1)
         ORDER BY updated_at DESC LIMIT $2",
    )
    .bind(pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Commodity names seen anywhere, for the target picker: `{ commodity, name }`.
pub async fn known_commodities(pool: &PgPool) -> sqlx::Result<Vec<Value>> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('commodity', commodity, 'name', max(name))
         FROM (
             SELECT lower(regexp_replace(name, '^\\$|_name;$', '', 'g')) AS commodity,
                    coalesce(name_localised, name) AS name
             FROM market_commodities
             UNION ALL
             SELECT r->>'commodity', r->>'name'
             FROM construction_depots, jsonb_array_elements(resources) r
         ) c
         GROUP BY commodity ORDER BY max(name)",
    )
    .fetch_all(pool)
    .await
}

/// Record a delivery; `false` when it was already recorded (a resend or replay).
#[allow(clippy::too_many_arguments)]
pub async fn record_delivery(
    pool: &PgPool,
    user_id: i64,
    kind: &str,
    market_id: i64,
    commodity: &str,
    name: Option<&str>,
    amount: i64,
    at: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "INSERT INTO hauling_deliveries (user_id, kind, market_id, commodity, name, amount, delivered_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(kind)
    .bind(market_id)
    .bind(commodity)
    .bind(name)
    .bind(i32::try_from(amount).unwrap_or(i32::MAX))
    .bind(at)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn create(
    pool: &PgPool,
    carrier_id: i64,
    user_id: i64,
    name: &str,
    depot_market_id: Option<i64>,
    targets: &[NewTarget],
) -> sqlx::Result<i64> {
    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO operations (carrier_id, created_by, name, depot_market_id)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(carrier_id)
    .bind(user_id)
    .bind(name)
    .bind(depot_market_id)
    .fetch_one(&mut *tx)
    .await?;
    replace_targets(&mut tx, id, targets).await?;
    tx.commit().await?;
    Ok(id)
}

async fn replace_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    targets: &[NewTarget],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM operation_targets WHERE operation_id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for t in targets {
        sqlx::query(
            "INSERT INTO operation_targets (operation_id, commodity, name, required)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (operation_id, commodity) DO UPDATE SET required = EXCLUDED.required",
        )
        .bind(id)
        .bind(&t.commodity)
        .bind(&t.name)
        .bind(t.required)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn get(pool: &PgPool, id: i64) -> sqlx::Result<Option<Operation>> {
    sqlx::query_as(concat!("SELECT ", columns!(), " FROM operations WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// A carrier's operations, newest first; archived ones only if asked for.
pub async fn list(pool: &PgPool, carrier_id: i64, archived: bool) -> sqlx::Result<Vec<Operation>> {
    sqlx::query_as(concat!(
        "SELECT ", columns!(), " FROM operations
         WHERE carrier_id = $1 AND ($2 OR status <> 'archived')
         ORDER BY status = 'active' DESC, created_at DESC"
    ))
    .bind(carrier_id)
    .bind(archived)
    .fetch_all(pool)
    .await
}

/// Change an operation; `None` leaves a field. Completing stamps `completed_at`,
/// reopening clears it.
pub async fn update(
    pool: &PgPool,
    id: i64,
    name: Option<&str>,
    status: Option<&str>,
    targets: Option<&[NewTarget]>,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE operations SET
             name = COALESCE($2, name),
             status = COALESCE($3, status),
             completed_at = CASE
                 WHEN $3 = 'complete' AND status <> 'complete' THEN now()
                 WHEN $3 = 'active' THEN NULL
                 ELSE completed_at END
         WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(status)
    .execute(&mut *tx)
    .await?;
    if let Some(targets) = targets {
        replace_targets(&mut tx, id, targets).await?;
    }
    tx.commit().await
}

/// Mark an active operation complete. `true` if this call did it, so the
/// announcement goes out once.
pub async fn complete(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE operations SET status = 'complete', completed_at = now()
         WHERE id = $1 AND status = 'active'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM operations WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

/// Active operations a delivery or depot report could finish.
pub async fn active_touching(pool: &PgPool, carrier_id: Option<i64>, depot: Option<i64>) -> sqlx::Result<Vec<Operation>> {
    sqlx::query_as(concat!(
        "SELECT ", columns!(), " FROM operations
         WHERE status = 'active' AND (carrier_id = $1 OR depot_market_id = $2)"
    ))
    .bind(carrier_id)
    .bind(depot)
    .fetch_all(pool)
    .await
}

/// Work out an operation's progress, with its depot's row if it has one.
pub async fn progress(pool: &PgPool, op: &Operation) -> sqlx::Result<(Progress, Option<Value>)> {
    let depot = match op.depot_market_id {
        Some(id) => depot(pool, id).await?,
        None => None,
    };

    let targets: Vec<Target> = match &depot {
        Some(d) => d["resources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(Target {
                    commodity: r["commodity"].as_str()?.to_string(),
                    name: r["name"].as_str()?.to_string(),
                    required: r["required"].as_i64()?,
                    provided: r["provided"].as_i64(),
                })
            })
            .collect(),
        None => sqlx::query_as::<_, (String, String, i32)>(
            "SELECT commodity, name, required FROM operation_targets
             WHERE operation_id = $1 ORDER BY name",
        )
        .bind(op.id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(commodity, name, required)| Target { commodity, name, required: required.into(), provided: None })
        .collect(),
    };

    let stock: HashMap<String, i64> = sqlx::query_as::<_, (String, Option<i64>)>(
        "SELECT name, stock FROM market_commodities WHERE carrier_id = $1",
    )
    .bind(op.carrier_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, stock)| (commodity_key(&name), stock.unwrap_or(0)))
    .collect();

    let delivered: Vec<Delivered> = sqlx::query_as::<_, (i64, String, String, String, i64)>(
        "SELECT d.user_id, u.username, d.kind, d.commodity, sum(d.amount)::bigint
         FROM hauling_deliveries d JOIN users u ON u.id = d.user_id
         WHERE d.delivered_at >= $1 AND (
             (d.kind = 'carrier' AND d.market_id = $2) OR
             (d.kind = 'depot' AND d.market_id = $3))
         GROUP BY d.user_id, u.username, d.kind, d.commodity",
    )
    .bind(op.created_at)
    .bind(op.carrier_id)
    .bind(op.depot_market_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(user_id, username, kind, commodity, amount)| Delivered { user_id, username, kind, commodity, amount })
    .collect();

    Ok((hauling::compute(targets, &stock, &delivered), depot))
}
