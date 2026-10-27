//! Owner finance tests through the real router. Need Postgres, like
//! `admin_tests`.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use super::admin_tests::{call, carrier, user};
use crate::{app_state::AppState, discord::Delivery, edsm, finances};

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

async fn log(pool: &PgPool, carrier_id: i64, at: chrono::DateTime<Utc>, event: Value) {
    sqlx::query("INSERT INTO carrier_events (timestamp, event_name, json_data, carrier) VALUES ($1, $2, $3, $4)")
        .bind(at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .bind(event["event"].as_str().unwrap())
        .bind(&event)
        .bind(carrier_id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn only_the_owner_sets_upkeep(pool: PgPool) {
    let (app, _) = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    let (_, other) = user(&pool, "other", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    let uri = "/api/carriers/ABC-123";

    let set = |n: i64| Some(json!({ "weekly_upkeep": n }));
    assert_eq!(call(&app, Method::PATCH, uri, Some(&other), set(10_000_000)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::PATCH, uri, Some(&session), set(-1)).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::PATCH, uri, Some(&session), set(10_000_000)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, uri, None, None).await.1["weekly_upkeep"], 10_000_000);

    // Other settings leave it alone; 0 clears it.
    call(&app, Method::PATCH, uri, Some(&session), Some(json!({ "is_squadron": true }))).await;
    assert_eq!(call(&app, Method::GET, uri, None, None).await.1["weekly_upkeep"], 10_000_000);
    call(&app, Method::PATCH, uri, Some(&session), set(0)).await;
    assert_eq!(call(&app, Method::GET, uri, None, None).await.1["weekly_upkeep"], Value::Null);
}

#[sqlx::test]
async fn history_leaves_out_the_owners_transfers(pool: PgPool) {
    let (app, _) = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    let (_, other) = user(&pool, "other", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;

    let t = finances::week_start(Utc::now());
    let transfer = json!({
        "event": "CarrierBankTransfer", "CarrierID": 42, "Deposit": 50_000_000, "Withdraw": null,
        "PlayerBalance": 0, "CarrierBalance": 150_000_000,
    });
    log(&pool, 42, t + Duration::seconds(1), json!({ "event": "CarrierStats", "Finance": { "CarrierBalance": 100_000_000 } })).await;
    log(&pool, 42, t + Duration::seconds(2), transfer.clone()).await;
    // Resent by the plugin: logged twice, counted once.
    log(&pool, 42, t + Duration::seconds(2), transfer).await;
    log(&pool, 42, t + Duration::seconds(3), json!({ "event": "CarrierFinance", "CarrierBalance": 140_000_000 })).await;

    let uri = "/api/carriers/ABC-123/finances?weeks=2";
    assert_eq!(call(&app, Method::GET, uri, Some(&other), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::GET, uri, None, None).await.0, StatusCode::UNAUTHORIZED);

    let (status, body) = call(&app, Method::GET, uri, Some(&session), None).await;
    assert_eq!(status, StatusCode::OK);
    let weeks = body["weeks"].as_array().unwrap();
    assert_eq!(weeks.len(), 2);
    assert_eq!(weeks[0]["earned"], Value::Null);
    assert_eq!(weeks[1]["earned"], -10_000_000);
    assert_eq!(weeks[1]["transferred"], 50_000_000);
    assert_eq!(weeks[1]["closing"], 140_000_000);
    assert_eq!(body["transfers"].as_array().unwrap().len(), 1);
    assert_eq!(body["transfers"][0]["amount"], 50_000_000);
}

#[sqlx::test]
async fn low_balance_notice_uses_the_owners_upkeep(pool: PgPool) {
    let (app, mut deliveries) = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    sqlx::query("UPDATE carriers SET carrier_balance = 50000000, weekly_upkeep = 10000000 WHERE carrier_id = 42")
        .execute(&pool)
        .await
        .unwrap();
    let (_, created) = call(&app, Method::POST, "/api/auth/tokens", Some(&session), Some(json!({ "name": "plugin" }))).await;
    let token = created["token"].as_str().unwrap().to_owned();
    let settings = json!({
        "url": "https://discord.com/api/webhooks/123/secret", "events": ["low_balance"],
        "low_fuel_threshold": 0, "low_balance_weeks": 4,
    });
    call(&app, Method::PUT, "/api/carriers/ABC-123/webhook", Some(&session), Some(settings)).await;

    let finance = |balance: i64| {
        let body = json!({
            "timestamp": Utc::now().to_rfc3339(), "event": "CarrierFinance", "CarrierID": 42,
            "CarrierBalance": balance, "ReserveBalance": 0, "AvailableBalance": balance, "ReservePercent": 0,
        });
        Request::builder()
            .method(Method::POST)
            .uri("/api/carrier/event")
            .header("x-ingest-token", &token)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    assert_eq!(app.clone().oneshot(finance(45_000_000)).await.unwrap().status(), StatusCode::ACCEPTED);
    assert!(deliveries.try_recv().is_err(), "still above four weeks");

    assert_eq!(app.clone().oneshot(finance(35_000_000)).await.unwrap().status(), StatusCode::ACCEPTED);
    let d = deliveries.try_recv().expect("a notice was queued");
    assert_eq!(d.body["embeds"][0]["title"], "Low carrier balance");

    app.clone().oneshot(finance(30_000_000)).await.unwrap();
    assert!(deliveries.try_recv().is_err(), "announced once on the way down");
}
