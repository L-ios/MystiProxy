//! Integration tests for the conflicts API (requires live PostgreSQL).
//!
//! Run with: TEST_DATABASE_URL=postgres://... cargo test -p mysticentral --test conflicts_integration

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    Router,
};
use mysticentral::handlers::{create_protected_routes, AppState};
use mysticentral::middleware::auth::{auth_middleware, AuthMiddlewareState};
use mysticentral::services::{ensure_admin_user, AuthService};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

// Serialize DB-backed tests against the shared database.
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new({});
fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}
fn db_url_missing() -> bool {
    std::env::var("TEST_DATABASE_URL").is_err() && std::env::var("DATABASE_URL").is_err()
}

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL or DATABASE_URL must point to a test PostgreSQL");
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
        .route(
            "/api/v1/auth/login",
            axum::routing::post(mysticentral::handlers::login),
        )
        .route("/api/v1/sync/push", axum::routing::post(push_shim))
        .route("/api/v1/mocks/:id", axum::routing::get(get_mock_shim))
        .merge(
            create_protected_routes()
                .layer(from_fn_with_state(mw.clone(), auth_middleware))
                .layer(axum::Extension(mw)),
        )
        .with_state(state)
}

/// Minimal push shim that mirrors sync_push's conflict branch using the real repo.
async fn push_shim(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(config): axum::Json<Value>,
) -> (StatusCode, axum::Json<Value>) {
    use mysticentral::services::{
        ConflictRecord, ConflictRepository, MockService, PostgresConflictRepository,
        PostgresMockRepository,
    };

    let parsed: Result<mysti_common::MockConfiguration, _> = serde_json::from_value(config);
    let config = match parsed {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"error": e.to_string()})),
            )
        }
    };

    let repo = PostgresMockRepository::new(state.pool.clone());
    let service = MockService::new(repo);
    let conflict_repo = PostgresConflictRepository::new(state.pool.clone());

    match service.get(config.id).await {
        Ok(existing)
            if existing
                .version_vector
                .is_concurrent_with(&config.version_vector) =>
        {
            conflict_repo
                .upsert(ConflictRecord {
                    config_id: config.id,
                    local_version: config.clone(),
                    central_version: existing.clone(),
                    detected_at: chrono::Utc::now(),
                })
                .await
                .unwrap();
            (
                StatusCode::OK,
                axum::Json(json!({"accepted": [], "conflicts": [{"id": config.id}]})),
            )
        }
        Ok(_) => {
            service.save(&config).await.unwrap();
            (
                StatusCode::OK,
                axum::Json(json!({"accepted": [config.id], "conflicts": []})),
            )
        }
        Err(_) => {
            service.save(&config).await.unwrap();
            (
                StatusCode::OK,
                axum::Json(json!({"accepted": [config.id], "conflicts": []})),
            )
        }
    }
}

