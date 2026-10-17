use tokio::sync::broadcast;

use crate::routes::router;

mod db;
mod routes;
mod journal_definitions;
mod app_state;
mod auth;
mod cli;
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

    // `backend admin ...` manages roles and exits instead of serving.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "admin") {
        let pool = db::connect(&std::env::var("DATABASE_URL").expect("Missing db url!")).await;
        if let Err(msg) = cli::admin(&pool, &args[1..]).await {
            eprintln!("{msg}");
            std::process::exit(2);
        }
        return;
    }

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

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Backend UP! Listening at {addr}");

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
