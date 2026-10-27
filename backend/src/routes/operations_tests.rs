//! Hauling operation tests through the real router. Need Postgres, like
//! `admin_tests`.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use super::admin_tests::{call, carrier, user};
use crate::{app_state::AppState, edsm};

fn app(pool: &PgPool) -> Router {
    let (tx, _rx) = broadcast::channel(16);
    super::router(AppState {
        db_pool: pool.clone(),
        http: edsm::client(),
        tx,
        legacy_token: None,
        anon_ingest: false,
        registration_open: true,
        rate_limit: false,
        notify: mpsc::channel(16).0,
        public_origin: None,
    })
}

async fn hauling(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/hauling/event")
        .header("x-ingest-token", token)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn token(app: &Router, session: &str) -> String {
    let (_, created) = call(app, Method::POST, "/api/auth/tokens", Some(session), Some(json!({ "name": "plugin" }))).await;
    created["token"].as_str().unwrap().to_owned()
}

fn sell(carrier: i64, commodity: &str, count: i64, at: &str) -> Value {
    json!({ "timestamp": at, "event": "MarketSell", "MarketID": carrier, "Type": commodity, "Count": count, "SellPrice": 1 })
}

#[sqlx::test]
async fn manual_operation_counts_sales_once_and_completes(pool: PgPool) {
    let app = app(&pool);
    let (owner, owner_session) = user(&pool, "owner", "user").await;
    let (_, hauler_session) = user(&pool, "hauler", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    let hauler = token(&app, &hauler_session).await;

    let body = json!({ "name": "Steel run", "targets": [{ "commodity": "Steel", "required": 1000 }] });
    assert_eq!(
        call(&app, Method::POST, "/api/carriers/ABC-123/operations", Some(&hauler_session), Some(body.clone())).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, created) = call(&app, Method::POST, "/api/carriers/ABC-123/operations", Some(&owner_session), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let uri = format!("/api/operations/{}", created["id"]);

    // Timestamps after the operation started; the same sale twice counts once.
    let later = (chrono::Utc::now() + chrono::Duration::seconds(5)).to_rfc3339();
    assert_eq!(hauling(&app, &hauler, sell(42, "steel", 600, &later)).await.1["stored"], true);
    assert_eq!(hauling(&app, &hauler, sell(42, "steel", 600, &later)).await.1["stored"], false);
    // A station that isn't a tracked carrier.
    assert_eq!(hauling(&app, &hauler, sell(7, "steel", 600, &later)).await.1["stored"], false);

    let (_, op) = call(&app, Method::GET, &uri, None, None).await;
    assert_eq!(op["progress"]["targets"][0]["provided"], 600);
    assert_eq!(op["progress"]["contributors"][0]["username"], "hauler");
    assert_eq!(op["can_manage"], false);

    let even_later = (chrono::Utc::now() + chrono::Duration::seconds(10)).to_rfc3339();
    hauling(&app, &hauler, sell(42, "steel", 400, &even_later)).await;
    let (_, op) = call(&app, Method::GET, &uri, Some(&owner_session), None).await;
    assert_eq!(op["operation"]["status"], "complete");
    assert_eq!(op["can_manage"], true);
}

#[sqlx::test]
async fn operations_follow_carrier_visibility(pool: PgPool) {
    let app = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "owner_only").await;
    let (_, created) = call(
        &app,
        Method::POST,
        "/api/carriers/ABC-123/operations",
        Some(&session),
        Some(json!({ "name": "Secret", "targets": [] })),
    )
    .await;
    let uri = format!("/api/operations/{}", created["id"]);

    assert_eq!(call(&app, Method::GET, &uri, None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123/operations", None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, &uri, Some(&session), None).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn depot_operation_tracks_the_depot(pool: PgPool) {
    let app = app(&pool);
    let (owner, session) = user(&pool, "owner", "user").await;
    carrier(&pool, 42, "ABC-123", Some(owner), "public").await;
    let plugin = token(&app, &session).await;
    let depot = |provided: i64, complete: bool, at: &str| json!({
        "timestamp": at, "event": "ColonisationConstructionDepot", "MarketID": 900,
        "ConstructionProgress": provided as f64 / 100.0, "ConstructionComplete": complete, "ConstructionFailed": false,
        "ResourcesRequired": [{ "Name": "$Steel_name;", "Name_Localised": "Steel", "RequiredAmount": 100, "ProvidedAmount": provided, "Payment": 1 }],
        "_StarSystem": "Col 285", "_StationName": "Site",
    });

    assert_eq!(hauling(&app, &plugin, depot(10, false, "2026-10-27T12:00:00Z")).await.1["stored"], true);
    // Older than what's stored: ignored.
    assert_eq!(hauling(&app, &plugin, depot(5, false, "2026-10-27T11:00:00Z")).await.1["stored"], false);

    let (_, found) = call(&app, Method::GET, "/api/depots?q=col", Some(&session), None).await;
    assert_eq!(found[0]["market_id"], 900);

    let (_, created) = call(
        &app,
        Method::POST,
        "/api/carriers/ABC-123/operations",
        Some(&session),
        Some(json!({ "name": "Outpost", "depot_market_id": 900 })),
    )
    .await;
    let uri = format!("/api/operations/{}", created["id"]);
    let (_, op) = call(&app, Method::GET, &uri, None, None).await;
    assert_eq!(op["progress"]["targets"][0]["remaining"], 90);

    hauling(&app, &plugin, depot(100, true, "2026-10-27T13:00:00Z")).await;
    let (_, op) = call(&app, Method::GET, &uri, None, None).await;
    assert_eq!(op["operation"]["status"], "complete");
}
