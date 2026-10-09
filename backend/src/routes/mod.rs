mod auth;
mod carriers;
mod carrier_event;
mod carrier_stream;
mod market;
mod recent_events;
mod healthz;

use std::{sync::Arc, time::Duration};

use axum::{Router, response::Html, routing::{delete, get, post}};
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};

use crate::app_state::AppState;

/// Limit `router` per client IP: `burst` requests, then one more every `every`.
/// The IP comes from `X-Forwarded-For` / `X-Real-IP`, so keep the backend behind
/// the proxy; exposed directly, those headers can be forged.
fn limited<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    every: Duration,
    burst: u32,
) -> Router<S> {
    let config = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .period(every)
            .burst_size(burst)
            .finish()
            .expect("rate limit config is valid"),
    );

    // Forget clients that have fully replenished, or the table only grows.
    let limiter = config.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            limiter.retain_recent();
        }
    });

    router.layer(GovernorLayer::new(config))
}

pub fn router(app: AppState) -> Router {
    // Each group has its own bucket per IP. Login/register run argon2 and are the
    // brute-force target, so they are tightest; ingest can burst (journal
    // replays); the health check stays unlimited.
    let credentials = Router::new()
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login));

    let ingest = Router::new()
        .route("/api/carrier/event", post(carrier_event::post))
        .route("/api/market/event", post(market::post));

    let general: Router<AppState> = Router::new()
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
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/tokens", get(auth::list_tokens).post(auth::create_token))
        .route("/api/auth/tokens/{id}", delete(auth::delete_token));

    let (credentials, ingest, general) = if app.rate_limit {
        (
            limited(credentials, Duration::from_secs(6), 5),
            limited(ingest, Duration::from_millis(100), 60),
            limited(general, Duration::from_millis(50), 60),
        )
    } else {
        (credentials, ingest, general)
    };

    let mut router = Router::new()
        .route("/api/healthz", get(healthz::healthz))
        .merge(credentials)
        .merge(ingest)
        .merge(general);

    // Barebones API test page, off unless asked for: it is a dev aid, not a
    // product surface.
    if std::env::var("DEV_CONSOLE").is_ok_and(|v| v == "1") {
        println!("Dev console enabled at /dev");
        router = router.route("/dev", get(|| async { Html(include_str!("../../dev/console.html")) }));
    }

    router.with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    async fn status(app: &Router, ip: &str) -> StatusCode {
        let req = Request::get("/")
            .header("x-forwarded-for", ip)
            .body(Body::empty())
            .unwrap();
        app.clone().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn limit_is_per_client_ip() {
        let app: Router = limited(
            Router::new().route("/", get(|| async { "ok" })),
            Duration::from_secs(3600),
            2,
        );

        assert_eq!(status(&app, "1.1.1.1").await, StatusCode::OK);
        assert_eq!(status(&app, "1.1.1.1").await, StatusCode::OK);
        assert_eq!(status(&app, "1.1.1.1").await, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(status(&app, "2.2.2.2").await, StatusCode::OK);
    }
}
