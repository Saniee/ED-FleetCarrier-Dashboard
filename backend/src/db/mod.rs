pub mod log_event;
pub mod carriers;
pub mod market;
pub mod commodities;
pub mod body_names;

use sqlx::PgPool;

pub async fn connect(database_url: &str) -> PgPool {
    let pool = PgPool::connect(database_url)
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}