//! Admin API tests through the real router. Each `#[sqlx::test]` gets its own
//! throwaway database (created from `DATABASE_URL`, migrations applied), so
//! these need Postgres and a role allowed to create databases.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::broadcast;
use tower::ServiceExt;

use crate::{
    app_state::AppState,
    auth::{hash_token, new_token},
    db::users,
    edsm,
};

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
    })
}

/// A user with a live session; returns `(id, session token)`. The password hash
/// is a placeholder: these tests never log in.
async fn user(pool: &PgPool, name: &str, role: &str) -> (i64, String) {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, 'x', $2) RETURNING id",
    )
    .bind(name)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap();
    let token = new_token();
    users::create_session(pool, &hash_token(&token), id, 3600).await.unwrap();
    (id, token)
}

async fn call(app: &Router, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn audit(pool: &PgPool, target: i64) -> Vec<String> {
    sqlx::query_scalar("SELECT action FROM admin_actions WHERE target_id = $1 ORDER BY id")
        .bind(target)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn only_admins_get_in(pool: PgPool) {
    let app = app(&pool);
    let (_, plain) = user(&pool, "plain", "user").await;
    let (_, boss) = user(&pool, "boss", "admin").await;

    assert_eq!(call(&app, Method::GET, "/api/admin/users", None, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, Method::GET, "/api/admin/users", Some(&plain), None).await.0, StatusCode::FORBIDDEN);
    let (status, body) = call(&app, Method::GET, "/api/admin/users", Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2);
}

#[sqlx::test]
async fn list_search_and_detail(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr_jameson", "user").await;
    user(&pool, "other", "user").await;
    // `_` is a LIKE wildcard; the search must treat it literally.
    let (_, body) = call(&app, Method::GET, "/api/admin/users?q=r_j", Some(&boss), None).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["username"], "cmdr_jameson");

    call(&app, Method::POST, "/api/auth/tokens", Some(&cmdr_session), Some(json!({ "name": "plugin" }))).await;
    let (status, detail) = call(&app, Method::GET, &format!("/api/admin/users/{cmdr}"), Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["tokens"], 1);
    assert_eq!(detail["active_sessions"], 1);
    assert_eq!(detail["token_list"][0]["name"], "plugin");
    // Metadata only: no secret or hash leaves the server.
    let token = detail["token_list"][0].as_object().unwrap();
    assert!(!token.contains_key("token") && !token.contains_key("token_hash"));

    assert_eq!(call(&app, Method::GET, "/api/admin/users/999999", Some(&boss), None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn disable_signs_out_and_hides_carriers_until_enabled(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr", "user").await;
    sqlx::query("INSERT INTO carriers (carrier_id, callsign, name, owner_id, visibility) VALUES (1, 'ABC-123', 'Test', $1, 'public')")
        .bind(cmdr)
        .execute(&pool)
        .await
        .unwrap();
    let listed = |b: &Value| b["total"].as_i64().unwrap();

    assert_eq!(listed(&call(&app, Method::GET, "/api/carriers", None, None).await.1), 1);

    let (status, _) = call(&app, Method::POST, &format!("/api/admin/users/{cmdr}/disable"), Some(&boss), Some(json!({ "reason": "spam" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&cmdr_session), None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(listed(&call(&app, Method::GET, "/api/carriers", None, None).await.1), 0);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", None, None).await.0, StatusCode::NOT_FOUND);
    // Admins still see it, to moderate it.
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", Some(&boss), None).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::POST, &format!("/api/admin/users/{cmdr}/disable"), Some(&boss), Some(json!({}))).await.0, StatusCode::CONFLICT);

    let (_, detail) = call(&app, Method::GET, &format!("/api/admin/users/{cmdr}"), Some(&boss), None).await;
    assert_eq!(detail["disabled_reason"], "spam");

    assert_eq!(call(&app, Method::POST, &format!("/api/admin/users/{cmdr}/enable"), Some(&boss), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(listed(&call(&app, Method::GET, "/api/carriers", None, None).await.1), 1);
    assert_eq!(call(&app, Method::POST, &format!("/api/admin/users/{cmdr}/enable"), Some(&boss), None).await.0, StatusCode::CONFLICT);
    assert_eq!(audit(&pool, cmdr).await, ["user.disable", "user.enable"]);
}

#[sqlx::test]
async fn admins_cannot_act_on_themselves(pool: PgPool) {
    let app = app(&pool);
    let (boss, boss_session) = user(&pool, "boss", "admin").await;
    user(&pool, "deputy", "admin").await;
    let me = format!("/api/admin/users/{boss}");

    assert_eq!(call(&app, Method::POST, &format!("{me}/disable"), Some(&boss_session), Some(json!({}))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::PATCH, &me, Some(&boss_session), Some(json!({ "role": "user" }))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::DELETE, &me, Some(&boss_session), None).await.0, StatusCode::BAD_REQUEST);
    assert!(audit(&pool, boss).await.is_empty());
}

#[sqlx::test]
async fn role_changes_take_effect_and_are_logged(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr", "user").await;
    let path = format!("/api/admin/users/{cmdr}");

    assert_eq!(call(&app, Method::PATCH, &path, Some(&boss), Some(json!({ "role": "admin" }))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, "/api/admin/users", Some(&cmdr_session), None).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::PATCH, &path, Some(&boss), Some(json!({ "role": "user" }))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, "/api/admin/users", Some(&cmdr_session), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::PATCH, &path, Some(&boss), Some(json!({ "role": "root" }))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(audit(&pool, cmdr).await, ["user.set_role", "user.set_role"]);
}

/// Two admins demoting each other at the same moment: the admin rows are
/// locked, so one goes first and the other then sees it would remove the last
/// admin. Whatever the interleaving, an active admin remains.
#[sqlx::test]
async fn an_admin_always_remains(pool: PgPool) {
    let app = app(&pool);
    for round in 0..8 {
        sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();
        let (a, a_session) = user(&pool, &format!("a{round}"), "admin").await;
        let (b, b_session) = user(&pool, &format!("b{round}"), "admin").await;
        let demote = |target: i64, session: String| {
            let app = app.clone();
            async move {
                call(&app, Method::PATCH, &format!("/api/admin/users/{target}"), Some(&session), Some(json!({ "role": "user" }))).await.0
            }
        };
        let (ra, rb) = tokio::join!(demote(b, a_session), demote(a, b_session));
        let admins: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE role = 'admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(admins, 1, "round {round}: {ra} / {rb}");
        assert!(ra == StatusCode::CONFLICT || rb == StatusCode::CONFLICT || ra == StatusCode::FORBIDDEN || rb == StatusCode::FORBIDDEN);
    }
}

#[sqlx::test]
async fn delete_releases_carriers_and_keeps_the_audit_trail(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr", "user").await;
    sqlx::query("INSERT INTO carriers (carrier_id, callsign, owner_id) VALUES (7, 'XYZ-789', $1)")
        .bind(cmdr)
        .execute(&pool)
        .await
        .unwrap();

    let (status, body) = call(&app, Method::DELETE, &format!("/api/admin/users/{cmdr}"), Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["released_carriers"], json!(["XYZ-789"]));
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&cmdr_session), None).await.0, StatusCode::UNAUTHORIZED);
    let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM carriers WHERE carrier_id = 7")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, None);
    let logged: Value = sqlx::query_scalar("SELECT detail FROM admin_actions WHERE target_id = $1 AND action = 'user.delete'")
        .bind(cmdr)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(logged["username"], "cmdr");
    assert_eq!(call(&app, Method::DELETE, &format!("/api/admin/users/{cmdr}"), Some(&boss), None).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn sessions_and_tokens_are_revoked_per_user(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr", "user").await;
    let (other, other_session) = user(&pool, "other", "user").await;
    let (_, created) = call(&app, Method::POST, "/api/auth/tokens", Some(&other_session), Some(json!({ "name": "theirs" }))).await;
    let their_token = created["meta"]["id"].as_i64().unwrap();

    // Another user's token id under this user's path: not found, untouched.
    assert_eq!(call(&app, Method::DELETE, &format!("/api/admin/users/{cmdr}/tokens/{their_token}"), Some(&boss), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::DELETE, &format!("/api/admin/users/{other}/tokens/{their_token}"), Some(&boss), None).await.0, StatusCode::NO_CONTENT);

    let (status, body) = call(&app, Method::DELETE, &format!("/api/admin/users/{cmdr}/sessions"), Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revoked"], 1);
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&cmdr_session), None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&other_session), None).await.0, StatusCode::OK);
}
