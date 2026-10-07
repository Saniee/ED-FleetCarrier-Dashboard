mod auth;
mod carriers;
mod carrier_event;
mod carrier_stream;
mod market;
mod recent_events;
mod healthz;

use axum::{Router, response::Html, routing::{delete, get, post}};
use crate::app_state::AppState;

pub fn router(app: AppState) -> Router {
    let api = Router::new()
        .route("/api/healthz", get(healthz::healthz))
        .route("/api/carrier/event", post(carrier_event::post))
        .route("/api/market/event", post(market::post))
        .route("/api/carriers", get(carriers::list))
        .route("/api/carriers/mine", get(carriers::mine))
        .route(
            "/api/carriers/{callsign}",
            get(carriers::get).patch(carriers::update_settings),
        )
        .route(
            "/api/carriers/{callsign}/claim",
            post(carriers::claim).delete(carriers::release),
        )
        .route("/api/carriers/{callsign}/events", get(recent_events::get_events_by_callsign))
        .route("/api/carriers/{callsign}/market", get(market::get_by_callsign))
        .route("/api/carriers/{callsign}/stream", get(carrier_stream::stream_by_callsign))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/tokens", get(auth::list_tokens).post(auth::create_token))
        .route("/api/auth/tokens/{id}", delete(auth::delete_token));

    let mut router = Router::new().merge(api);

    // Barebones API test page, off unless asked for: it is a dev aid, not a
    // product surface.
    if std::env::var("DEV_CONSOLE").is_ok_and(|v| v == "1") {
        println!("Dev console enabled at /dev");
        router = router.route("/dev", get(|| async { Html(include_str!("../../dev/console.html")) }));
    }

    router.with_state(app)
}
