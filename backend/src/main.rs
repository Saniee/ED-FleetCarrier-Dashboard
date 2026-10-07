use tokio::sync::broadcast;

use crate::routes::router;

mod db;
mod routes;
mod journal_definitions;
mod app_state;
mod auth;
mod edsm;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    // Load .env for a native `cargo run`. In a container the environment is
    // provided by the orchestrator and there is no .env, so its absence is fine.
    let _ = dotenvy::dotenv();
    let (tx, _rx) = broadcast::channel(100);

    println!("Connecting to db...");
    let pool = db::connect(&std::env::var("DATABASE_URL").expect("Missing db url!")).await;
    println!("Connected!");

    let state = app_state::AppState {
        db_pool: pool,
        http: edsm::client(),
        tx,
        legacy_token: std::env::var("TOKEN").ok().filter(|t| !t.trim().is_empty()),
    };

    if state.legacy_token.is_none() {
        eprintln!("WARNING: TOKEN is not set; token-less ingest requests are accepted.");
    }

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Backend UP! Listening at 0.0.0.0:8080");

    axum::serve(listener, router(state)).await.unwrap();
}
