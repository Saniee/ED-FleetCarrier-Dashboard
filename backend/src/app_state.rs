#![allow(dead_code)]

use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::db::carriers::CarrierSnapshot;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    /// Fan-out of the current carrier table to live subscribers. Every applied
    /// event publishes a freshly read snapshot here.
    pub tx: broadcast::Sender<CarrierSnapshot>,
}
