use std::collections::HashMap;
use std::sync::Arc;

use auth_service::store::MemoryAuthStore;
use auth_service::tokens::{Claims, KeyRing};
use auth_service::{router, AppState};
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use chrono::{Duration, Utc};
use common::clock::{Clock, FakeClock};
use common::config::ConfigLoader;
use common::defaults;
use common::settings::AuthSettings;
use http_body_util::BodyExt;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde_json::{json, Value};
use tower::ServiceExt;

struct Harness {
    state: AppState,
    clock: Arc<FakeClock>,
}

fn harness() -> Harness {
    let mut settings = AuthSettings::from_config(&ConfigLoader::new("auth", HashMap::new()));
    settings.argon2_memory_kb = 4096;
    settings.argon2_iterations = 1;
    settings.argon2_parallelism = 1;
    settings.cookie_secure = false;

    let started_at = Utc::now();
    let clock = Arc::new(FakeClock::new(started_at));
    let keys = Arc::new(KeyRing::generate(defaults::AUTH_JWT_KEY_ID).expect("key ring"));
    let state = AppState::new(
        settings,
        Arc::new(MemoryAuthStore::new()),
        keys,
        clock.clone() as Arc<dyn Clock>,
    );
    Harness { state, clock }
}

async fn call(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).expect("json body"))
        }
        None => Body::empty(),
    };
    let response = router(state.clone())
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json response")
    };
    (status, value)
}

async fn register(state: &AppState, email: &str, password: &str) -> (StatusCode, Value) {
    call(
        state,
        "POST",
        "/auth/register",
        Some(json!({"email": email, "password": password, "nickname": "Player"})),
    )
    .await
}

async fn login(state: &AppState, email: &str, password: &str) -> (StatusCode, Value) {
    call(
        state,
        "POST",
        "/auth/login",
        Some(json!({"email": email, "password": password})),
    )
    .await
}

async fn refresh(state: &AppState, token: &str) -> (StatusCode, Value) {
    call(
        state,
        "POST",
        "/auth/refresh-token",
        Some(json!({"refresh_token": token})),
    )
    .await
}

#[tokio::test]
async fn health_reports_ok() {
    let harness = harness();
    let (status, body) = call(&harness.state, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "auth-service");
}

#[tokio::test]
async fn register_then_login_returns_verifiable_access_token() {
    let harness = harness();
    let (status, user) = register(&harness.state, "player@example.com", "s3cret-pass").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(user["user"]["email"], "player@example.com");
    assert_eq!(user["user"]["nickname"], "Player");
    assert_eq!(user["user"]["roles"][0], "student");
    assert!(!user["accessToken"]
        .as_str()
        .expect("register token")
        .is_empty());

    let (status, body) = login(&harness.state, "Player@Example.com", "s3cret-pass").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tokenType"], "Bearer");
    assert_eq!(body["expiresIn"], 900);
    assert_eq!(body["exp"], harness.clock.now().timestamp() + 900);

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&["user-api"]);
    validation.set_issuer(&[defaults::AUTH_JWT_ISSUER]);
    let decoded = decode::<Claims>(
        body["accessToken"].as_str().expect("access token"),
        &DecodingKey::from_rsa_pem(harness.state.keys.public_pem().as_bytes())
            .expect("decoding key"),
        &validation,
    )
    .expect("decode");
    assert_eq!(
        decoded.claims.sub,
        user["user"]["id"].as_str().expect("user id")
    );
    assert_eq!(decoded.claims.email, "player@example.com");
    assert_eq!(decoded.claims.exp, harness.clock.now().timestamp() + 900);
}

#[tokio::test]
async fn duplicate_registration_conflicts() {
    let harness = harness();
    register(&harness.state, "dupe@example.com", "s3cret-pass").await;
    let (status, body) = register(&harness.state, "DUPE@example.com", "s3cret-pass").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "conflict");
}

#[tokio::test]
async fn weak_password_and_bad_email_are_rejected() {
    let harness = harness();
    let (status, body) = register(&harness.state, "not-an-email", "s3cret-pass").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");

    let (status, body) = register(&harness.state, "short@example.com", "short").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn wrong_password_is_unauthenticated() {
    let harness = harness();
    register(&harness.state, "player@example.com", "s3cret-pass").await;
    let (status, body) = login(&harness.state, "player@example.com", "wrong-pass").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthenticated");
}

#[tokio::test]
async fn refresh_rotates_and_reuse_after_grace_revokes_family() {
    let harness = harness();
    register(&harness.state, "player@example.com", "s3cret-pass").await;
    let (_, session) = login(&harness.state, "player@example.com", "s3cret-pass").await;
    let original = session["refreshToken"].as_str().expect("refresh token");

    harness.clock.advance(Duration::seconds(1));
    let (status, first) = refresh(&harness.state, original).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first["refreshToken"], original);

    harness.clock.advance(Duration::seconds(2));
    let (status, _) = refresh(&harness.state, original).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "replay inside the grace window is tolerated"
    );

    harness.clock.advance(Duration::seconds(6));
    let (status, body) = refresh(&harness.state, original).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "forbidden");

    let rotated = first["refreshToken"].as_str().expect("rotated token");
    let (status, body) = refresh(&harness.state, rotated).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthenticated");
}

#[tokio::test]
async fn logout_revokes_the_refresh_family() {
    let harness = harness();
    register(&harness.state, "player@example.com", "s3cret-pass").await;
    let (_, session) = login(&harness.state, "player@example.com", "s3cret-pass").await;
    let token = session["refreshToken"].as_str().expect("refresh token");

    let (status, _) = call(
        &harness.state,
        "POST",
        "/auth/logout",
        Some(json!({"refresh_token": token})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = refresh(&harness.state, token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthenticated");
}

async fn call_with_bearer(state: &AppState, uri: &str, token: &str) -> (StatusCode, HeaderMap) {
    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    (response.status(), response.headers().clone())
}

#[tokio::test]
async fn verify_returns_identity_headers_for_the_gateway() {
    let harness = harness();
    register(&harness.state, "player@example.com", "s3cret-pass").await;
    let (_, session) = login(&harness.state, "player@example.com", "s3cret-pass").await;
    let token = session["accessToken"].as_str().expect("access token");

    let (status, headers) = call_with_bearer(&harness.state, "/auth/verify", token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        headers["x-user-id"].to_str().expect("user id header"),
        session["user"]["id"].as_str().expect("user id")
    );
    assert_eq!(
        headers["x-user-roles"].to_str().expect("roles header"),
        "student"
    );

    let (status, _) = call(&harness.state, "GET", "/auth/verify", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwks_exposes_the_signing_key() {
    let harness = harness();
    let (status, body) = call(&harness.state, "GET", "/.well-known/jwks.json", None).await;
    assert_eq!(status, StatusCode::OK);
    let keys = body["keys"].as_array().expect("keys");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["kty"], "RSA");
    assert_eq!(keys[0]["alg"], "RS256");
    assert_eq!(keys[0]["kid"], harness.state.keys.kid());
}

#[tokio::test]
async fn login_response_sets_an_http_only_cookie() {
    let harness = harness();
    register(&harness.state, "player@example.com", "s3cret-pass").await;
    let response = router(harness.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "email": "player@example.com",
                        "password": "s3cret-pass"
                    }))
                    .expect("json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("set-cookie");
    assert!(cookie.contains("refresh_token="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
}
