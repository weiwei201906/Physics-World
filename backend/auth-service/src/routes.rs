//! HTTP surface for the auth service.

use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{AUTHORIZATION, COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use common::clock::Clock;
use common::defaults;
use common::error::{AppError, Result};
use common::settings::AuthSettings;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::password::{hash_password, verify_password};
use crate::store::{AuthStore, RefreshToken, User};
use crate::tokens::{hash_refresh_token, KeyRing};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<AuthSettings>,
    pub store: Arc<dyn AuthStore>,
    pub keys: Arc<KeyRing>,
    pub clock: Arc<dyn Clock>,
}

impl AppState {
    pub fn new(
        settings: AuthSettings,
        store: Arc<dyn AuthStore>,
        keys: Arc<KeyRing>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            settings: Arc::new(settings),
            store,
            keys,
            clock,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh-token", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/verify", get(verify))
        .route("/.well-known/jwks.json", get(jwks))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "auth-service",
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserView {
    id: Uuid,
    email: String,
    nickname: String,
    avatar_url: Option<String>,
    roles: Vec<String>,
    created_at: DateTime<Utc>,
}

impl From<&User> for UserView {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            nickname: user.nickname.clone(),
            avatar_url: user.avatar_url.clone(),
            roles: user.roles.clone(),
            created_at: user.created_at,
        }
    }
}

#[derive(Deserialize)]
struct RegisterRequest {
    email: String,
    password: String,
    nickname: Option<String>,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize, Default)]
struct RefreshRequest {
    refresh_token: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    exp: i64,
    refresh_token: String,
    user: UserView,
}

fn normalized_email(raw: &str) -> Result<String> {
    let email = raw.trim().to_ascii_lowercase();
    if email.is_empty() || email.len() > 320 || !email.contains('@') {
        return Err(AppError::invalid("email is not valid"));
    }
    Ok(email)
}

fn validate_password(settings: &AuthSettings, password: &str) -> Result<()> {
    if password.chars().count() < settings.password_min_length {
        return Err(AppError::invalid(format!(
            "password must contain at least {} characters",
            settings.password_min_length
        )));
    }
    Ok(())
}

async fn hash_in_background(settings: Arc<AuthSettings>, password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&settings, &password))
        .await
        .map_err(|error| AppError::internal(format!("password task failed: {error}")))?
}

async fn verify_in_background(
    settings: Arc<AuthSettings>,
    password: String,
    encoded: String,
) -> Result<bool> {
    tokio::task::spawn_blocking(move || verify_password(&settings, &password, &encoded))
        .await
        .map_err(|error| AppError::internal(format!("password task failed: {error}")))?
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Response> {
    let email = normalized_email(&request.email)?;
    validate_password(&state.settings, &request.password)?;
    let password_hash = hash_in_background(state.settings.clone(), request.password).await?;
    let nickname = request
        .nickname
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| defaults::PLACEHOLDER_NICKNAME.to_string());
    let user = state
        .store
        .create_user(&email, password_hash, nickname, state.clock.now())
        .await?;
    let mut response = issue_tokens(&state, &user, None).await?;
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response> {
    let email = normalized_email(&request.email)?;
    let user = state
        .store
        .find_user_by_email(&email)
        .await?
        .ok_or_else(|| AppError::unauthenticated("invalid email or password"))?;
    let verified = verify_in_background(
        state.settings.clone(),
        request.password,
        user.password_hash.clone(),
    )
    .await?;
    if !verified {
        return Err(AppError::unauthenticated("invalid email or password"));
    }
    issue_tokens(&state, &user, None).await
}

async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RefreshRequest>,
) -> Result<Response> {
    let raw = request
        .refresh_token
        .or_else(|| cookie_token(&headers, &state.settings.cookie_name))
        .ok_or_else(|| AppError::unauthenticated("missing refresh token"))?;
    let rotation = state
        .store
        .rotate_refresh(
            &hash_refresh_token(&raw),
            state.clock.now(),
            state.settings.refresh_grace(),
        )
        .await?;
    let user = state
        .store
        .find_user(rotation.user_id)
        .await?
        .ok_or_else(|| AppError::unauthenticated("refresh token owner is missing"))?;
    issue_tokens(&state, &user, Some(rotation.family_id)).await
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<RefreshRequest>>,
) -> Result<Response> {
    let raw = body
        .and_then(|Json(request)| request.refresh_token)
        .or_else(|| cookie_token(&headers, &state.settings.cookie_name));
    if let Some(raw) = raw {
        state
            .store
            .revoke_family_of(&hash_refresh_token(&raw))
            .await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn jwks(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.keys.jwks())
}

/// Gateway sub-request target: validates a Bearer token and returns the
/// identity headers the gateway forwards to downstream services.
async fn verify(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthenticated("missing bearer token"))?;
    let claims = state.keys.verify_access_token(token)?;
    let values = [
        ("x-user-id", claims.sub),
        ("x-user-roles", claims.roles.join(",")),
        ("x-user-email", claims.email),
    ];
    let mut response = StatusCode::NO_CONTENT.into_response();
    for (name, value) in values {
        let header = HeaderValue::from_str(&value)
            .map_err(|error| AppError::internal(format!("invalid identity header: {error}")))?;
        response
            .headers_mut()
            .insert(HeaderName::from_static(name), header);
    }
    Ok(response)
}

