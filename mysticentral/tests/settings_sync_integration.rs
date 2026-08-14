//! Integration tests for settings / sync-status / instance push (live PostgreSQL).

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};
use mysticentral::handlers::{create_protected_routes, AppState};
use mysticentral::middleware::auth::{auth_middleware, AuthMiddlewareState};
use mysticentral::services::{ensure_admin_user, AuthService};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn db_url_missing() -> bool {
    std::env::var("TEST_DATABASE_URL").is_err() && std::env::var("DATABASE_URL").is_err()
}

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL or DATABASE_URL required");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

fn test_app(pool: sqlx::PgPool) -> Router {
    let auth_service =
        AuthService::new("test-secret-that-is-long-enough-32ch".to_string(), 1).unwrap();
    let state = AppState::new(pool.clone(), auth_service.clone());
    let mw = AuthMiddlewareState {
        auth_service,
        pool: pool.clone(),
    };
    Router::new()
        .route("/api/v1/auth/login", post(mysticentral::handlers::login))
        .merge(
            create_protected_routes()
                .layer(from_fn_with_state(mw.clone(), auth_middleware))
                .layer(axum::Extension(mw)),
        )
        .with_state(state)
}

/// Stand up a fake instance (local-management API) that accepts /api/v1/sync/trigger.
async fn start_fake_instance() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route("/api/v1/sync/trigger", post(|| async { StatusCode::OK }));
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), handle)
}

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM sync_conflicts")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM mystiproxy_instances")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM mock_configurations")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users")
        .execute(pool)
        .await
        .unwrap();
    // Reset the single settings row to migration defaults
    sqlx::query(
        "UPDATE system_settings SET central_url = '', sync_interval_secs = 30, log_level = 'info', max_request_history = 1000, default_environment = NULL",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn login_token(app: &Router) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"changeme123"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let b = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice::<Value>(&b).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn req(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn register_instance(pool: &sqlx::PgPool, endpoint: &str) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mystiproxy_instances (id, name, endpoint_url, sync_status, last_heartbeat) VALUES ($1, $2, $3, 'connected', NOW())",
    )
    .bind(id)
    .bind(format!("inst-{endpoint}"))
    .bind(endpoint)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn settings_crud_and_validation() {
    if db_url_missing() {
        eprintln!("SKIP");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    // Defaults
    let (status, s) = req(&app, "GET", "/api/v1/settings", &token, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["sync_interval"], 30);
    assert_eq!(s["log_level"], "info");
    assert_eq!(s["max_request_history"], 1000);
    assert_eq!(s["central_url"], "");

    // Full update
    let (status, s) = req(
        &app,
        "PUT",
        "/api/v1/settings",
        &token,
        Some(json!({
            "central_url": "https://central.example.com",
            "sync_interval": 120,
            "log_level": "warn",
            "max_request_history": 250,
            "default_environment": "staging"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["sync_interval"], 120);
    assert_eq!(s["log_level"], "warn");

    // Partial update keeps others
    let (_, s) = req(
        &app,
        "PUT",
        "/api/v1/settings",
        &token,
        Some(json!({"log_level": "debug"})),
    )
    .await;
    assert_eq!(s["log_level"], "debug");
    assert_eq!(s["sync_interval"], 120, "untouched field preserved");

    // Validation failures
    let (st, _) = req(
        &app,
        "PUT",
        "/api/v1/settings",
        &token,
        Some(json!({"log_level": "loud"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = req(
        &app,
        "PUT",
        "/api/v1/settings",
        &token,
        Some(json!({"sync_interval": 2})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = req(
        &app,
        "PUT",
        "/api/v1/settings",
        &token,
        Some(json!({"central_url": "ftp://x"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // No token -> 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    cleanup(&pool).await;
}

#[tokio::test]
async fn sync_status_reflects_instances_and_conflicts() {
    if db_url_missing() {
        eprintln!("SKIP");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    // Empty: not connected
    let (status, s) = req(&app, "GET", "/api/v1/sync/status", &token, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["connected"], false);
    assert_eq!(s["pending_changes"], 0);
    assert_eq!(s["sync_in_progress"], false);

    // Register a fresh-heartbeat instance
    register_instance(&pool, "http://127.0.0.1:9").await;
    let (_, s) = req(&app, "GET", "/api/v1/sync/status", &token, None).await;
    assert_eq!(s["connected"], true, "fresh heartbeat counts as connected");

    // Two unresolved conflicts -> pending_changes = 2
    for i in 0..2 {
        let cid = uuid::Uuid::new_v4();
        let cfg = mysti_common::MockConfiguration::new(
            format!("c{i}"),
            format!("/c{i}"),
            mysti_common::HttpMethod::Get,
            mysti_common::MatchingRules::default(),
            mysti_common::ResponseConfig::default(),
        );
        sqlx::query("INSERT INTO sync_conflicts (config_id, local_version, central_version) VALUES ($1, $2, $3)")
            .bind(cid)
            .bind(serde_json::to_value(&cfg).unwrap())
            .bind(serde_json::to_value(&cfg).unwrap())
            .execute(&pool).await.unwrap();
    }
    let (_, s) = req(&app, "GET", "/api/v1/sync/status", &token, None).await;
    assert_eq!(s["pending_changes"], 2);

    cleanup(&pool).await;
}

#[tokio::test]
async fn push_to_live_and_dead_instances() {
    if db_url_missing() {
        eprintln!("SKIP");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    // Live fake instance
    let (endpoint, _server) = start_fake_instance().await;
    let live_id = register_instance(&pool, &endpoint).await;

    // Dead instance (reserved but unlistened port)
    let dead_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead_listener.local_addr().unwrap().port();
    drop(dead_listener);
    let dead_id = register_instance(&pool, &format!("http://127.0.0.1:{dead_port}")).await;

    // Unknown instance -> 404
    let (st, _) = req(
        &app,
        "POST",
        &format!("/api/v1/instances/{}/push", uuid::Uuid::new_v4()),
        &token,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Push to live -> 200, last_sync_at set
    let (st, body) = req(
        &app,
        "POST",
        &format!("/api/v1/instances/{live_id}/push"),
        &token,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    let last_sync: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_sync_at FROM mystiproxy_instances WHERE id = $1")
            .bind(live_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_sync.is_some(), "last_sync_at updated on success");

    // Push to dead -> 502, sync_status error
    let (st, body) = req(
        &app,
        "POST",
        &format!("/api/v1/instances/{dead_id}/push"),
        &token,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["ok"], false);
    let status: String =
        sqlx::query_scalar("SELECT sync_status FROM mystiproxy_instances WHERE id = $1")
            .bind(dead_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "error");

    // push-all aggregates both
    let (st, summary) = req(&app, "POST", "/api/v1/instances/push-all", &token, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(summary["pushed"], 1);
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["results"].as_array().unwrap().len(), 2);

    // POST /sync contract
    let (st, sync) = req(
        &app,
        "POST",
        "/api/v1/sync",
        &token,
        Some(json!({"force": true})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sync["synced_count"], 1);
    assert_eq!(sync["success"], false); // one failed
    assert!(sync["synced_at"].as_str().is_some());
    assert!(sync["conflicts"].as_array().is_some());

    cleanup(&pool).await;
}
