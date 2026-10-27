//! Owners' Discord webhooks (`carrier_webhooks`), one per carrier.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// A carrier's webhook as its owner sees it in the settings.
#[derive(sqlx::FromRow)]
pub struct Webhook {
    pub url: String,
    pub events: Vec<String>,
    pub low_fuel_threshold: i32,
    pub enabled: bool,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
}

/// What ingest needs to deliver notices.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Active {
    pub url: String,
    pub events: Vec<String>,
    pub low_fuel_threshold: i32,
}

impl Active {
    pub fn wants(&self, event: &str) -> bool {
        self.events.iter().any(|e| e == event)
    }
}

pub async fn get(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<Webhook>> {
    sqlx::query_as(
        "SELECT url, events, low_fuel_threshold, enabled, last_sent_at, last_error, last_error_at
         FROM carrier_webhooks WHERE carrier_id = $1",
    )
    .bind(carrier_id)
    .fetch_optional(pool)
    .await
}

/// The webhook to deliver to, if there is one that is switched on, was set up
/// by the carrier's current owner, and that owner's account isn't disabled.
pub async fn active(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<Active>> {
    sqlx::query_as(
        "SELECT w.url, w.events, w.low_fuel_threshold
         FROM carrier_webhooks w
         JOIN carriers c ON c.carrier_id = w.carrier_id AND c.owner_id = w.owner_id
         JOIN users u ON u.id = w.owner_id AND u.disabled_at IS NULL
         WHERE w.carrier_id = $1 AND w.enabled",
    )
    .bind(carrier_id)
    .fetch_optional(pool)
    .await
}

/// Create or replace a carrier's webhook and switch it on. `url` `None` keeps
/// the stored one; `false` when there is none to keep.
pub async fn save(
    pool: &PgPool,
    carrier_id: i64,
    owner_id: i64,
    url: Option<&str>,
    events: &[String],
    low_fuel_threshold: i32,
) -> sqlx::Result<bool> {
    let done = match url {
        Some(url) => {
            sqlx::query(
                "INSERT INTO carrier_webhooks (carrier_id, owner_id, url, events, low_fuel_threshold)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (carrier_id) DO UPDATE SET
                     owner_id = EXCLUDED.owner_id,
                     url = EXCLUDED.url,
                     events = EXCLUDED.events,
                     low_fuel_threshold = EXCLUDED.low_fuel_threshold,
                     enabled = true,
                     last_error = NULL,
                     last_error_at = NULL,
                     updated_at = now()",
            )
            .bind(carrier_id)
            .bind(owner_id)
            .bind(url)
            .bind(events)
            .bind(low_fuel_threshold)
            .execute(pool)
            .await?
        }
        None => {
            sqlx::query(
                "UPDATE carrier_webhooks SET
                     owner_id = $2,
                     events = $3,
                     low_fuel_threshold = $4,
                     enabled = true,
                     last_error = NULL,
                     last_error_at = NULL,
                     updated_at = now()
                 WHERE carrier_id = $1",
            )
            .bind(carrier_id)
            .bind(owner_id)
            .bind(events)
            .bind(low_fuel_threshold)
            .execute(pool)
            .await?
        }
    };
    Ok(done.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, carrier_id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM carrier_webhooks WHERE carrier_id = $1")
        .bind(carrier_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Record a delivery. Matches on the URL as well, so a result for a URL the
/// owner has since replaced doesn't land on the new one.
pub async fn record_sent(pool: &PgPool, carrier_id: i64, url: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE carrier_webhooks SET last_sent_at = now()
         WHERE carrier_id = $1 AND url = $2",
    )
    .bind(carrier_id)
    .bind(url)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failed delivery; `disable` switches the webhook off (Discord says
/// it no longer exists).
pub async fn record_error(
    pool: &PgPool,
    carrier_id: i64,
    url: &str,
    error: &str,
    disable: bool,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE carrier_webhooks SET
             last_error = $3,
             last_error_at = now(),
             enabled = enabled AND NOT $4
         WHERE carrier_id = $1 AND url = $2",
    )
    .bind(carrier_id)
    .bind(url)
    .bind(error)
    .bind(disable)
    .execute(pool)
    .await?;
    Ok(())
}