async fn issue_tokens(state: &AppState, user: &User, family_id: Option<Uuid>) -> Result<Response> {
    let now = state.clock.now();
    let (access_token, expires_in) = state.keys.issue_access_token(
        user.id,
        &user.email,
        &user.roles,
        now,
        state.settings.access_ttl(),
    )?;
    let refresh_raw = state
        .keys
        .generate_refresh_token(state.settings.refresh_token_bytes)?;
    state
        .store
        .insert_refresh(RefreshToken {
            token_hash: hash_refresh_token(&refresh_raw),
            family_id: family_id.unwrap_or_else(Uuid::new_v4),
            user_id: user.id,
            created_at: now,
            expires_at: now + state.settings.refresh_ttl(),
            rotated_at: None,
            revoked: false,
        })
        .await?;
    let body = TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in,
        exp: now.timestamp() + expires_in,
        refresh_token: refresh_raw.clone(),
        user: UserView::from(user),
    };
    let cookie = refresh_cookie(&state.settings, &refresh_raw)?;
    let mut response = Json(body).into_response();
    response.headers_mut().insert(SET_COOKIE, cookie);
    Ok(response)
}

fn refresh_cookie(settings: &AuthSettings, token: &str) -> Result<HeaderValue> {
    let mut value = format!(
        "{}={}; Path={}; Max-Age={}; HttpOnly; SameSite={}",
        settings.cookie_name,
        token,
        settings.cookie_path,
        settings.cookie_max_age_sec(),
        settings.cookie_same_site
    );
    if settings.cookie_secure {
        value.push_str("; Secure");
    }
    HeaderValue::from_str(&value)
        .map_err(|error| AppError::internal(format!("invalid cookie value: {error}")))
}

fn cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::config::ConfigLoader;
    use std::collections::HashMap;

    #[test]
    fn email_is_trimmed_and_lowercased() {
        assert_eq!(
            normalized_email("  Player@Example.COM ").expect("valid"),
            "player@example.com"
        );
        assert!(normalized_email("player").is_err());
    }

    #[test]
    fn password_length_is_enforced() {
        let settings = AuthSettings::from_config(&ConfigLoader::new("auth", HashMap::new()));
        assert!(validate_password(&settings, "short").is_err());
        assert!(validate_password(&settings, "longenough").is_ok());
    }

    #[test]
    fn cookie_parser_reads_named_value_only() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str("other=1; refresh_token=abc.def; theme=dark").expect("cookie"),
        );
        assert_eq!(
            cookie_token(&headers, "refresh_token"),
            Some("abc.def".to_string())
        );
    }
}
