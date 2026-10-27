use tokio::sync::broadcast;

use crate::routes::router;

mod db;
mod routes;
mod journal_definitions;
mod app_state;
mod auth;
mod cli;
mod discord;
mod edsm;
mod error;
mod finances;
mod hauling;
mod notify;

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

    let http = edsm::client();
    let state = app_state::AppState {
        notify: discord::spawn(pool.clone(), http.clone()),
        db_pool: pool,
        http,
        tx,
        legacy_token: std::env::var("TOKEN").ok().filter(|t| !t.trim().is_empty()),
        anon_ingest: env_flag("ALLOW_ANON_INGEST", false),
        registration_open: env_flag("ALLOW_REGISTRATION", true),
        rate_limit: env_flag("RATE_LIMIT", true),
        public_origin: std::env::var("PUBLIC_ORIGIN").ok().filter(|o| !o.trim().is_empty()),
    };

    if state.anon_ingest {
        eprintln!("WARNING: ALLOW_ANON_INGEST is set; ingest accepts requests with no token. Dev only.");
    }
    let registration = db::settings::get_bool(&state.db_pool, db::settings::REGISTRATION_OPEN)
        .await
        .ok()
        .flatten();
    if !registration.unwrap_or(state.registration_open) {
        let from = if registration.is_some() { "admin dashboard" } else { "ALLOW_REGISTRATION" };
        println!("Registration is closed ({from}).");
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
