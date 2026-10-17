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

#[sqlx::test]
async fn registration_switch_overrides_the_environment(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (_, plain) = user(&pool, "plain", "user").await;
    let register = |name: &str| json!({ "username": name, "password": "correct horse battery" });

    assert_eq!(call(&app, Method::GET, "/api/admin/settings", Some(&plain), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::PUT, "/api/admin/settings", Some(&plain), Some(json!({ "registration": "closed" }))).await.0, StatusCode::FORBIDDEN);

    // The environment (open, in `app`) applies until something is stored.
    let (_, s) = call(&app, Method::GET, "/api/admin/settings", Some(&boss), None).await;
    assert_eq!(s["registration"], json!({ "open": true, "setting": null, "env": true }));

    let (status, s) = call(&app, Method::PUT, "/api/admin/settings", Some(&boss), Some(json!({ "registration": "closed" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["registration"]["open"], false);
    assert_eq!(call(&app, Method::POST, "/api/auth/register", None, Some(register("late"))).await.0, StatusCode::FORBIDDEN);

    // Same value again: no change, no log entry.
    call(&app, Method::PUT, "/api/admin/settings", Some(&boss), Some(json!({ "registration": "closed" }))).await;
    call(&app, Method::PUT, "/api/admin/settings", Some(&boss), Some(json!({ "registration": "default" }))).await;
    assert_eq!(call(&app, Method::POST, "/api/auth/register", None, Some(register("early"))).await.0, StatusCode::CREATED);

    assert_eq!(call(&app, Method::PUT, "/api/admin/settings", Some(&boss), Some(json!({ "registration": "maybe" }))).await.0, StatusCode::UNPROCESSABLE_ENTITY);

    let log: Vec<Value> = sqlx::query_scalar("SELECT detail FROM admin_actions WHERE action = 'settings.registration' ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(log, vec![
        json!({ "from": null, "to": false, "env": true }),
        json!({ "from": false, "to": null, "env": true }),
    ]);
}

#[sqlx::test]
async fn audit_log_pages_newest_first_and_filters_by_account(pool: PgPool) {
    let app = app(&pool);
    let (boss_id, boss) = user(&pool, "boss", "admin").await;
    let (a, _) = user(&pool, "alpha", "user").await;
    let (b, _) = user(&pool, "bravo", "user").await;

    call(&app, Method::POST, &format!("/api/admin/users/{a}/disable"), Some(&boss), Some(json!({ "reason": "spam" }))).await;
    call(&app, Method::POST, &format!("/api/admin/users/{a}/enable"), Some(&boss), None).await;
    call(&app, Method::DELETE, &format!("/api/admin/users/{b}"), Some(&boss), None).await;

    assert_eq!(call(&app, Method::GET, "/api/admin/audit", None, None).await.0, StatusCode::UNAUTHORIZED);

    let (status, page) = call(&app, Method::GET, "/api/admin/audit?per_page=2", Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 3);
    assert_eq!(page["total_pages"], 2);
    let first = &page["items"][0];
    assert_eq!(first["action"], "user.delete");
    // Deleted, so the name comes from the entry itself.
    assert_eq!(first["target"], "bravo");
    assert_eq!(first["admin"], "boss");
    assert_eq!(first["admin_id"], boss_id);
    assert_eq!(page["items"][1]["action"], "user.enable");

    let (_, page) = call(&app, Method::GET, "/api/admin/audit?page=2&per_page=2", Some(&boss), None).await;
    assert_eq!(page["items"][0]["detail"]["reason"], "spam");

    let (_, page) = call(&app, Method::GET, &format!("/api/admin/audit?user={a}"), Some(&boss), None).await;
    assert_eq!(page["total"], 2);
    assert!(page["items"].as_array().unwrap().iter().all(|i| i["target_id"] == a));
}

async fn carrier(pool: &PgPool, id: i64, callsign: &str, owner: Option<i64>, visibility: &str) {
    sqlx::query("INSERT INTO carriers (carrier_id, callsign, name, owner_id, visibility) VALUES ($1, $2, 'Test', $3, $4)")
        .bind(id)
        .bind(callsign)
        .bind(owner)
        .bind(visibility)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn hidden_carriers_are_seen_only_by_owner_and_admins(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (cmdr, cmdr_session) = user(&pool, "cmdr", "user").await;
    let (_, other) = user(&pool, "other", "user").await;
    carrier(&pool, 1, "ABC-123", Some(cmdr), "public").await;
    carrier(&pool, 2, "SEC-RET", Some(cmdr), "owner_only").await;

    assert_eq!(call(&app, Method::GET, "/api/admin/carriers", Some(&other), None).await.0, StatusCode::FORBIDDEN);
    // Every carrier, owner_only too, with its owner.
    let (_, list) = call(&app, Method::GET, "/api/admin/carriers", Some(&boss), None).await;
    assert_eq!(list["total"], 2);
    let (_, list) = call(&app, Method::GET, "/api/admin/carriers?q=cmdr", Some(&boss), None).await;
    assert_eq!(list["total"], 2);
    assert_eq!(list["items"][0]["owner"], "cmdr");

    let (status, _) = call(&app, Method::POST, "/api/admin/carriers/1/hide", Some(&boss), Some(json!({ "reason": "offensive name" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::POST, "/api/admin/carriers/1/hide", Some(&boss), Some(json!({}))).await.0, StatusCode::CONFLICT);

    let (_, public) = call(&app, Method::GET, "/api/carriers", None, None).await;
    assert_eq!(public["total"], 0);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", Some(&other), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123/events", None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", Some(&boss), None).await.0, StatusCode::OK);
    // The owner still sees it, and why.
    let (status, snapshot) = call(&app, Method::GET, "/api/carriers/ABC-123", Some(&cmdr_session), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["moderation_reason"], "offensive name");
    let (_, mine) = call(&app, Method::GET, "/api/carriers/mine", Some(&cmdr_session), None).await;
    let hidden = mine.as_array().unwrap().iter().find(|c| c["callsign"] == "ABC-123").unwrap();
    assert!(hidden["moderation_hidden_at"].is_string());
    // Owner settings can't undo it.
    call(&app, Method::PATCH, "/api/carriers/ABC-123", Some(&cmdr_session), Some(json!({ "visibility": "public" }))).await;
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", None, None).await.0, StatusCode::NOT_FOUND);

    let (_, list) = call(&app, Method::GET, "/api/admin/carriers?filter=hidden", Some(&boss), None).await;
    assert_eq!(list["total"], 1);

    assert_eq!(call(&app, Method::POST, "/api/admin/carriers/1/unhide", Some(&boss), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::POST, "/api/admin/carriers/1/unhide", Some(&boss), None).await.0, StatusCode::CONFLICT);
    assert_eq!(call(&app, Method::GET, "/api/carriers/ABC-123", None, None).await.0, StatusCode::OK);

    // Admins get no way around owner_only: that is the owner's privacy.
    assert_eq!(call(&app, Method::GET, "/api/carriers/SEC-RET", Some(&boss), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, Method::POST, "/api/admin/carriers/99/hide", Some(&boss), Some(json!({}))).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn reassign_release_and_delete_carriers(pool: PgPool) {
    let app = app(&pool);
    let (_, boss) = user(&pool, "boss", "admin").await;
    let (impostor, impostor_session) = user(&pool, "impostor", "user").await;
    let (real, real_session) = user(&pool, "real_owner", "user").await;
    carrier(&pool, 1, "ABC-123", Some(impostor), "public").await;
    sqlx::query("INSERT INTO carrier_events (timestamp, event_name, json_data, carrier) VALUES ('2026-01-01T00:00:00Z', 'CarrierStats', '{}', 1)")
        .execute(&pool)
        .await
        .unwrap();

    let owner = |name: Option<&str>| Some(json!({ "username": name }));
    assert_eq!(call(&app, Method::PUT, "/api/admin/carriers/1/owner", Some(&boss), owner(Some("nobody"))).await.0, StatusCode::NOT_FOUND);
    // Case-insensitive, like logging in.
    assert_eq!(call(&app, Method::PUT, "/api/admin/carriers/1/owner", Some(&boss), owner(Some("REAL_OWNER"))).await.0, StatusCode::NO_CONTENT);
    let (_, detail) = call(&app, Method::GET, "/api/admin/carriers/1", Some(&boss), None).await;
    assert_eq!(detail["owner_id"], real);
    assert_eq!(detail["events"], 1);
    // The settings now answer to the new owner only.
    assert_eq!(call(&app, Method::PATCH, "/api/carriers/ABC-123", Some(&impostor_session), Some(json!({ "visibility": "private" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::PATCH, "/api/carriers/ABC-123", Some(&real_session), Some(json!({ "visibility": "private" }))).await.0, StatusCode::NO_CONTENT);

    // Same owner again: nothing to log.
    call(&app, Method::PUT, "/api/admin/carriers/1/owner", Some(&boss), owner(Some("real_owner"))).await;
    assert_eq!(call(&app, Method::PUT, "/api/admin/carriers/1/owner", Some(&boss), owner(None)).await.0, StatusCode::NO_CONTENT);
    let (_, detail) = call(&app, Method::GET, "/api/admin/carriers/1", Some(&boss), None).await;
    assert!(detail["owner_id"].is_null());

    let (status, body) = call(&app, Method::DELETE, "/api/admin/carriers/1", Some(&boss), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["events"], 1);
    let left: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM carriers), (SELECT count(*) FROM carrier_events)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, (0, 0));

    // The carrier's history, under its callsign though it is gone.
    let (_, log) = call(&app, Method::GET, "/api/admin/audit?carrier=1", Some(&boss), None).await;
    let actions: Vec<&str> = log["items"].as_array().unwrap().iter().map(|i| i["action"].as_str().unwrap()).collect();
    assert_eq!(actions, ["carrier.delete", "carrier.release", "carrier.reassign"]);
    assert_eq!(log["items"][0]["target"], "ABC-123");
    assert_eq!(log["items"][0]["target_kind"], "carrier");
    assert_eq!(log["items"][2]["detail"]["owner"], "impostor");
    assert_eq!(log["items"][2]["detail"]["to_owner"], "real_owner");
    // Each owner's history includes what happened to their carrier.
    let (_, log) = call(&app, Method::GET, &format!("/api/admin/audit?user={impostor}"), Some(&boss), None).await;
    assert_eq!(log["total"], 1);
    let (_, log) = call(&app, Method::GET, &format!("/api/admin/audit?user={real}"), Some(&boss), None).await;
    // Given, then released: the delete came after, while unowned.
    assert_eq!(log["total"], 2);
}
