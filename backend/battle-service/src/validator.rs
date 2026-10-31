//! Access-token validation against auth-service.
//!
//! A browser cannot attach an `Authorization` header to a WebSocket handshake,
//! so the token arrives in the query string. The gateway cannot forward query
//! arguments into an `auth_request` sub-request, so battle-service verifies the
//! token with auth-service's `/auth/verify` endpoint itself.

use async_trait::async_trait;
use axum::body::Bytes;
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use chrono::DateTime;
use common::error::{AppError, Result};
use http_body_util::Empty;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use uuid::Uuid;

use crate::ws::{TokenClaims, TokenValidator};

pub struct HttpTokenValidator {
    client: Client<HttpConnector, Empty<Bytes>>,
    verify_url: String,
}

impl HttpTokenValidator {
    /// `auth_base_url` is the auth-service root, for example
    /// `http://127.0.0.1:8081`.
    pub fn new(auth_base_url: &str) -> Self {
        Self {
            client: Client::builder(TokioExecutor::new()).build_http(),
            verify_url: format!("{}/auth/verify", auth_base_url.trim_end_matches('/')),
        }
    }
}

#[async_trait]
impl TokenValidator for HttpTokenValidator {
    async fn validate(&self, token: &str) -> Result<TokenClaims> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::unauthenticated("empty token"));
        }

        let request = Request::builder()
            .method(Method::GET)
            .uri(&self.verify_url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Empty::new())
            .map_err(|error| AppError::internal(format!("verify request failed: {error}")))?;

        let response =
            self.client.request(request).await.map_err(|error| {
                AppError::unavailable(format!("auth-service unreachable: {error}"))
            })?;

        if response.status() != StatusCode::NO_CONTENT {
            return Err(AppError::unauthenticated("invalid access token"));
        }

        claims_from_headers(response.headers())
    }
}

fn claims_from_headers(headers: &HeaderMap) -> Result<TokenClaims> {
    let user_id = headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or_else(|| AppError::unauthenticated("auth-service returned no identity"))?;

    let expires_at = headers
        .get("x-user-exp")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|secs| DateTime::from_timestamp(secs, 0))
        .ok_or_else(|| AppError::unauthenticated("auth-service returned no expiry"))?;

    Ok(TokenClaims {
        user_id,
        expires_at,
        jti: None,
    })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn identity_headers_map_to_claims() {
        let mut headers = HeaderMap::new();
        let user_id = Uuid::new_v4();
        headers.insert(
            "x-user-id",
            HeaderValue::from_str(&user_id.to_string()).unwrap(),
        );
        headers.insert("x-user-exp", HeaderValue::from_str("1700000000").unwrap());
        let claims = claims_from_headers(&headers).expect("claims");
        assert_eq!(claims.user_id, user_id);
        assert_eq!(claims.expires_at.timestamp(), 1700000000);
    }

    #[test]
    fn missing_identity_is_unauthenticated() {
        let error = claims_from_headers(&HeaderMap::new()).expect_err("error");
        assert_eq!(error.code(), "unauthenticated");
    }
}