async fn get_mock_shim(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> (StatusCode, axum::Json<Value>) {
    use mysticentral::services::{MockRepository, MockService, PostgresMockRepository};
    let service = MockService::new(PostgresMockRepository::new(state.pool.clone()));
    match service.get(id).await {
        Ok(c) => (
            StatusCode::OK,
            axum::Json(serde_json::to_value(&c).unwrap_or_default()),
        ),
        Err(_) => (StatusCode::NOT_FOUND, axum::Json(json!({}))),
    }
}

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM sync_conflicts")
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["token"]
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

fn base_config(name: &str) -> Value {
    json!({
        "name": name,
        "path": "/conflict-test",
        "method": "GET",
        "matching_rules": {"path_pattern_type": "exact"},
        "response_config": {"status": 200},
        "source": "central",
        "is_active": true,
        "content_hash": "x".repeat(64),
    })
}

#[tokio::test]
async fn conflicts_full_lifecycle() {
    if db_url_missing() {
        eprintln!("SKIP: no TEST_DATABASE_URL");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    // Seed a central config via push (non-conflicting)
    let mut central = base_config("central-v1");
    let config_id = uuid::Uuid::new_v4();
    central["id"] = json!(config_id);
    central["version_vector"] = json!({ format!("{}", config_id): 1 });
    central["created_at"] = json!("2026-08-15T00:00:00Z");
    central["updated_at"] = json!("2026-08-15T00:00:00Z");
    let (status, body) = req(
        &app,
        "POST",
        "/api/v1/sync/push",
        &token,
        Some(central.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "seed push: {body}");
    assert!(body["accepted"].as_array().unwrap().len() == 1);

    // 1. Push a CONCURRENT local version (different instance id) -> conflict recorded
    let mut local = central.clone();
    local["name"] = json!("local-v2");
    let other_instance = uuid::Uuid::new_v4();
    local["version_vector"] = json!({ format!("{}", other_instance): 5 });
    let (status, body) = req(
        &app,
        "POST",
        "/api/v1/sync/push",
        &token,
        Some(local.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["conflicts"].as_array().unwrap().len(), 1);

    // GET /conflicts shows exactly one record with both versions
    let (status, list) = req(&app, "GET", "/api/v1/conflicts", &token, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 1, "list: {list}");
    let entry = &list["data"][0];
    assert_eq!(entry["config_id"], config_id.to_string());
    assert_eq!(entry["local_version"]["name"], "local-v2");
    assert_eq!(entry["central_version"]["name"], "central-v1");
    assert!(entry["detected_at"].as_str().is_some());

    // 2. Repeat conflicting push -> still one record (upsert)
    let (status, _) = req(
        &app,
        "POST",
        "/api/v1/sync/push",
        &token,
        Some(local.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, list2) = req(&app, "GET", "/api/v1/conflicts", &token, None).await;
    assert_eq!(list2["total"], 1, "upsert must not duplicate");

    // 3. GET single conflict
    let (status, one) = req(
        &app,
        "GET",
        &format!("/api/v1/conflicts/{config_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["local_version"]["name"], "local-v2");

    // 4. resolve keep_local -> config becomes local, conflict cleared
    let (status, resolved) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{config_id}/resolve"),
        &token,
        Some(json!({"resolution": "keep_local"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resolve: {resolved}");
    assert_eq!(resolved["name"], "local-v2");

    let (_, mock) = req(
        &app,
        "GET",
        &format!("/api/v1/mocks/{config_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(
        mock["name"], "local-v2",
        "central config must take local version"
    );

    let (_, list3) = req(&app, "GET", "/api/v1/conflicts", &token, None).await;
    assert_eq!(list3["total"], 0, "conflict must be cleared after resolve");

    cleanup(&pool).await;
}

#[tokio::test]
async fn conflicts_dismiss_and_merge_errors() {
    if db_url_missing() {
        eprintln!("SKIP: no TEST_DATABASE_URL");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    // Seed central
    let mut central = base_config("c1");
    let config_id = uuid::Uuid::new_v4();
    central["id"] = json!(config_id);
    central["version_vector"] = json!({ format!("{}", config_id): 1 });
    central["created_at"] = json!("2026-08-15T00:00:00Z");
    central["updated_at"] = json!("2026-08-15T00:00:00Z");
    req(
        &app,
        "POST",
        "/api/v1/sync/push",
        &token,
        Some(central.clone()),
    )
    .await;

    // Create conflict
    let mut local = central.clone();
    local["name"] = json!("l1");
    local["version_vector"] = json!({ format!("{}", uuid::Uuid::new_v4()): 2 });
    req(&app, "POST", "/api/v1/sync/push", &token, Some(local)).await;

    // merge without merged_config -> 400
    let (status, body) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{config_id}/resolve"),
        &token,
        Some(json!({"resolution": "merge"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // invalid resolution -> 400
    let (status, _) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{config_id}/resolve"),
        &token,
        Some(json!({"resolution": "bogus"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // unknown conflict -> 404
    let (status, _) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{}/resolve", uuid::Uuid::new_v4()),
        &token,
        Some(json!({"resolution": "keep_central"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // DELETE dismiss -> 204, config stays central, list empty
    let (status, _) = req(
        &app,
        "DELETE",
        &format!("/api/v1/conflicts/{config_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, mock) = req(
        &app,
        "GET",
        &format!("/api/v1/mocks/{config_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(mock["name"], "c1", "dismiss keeps central version");

    let (_, list) = req(&app, "GET", "/api/v1/conflicts", &token, None).await;
    assert_eq!(list["total"], 0);

    // DELETE again -> 404
    let (status, _) = req(
        &app,
        "DELETE",
        &format!("/api/v1/conflicts/{config_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&pool).await;
}

#[tokio::test]
async fn merge_resolution_merges_vectors() {
    if db_url_missing() {
        eprintln!("SKIP: no TEST_DATABASE_URL");
        return;
    }
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();
    let app = test_app(pool.clone());
    let token = login_token(&app).await;

    let mut central = base_config("central");
    let config_id = uuid::Uuid::new_v4();
    central["id"] = json!(config_id);
    central["version_vector"] = json!({ format!("{}", config_id): 1 });
    central["created_at"] = json!("2026-08-15T00:00:00Z");
    central["updated_at"] = json!("2026-08-15T00:00:00Z");
    req(&app, "POST", "/api/v1/sync/push", &token, Some(central)).await;

    let mut local = base_config("local");
    local["id"] = json!(config_id);
    let local_instance = uuid::Uuid::new_v4();
    local["version_vector"] = json!({ format!("{}", local_instance): 3 });
    local["created_at"] = json!("2026-08-15T00:00:00Z");
    local["updated_at"] = json!("2026-08-15T00:00:00Z");
    req(&app, "POST", "/api/v1/sync/push", &token, Some(local)).await;

    // merge with explicit merged config
    let mut merged = base_config("merged-name");
    merged["id"] = json!(config_id);
    merged["created_at"] = json!("2026-08-15T00:00:00Z");
    merged["updated_at"] = json!("2026-08-15T00:00:00Z");
    let (status, body) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{config_id}/resolve"),
        &token,
        Some(json!({"resolution": "merge", "merged_config": merged})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "merged-name");

    // Vector must contain BOTH sides
    let (_, mock) = req(
        &app,
        "GET",
        &format!("/api/v1/mocks/{config_id}"),
        &token,
        None,
    )
    .await;
    let vv = &mock["version_vector"];
    assert!(
        vv.get(config_id.to_string()).is_some(),
        "central side present: {vv}"
    );
    assert!(
        vv.get(local_instance.to_string()).is_some(),
        "local side present: {vv}"
    );

    // merged_config with wrong id -> 400
    // (new conflict first)
    let mut again = base_config("again");
    again["id"] = json!(config_id);
    again["version_vector"] = json!({ format!("{}", uuid::Uuid::new_v4()): 9 });
    again["created_at"] = json!("2026-08-15T00:00:00Z");
    again["updated_at"] = json!("2026-08-15T00:00:00Z");
    req(&app, "POST", "/api/v1/sync/push", &token, Some(again)).await;

    let mut wrong = base_config("wrong");
    wrong["id"] = json!(uuid::Uuid::new_v4());
    wrong["created_at"] = json!("2026-08-15T00:00:00Z");
    wrong["updated_at"] = json!("2026-08-15T00:00:00Z");
    let (status, _) = req(
        &app,
        "PUT",
        &format!("/api/v1/conflicts/{config_id}/resolve"),
        &token,
        Some(json!({"resolution": "merge", "merged_config": wrong})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&pool).await;
}
