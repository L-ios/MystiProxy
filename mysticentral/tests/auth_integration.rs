//! Integration tests for auth & user management (requires live PostgreSQL).
//!
//! Run with: DATABASE_URL=postgres://... cargo test -p mysticentral --test auth_integration

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    routing::{get, put},
    Router,
};
use mysticentral::handlers::login as mysticentral_login_shim;
use mysticentral::handlers::{create_protected_routes, AppState};
use mysticentral::middleware::auth::{auth_middleware, AuthMiddlewareState};
use mysticentral::services::{ensure_admin_user, AuthService};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

// Tests share the database; serialize them with a file lock to avoid
// bootstrap/unique-key races between concurrent tokio tests.
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct TestGuard(std::sync::MutexGuard<'static, ()>);
fn test_lock() -> TestGuard {
    // Leak-free: guard releases on drop
    TestGuard(TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner()))
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
            axum::routing::post(mysticentral_login_shim),
        )
        .merge(
            create_protected_routes()
                .layer(from_fn_with_state(mw.clone(), auth_middleware))
                .layer(axum::Extension(mw)),
        )
        .route("/test/whoami", get(whoami))
        .route("/test/changepw", put(change_pw))
        .with_state(state)
}

async fn whoami(
    axum::Extension(ctx): axum::Extension<mysticentral::middleware::auth::AuthContext>,
) -> axum::Json<Value> {
    axum::Json(json!({"username": ctx.user.username}))
}

async fn change_pw(
    axum::Extension(ctx): axum::Extension<mysticentral::middleware::auth::AuthContext>,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(body): axum::Json<Value>,
) -> StatusCode {
    let old = body["old_password"].as_str().unwrap_or_default();
    let new = body["new_password"].as_str().unwrap_or_default();
    if !mysticentral::services::AuthService::verify_password(old, &ctx.user.password_hash) {
        return StatusCode::BAD_REQUEST;
    }
    let repo = mysticentral::services::PostgresUserRepository::new(state.pool.clone());
    use mysticentral::services::UserRepository;
    let hash = mysticentral::services::AuthService::hash_password(new).unwrap();
    repo.update_password(ctx.user.id, &hash).await.unwrap();
    StatusCode::NO_CONTENT
}

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM users")
        .execute(pool)
        .await
        .unwrap();
}

async fn login_get_token(app: &Router, user: &str, pass: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": user, "password": pass}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn full_auth_flow() {
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;

    // Bootstrap creates initial admin
    let admin = ensure_admin_user(&pool).await.unwrap();
    assert!(admin.is_some());
    assert_eq!(admin.unwrap().username, "admin");
    // Idempotent: second call skips
    assert!(ensure_admin_user(&pool).await.unwrap().is_none());

    let app = test_app(pool.clone());

    // 1. Correct credentials -> 200 with token/user/expires_at
    let (status, body) = login_get_token(&app, "admin", "changeme123").await;
    assert_eq!(status, StatusCode::OK, "login should succeed: {body}");
    assert!(body["token"].as_str().is_some_and(|t| !t.is_empty()));
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["role"], "admin");
    assert!(body["expires_at"].as_str().is_some());
    let token = body["token"].as_str().unwrap().to_string();

    // 2. Wrong password -> 401
    let (status, _) = login_get_token(&app, "admin", "wrong-pass").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 3. Unknown user -> 401 with identical message (anti-enumeration)
    let (_, body2) = login_get_token(&app, "ghost", "whatever1").await;
    assert_eq!(body2["message"], "Invalid username or password");

    // 4. No token -> 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 5. Garbage token -> 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .header("authorization", "Bearer garbage.token.here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 6. Admin token -> 200 user list
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 7. Admin creates viewer user
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"viewer1","email":"v1@t.local","password":"viewerpass1","role":"viewer"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 8. New viewer can log in
    let (status, vbody) = login_get_token(&app, "viewer1", "viewerpass1").await;
    assert_eq!(status, StatusCode::OK);
    let vtoken = vbody["token"].as_str().unwrap().to_string();

    // 9. Viewer cannot list users -> 403
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .header("authorization", format!("Bearer {vtoken}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 10. Viewer can read own profile via /users/me
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/me")
                .header("authorization", format!("Bearer {vtoken}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let me: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["username"], "viewer1");

    // 11. Change password; old password stops working
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/users/me/password")
                .header("authorization", format!("Bearer {vtoken}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"old_password":"viewerpass1","new_password":"newviewerpw1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let (status, _) = login_get_token(&app, "viewer1", "viewerpass1").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "old password must fail");
    let (status, _) = login_get_token(&app, "viewer1", "newviewerpw1").await;
    assert_eq!(status, StatusCode::OK, "new password must work");

    // 12. Admin cannot delete self
    let admin_id = {
        use mysticentral::services::UserRepository;
        let repo = mysticentral::services::PostgresUserRepository::new(pool.clone());
        repo.find_by_username("admin").await.unwrap().unwrap().id
    };
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 13. Admin cannot change own role
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"role":"viewer"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    cleanup(&pool).await;
}

#[tokio::test]
async fn duplicate_username_conflict() {
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();

    let auth_service = AuthService::new("test-secret-that-is-long-enough-32ch".into(), 1).unwrap();
    let state = AppState::new(pool.clone(), auth_service.clone());
    let mw = AuthMiddlewareState {
        auth_service,
        pool: pool.clone(),
    };
    let app = Router::new()
        .route(
            "/api/v1/auth/login",
            axum::routing::post(mysticentral_login_shim),
        )
        .merge(
            create_protected_routes()
                .layer(from_fn_with_state(mw.clone(), auth_middleware))
                .layer(axum::Extension(mw)),
        )
        .with_state(state);

    let (_, body) = login_get_token(&app, "admin", "changeme123").await;
    let token = body["token"].as_str().unwrap().to_string();

    // Create once -> 201
    let mk = |app: &Router| {
        app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"dup","email":"dup@t.local","password":"password123","role":"viewer"})
                        .to_string(),
                ))
                .unwrap(),
        )
    };
    let resp = mk(&app).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    // Duplicate -> 409
    let resp = mk(&app).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    cleanup(&pool).await;
}

#[tokio::test]
async fn short_password_rejected() {
    let _g = test_lock();
    let pool = test_pool().await;
    cleanup(&pool).await;
    ensure_admin_user(&pool).await.unwrap();

    let auth_service = AuthService::new("test-secret-that-is-long-enough-32ch".into(), 1).unwrap();
    let state = AppState::new(pool.clone(), auth_service.clone());
    let mw = AuthMiddlewareState {
        auth_service,
        pool: pool.clone(),
    };
    let app = Router::new()
        .route(
            "/api/v1/auth/login",
            axum::routing::post(mysticentral_login_shim),
        )
        .merge(
            create_protected_routes()
                .layer(from_fn_with_state(mw.clone(), auth_middleware))
                .layer(axum::Extension(mw)),
        )
        .with_state(state);

    let (_, body) = login_get_token(&app, "admin", "changeme123").await;
    let token = body["token"].as_str().unwrap().to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"short","email":"s@t.local","password":"123","role":"viewer"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    cleanup(&pool).await;
}
