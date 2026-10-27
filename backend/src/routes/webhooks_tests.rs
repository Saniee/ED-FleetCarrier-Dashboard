//! Webhook settings and ingest-to-queue tests through the real router. Need
//! Postgres, like `admin_tests`.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use super::admin_tests::{call, carrier, user};
use crate::{app_state::AppState, discord::Delivery, edsm};

const HOOK: &str = "https://discord.com/api/webhooks/123/secret";

fn app(pool: &PgPool) -> (Router, mpsc::Receiver<Delivery>) {
    let (tx, _rx) = broadcast::channel(16);
    let (notify, deliveries) = mpsc::channel(16);
    let router = super::router(AppState {
        db_pool: pool.clone(),
        http: edsm::client(),
        tx,
        legacy_token: None,
        anon_ingest: false,
        registration_open: true,
        rate_limit: false,
        notify,
        public_origin: None,
    });
    (router, deliveries)
}

async fn ingest(app: &Router, token: &str, body: Value) -> StatusCode {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/carrier/event")
        .header("x-ingest-token", token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

#[sqlx::test]
async fn only_the_owner_manages_the_webhook(pool: PgPool) {
    let (app, _) = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    let (_, other) = user(&pool, "other", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    let uri = "/api/carriers/ABC-123/webhook";
    let body = json!({ "url": HOOK, "events": ["jump"], "low_fuel_threshold": 200 });

    assert_eq!(call(&app, Method::PUT, uri, Some(&other), Some(body.clone())).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::GET, uri, Some(&other), None).await.0, StatusCode::FORBIDDEN);

    let bad = json!({ "url": "https://evil.example/hook", "events": [], "low_fuel_threshold": 0 });
    assert_eq!(call(&app, Method::PUT, uri, Some(&session), Some(bad)).await.0, StatusCode::BAD_REQUEST);
    let no_url = json!({ "events": [], "low_fuel_threshold": 0 });
    assert_eq!(call(&app, Method::PUT, uri, Some(&session), Some(no_url)).await.0, StatusCode::BAD_REQUEST);

    assert_eq!(call(&app, Method::PUT, uri, Some(&session), Some(body)).await.0, StatusCode::NO_CONTENT);
    let (status, hook) = call(&app, Method::GET, uri, Some(&session), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hook["configured"], true);
    assert!(!hook["url"].as_str().unwrap().contains("secret"), "token is never shown");

    // The public carrier row doesn't carry it either.
    let (_, row) = call(&app, Method::GET, "/api/carriers/ABC-123", None, None).await;
    assert!(!row.to_string().contains("secret"));

    assert_eq!(call(&app, Method::DELETE, uri, Some(&session), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, uri, Some(&session), None).await.1["configured"], false);
}

#[sqlx::test]
async fn ingest_queues_a_jump_notice_for_the_current_owner_only(pool: PgPool) {
    let (app, mut deliveries) = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    let (_, created) = call(&app, Method::POST, "/api/auth/tokens", Some(&session), Some(json!({ "name": "plugin" }))).await;
    let token = created["token"].as_str().unwrap().to_owned();
    let settings = json!({ "url": HOOK, "events": ["jump"], "low_fuel_threshold": 0 });
    call(&app, Method::PUT, "/api/carriers/ABC-123/webhook", Some(&session), Some(settings)).await;

    let jump = |departure: &str| json!({
        "timestamp": Utc::now().to_rfc3339(), "event": "CarrierJumpRequest", "CarrierID": 42,
        "SystemName": "Sol", "Body": "Earth", "SystemAddress": 10477373803_i64, "BodyID": 3,
        "DepartureTime": departure,
    });

    assert_eq!(ingest(&app, &token, jump("2026-10-27T12:15:00Z")).await, StatusCode::ACCEPTED);
    let d = deliveries.try_recv().expect("a notice was queued");
    assert_eq!(d.url, HOOK);
    assert_eq!(d.body["embeds"][0]["title"], "Jump scheduled");

    // A repeat changes nothing, so it isn't announced again.
    assert_eq!(ingest(&app, &token, jump("2026-10-27T12:15:00Z")).await, StatusCode::ACCEPTED);
    assert!(deliveries.try_recv().is_err());

    // Released: the old owner's channel hears nothing more.
    sqlx::query("UPDATE carriers SET owner_id = NULL WHERE carrier_id = 42").execute(&pool).await.unwrap();
    ingest(&app, &token, jump("2026-10-27T13:00:00Z")).await;
    assert!(deliveries.try_recv().is_err());
}
