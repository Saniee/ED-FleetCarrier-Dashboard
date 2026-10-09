use tokio::sync::broadcast;

use crate::routes::router;

mod db;
mod routes;
mod journal_definitions;
mod app_state;
mod auth;
mod edsm;
mod error;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Ok("1" | "true" | "yes" | "on") => true,
        Ok("0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

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
        anon_ingest: env_flag("ALLOW_ANON_INGEST", false),
        registration_open: env_flag("ALLOW_REGISTRATION", true),
        rate_limit: env_flag("RATE_LIMIT", true),
    };

    if state.anon_ingest {
        eprintln!("WARNING: ALLOW_ANON_INGEST is set; ingest accepts requests with no token. Dev only.");
    }
    if !state.registration_open {
        println!("Registration is closed.");
    }

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Backend UP! Listening at 0.0.0.0:8080");

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
