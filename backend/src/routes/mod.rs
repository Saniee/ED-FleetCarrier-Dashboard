mod admin;
#[cfg(test)]
mod admin_tests;
mod auth;
mod carriers;
mod carrier_event;
mod carrier_stream;
mod finances;
mod market;
mod operations;
mod plugin;
mod recent_events;
mod webhooks;
#[cfg(test)]
mod webhooks_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod finances_tests;
mod healthz;

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    extract::ConnectInfo,
    http::Request,
    response::Html,
    routing::{delete, get, post, put},
};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder, key_extractor::KeyExtractor,
};

use crate::app_state::AppState;

/// The client IP as seen by the reverse proxy in front of the backend.
///
/// Takes the *last* `X-Forwarded-For` entry: the one the nearest proxy added.
/// Earlier entries come from the client and can be anything, and a proxy that
/// appends (nginx's `proxy_add_x_forwarded_for`) leaves them in place, so the
/// first entry would let a client pick its own rate-limit bucket. Falls back to
/// `X-Real-IP`, then the socket peer. Exposed without a proxy, both headers can
/// be forged, which is why compose binds the backend to loopback.
#[derive(Clone, Copy)]
struct ProxyIpKeyExtractor;

impl KeyExtractor for ProxyIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<IpAddr, GovernorError> {
        let headers = req.headers();
        let header_ip = |name: &str, last: bool| {
            let value = headers.get(name)?.to_str().ok()?;
            let mut entries = value.split(',').map(str::trim);
            let entry = if last { entries.next_back() } else { entries.next() }?;
            entry.parse::<IpAddr>().ok()
        };

        header_ip("x-forwarded-for", true)
            .or_else(|| header_ip("x-real-ip", false))
            .or_else(|| {
                req.extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ConnectInfo(addr)| addr.ip())
            })
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Limit `router` per client IP: `burst` requests, then one more every `every`.
fn limited<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    every: Duration,
    burst: u32,
) -> Router<S> {
    let config = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(ProxyIpKeyExtractor)
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
    // Each group has its own bucket per IP. Login/register (and token
    // regeneration) run argon2 and are the brute-force target, so they are tightest (with the
    // webhook test, which posts to Discord); ingest can burst (journal
    // replays); the health check stays unlimited.
    let credentials = Router::new()
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        // Checks the password too, so it shares the brute-force bucket.
        .route("/api/auth/tokens/{id}/regenerate", post(auth::regenerate_token))
        // Posts to Discord on the owner's behalf; kept slow so it can't be
        // used to flood a channel.
        .route("/api/carriers/{callsign}/webhook/test", post(webhooks::test));

    let ingest = Router::new()
        .route("/api/carrier/event", post(carrier_event::post))
        .route("/api/market/event", post(market::post))
        .route("/api/hauling/event", post(operations::post_event));

    let general: Router<AppState> = Router::new()
        .route("/api/carriers", get(carriers::list))
        .route("/api/carriers/mine", get(carriers::mine))
        .route(
            "/api/carriers/{callsign}",
            get(carriers::get).patch(carriers::update_settings),
        )
        // Claiming happens only through ingest (the owner's CarrierStats). A
        // claim-by-callsign endpoint let any account take over any unowned
        // carrier, so only releasing is exposed here.
        .route("/api/carriers/{callsign}/claim", delete(carriers::release))
        .route("/api/carriers/{callsign}/events", get(recent_events::get_events_by_callsign))
        .route("/api/carriers/{callsign}/market", get(market::get_by_callsign))
        .route("/api/carriers/{callsign}/stream", get(carrier_stream::stream_by_callsign))
        .route(
            "/api/carriers/{callsign}/operations",
            get(operations::list).post(operations::create),
        )
        .route(
            "/api/operations/{id}",
            get(operations::get).patch(operations::update).delete(operations::delete),
        )
        .route("/api/depots", get(operations::depots))
        .route("/api/commodities", get(operations::commodities))
        .route(
            "/api/carriers/{callsign}/webhook",
            get(webhooks::get).put(webhooks::put).delete(webhooks::delete),
        )
        .route("/api/carriers/{callsign}/finances", get(finances::history))
        .route("/api/systems", get(finances::system))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/plugin", get(plugin::info))
        .route("/api/plugin/download", get(plugin::download))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/tokens", get(auth::list_tokens).post(auth::create_token))
        .route("/api/auth/tokens/{id}", delete(auth::delete_token))
        .route("/api/admin/users", get(admin::list_users))
        .route("/api/admin/users/{id}", get(admin::get_user).patch(admin::update_user).delete(admin::delete_user))
        .route("/api/admin/users/{id}/disable", post(admin::disable_user))
        .route("/api/admin/users/{id}/enable", post(admin::enable_user))
        .route("/api/admin/users/{id}/sessions", delete(admin::revoke_sessions))
        .route("/api/admin/users/{id}/tokens/{token_id}", delete(admin::revoke_token))
        .route("/api/admin/settings", get(admin::get_settings).put(admin::update_settings))
        .route("/api/admin/audit", get(admin::list_audit))
        .route("/api/admin/carriers", get(admin::list_carriers))
        .route("/api/admin/carriers/{id}", get(admin::get_carrier).delete(admin::delete_carrier))
        .route("/api/admin/carriers/{id}/owner", put(admin::set_carrier_owner))
        .route("/api/admin/carriers/{id}/hide", post(admin::hide_carrier))
        .route("/api/admin/carriers/{id}/unhide", post(admin::unhide_carrier))
        .route("/api/admin/carriers/{id}/events", delete(admin::trim_carrier_events))
        .route("/api/admin/stats", get(admin::stats));

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

    /// An appending proxy keeps whatever the client sent in front of the real
    /// address, so only the last entry may pick the bucket.
    #[tokio::test]
    async fn spoofed_forwarded_for_entries_are_ignored() {
        let app: Router = limited(
            Router::new().route("/", get(|| async { "ok" })),
            Duration::from_secs(3600),
            1,
        );

        assert_eq!(status(&app, "6.6.6.1, 1.1.1.1").await, StatusCode::OK);
        assert_eq!(status(&app, "6.6.6.2, 1.1.1.1").await, StatusCode::TOO_MANY_REQUESTS);
    }
}
