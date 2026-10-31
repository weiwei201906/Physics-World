//! Identity injected by the gateway.
//!
//! Downstream services trust `X-User-Id`, `X-User-Roles` and `X-Real-IP`
//! because NetworkPolicy forbids direct pod access. The gateway strips these
//! headers from client requests before injecting its own values.

use std::net::IpAddr;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use uuid::Uuid;

use crate::error::{AppError, Result};

pub const HEADER_USER_ID: &str = "x-user-id";
pub const HEADER_USER_ROLES: &str = "x-user-roles";
pub const HEADER_REAL_IP: &str = "x-real-ip";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub user_id: Uuid,
    pub roles: Vec<String>,
    pub real_ip: Option<IpAddr>,
}

impl Identity {
    pub fn from_headers(headers: &HeaderMap) -> Result<Self> {
        let raw_user_id = headers
            .get(HEADER_USER_ID)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::unauthenticated("missing X-User-Id"))?;
        let user_id = Uuid::parse_str(raw_user_id.trim())
            .map_err(|_| AppError::unauthenticated("invalid X-User-Id"))?;
        let roles = headers
            .get(HEADER_USER_ROLES)
            .and_then(|value| value.to_str().ok())
            .map(parse_roles)
            .unwrap_or_default();
        let real_ip = headers
            .get(HEADER_REAL_IP)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<IpAddr>().ok());
        Ok(Self {
            user_id,
            roles,
            real_ip,
        })
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(role))
    }

    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

fn parse_roles(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Extractor that rejects requests without a valid gateway identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated(pub Identity);

impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Identity::from_headers(&parts.headers).map(Authenticated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (key, value) in pairs {
            if let Ok(value) = HeaderValue::from_str(value) {
                if let Ok(name) = axum::http::HeaderName::from_bytes(key.as_bytes()) {
                    headers.insert(name, value);
                }
            }
        }
        headers
    }

    #[test]
    fn parses_identity_with_roles_and_ip() {
        let identity = Identity::from_headers(&headers(&[
            ("x-user-id", "0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d"),
            ("x-user-roles", "student, admin"),
            ("x-real-ip", "10.1.2.3"),
        ]))
        .expect("valid identity");
        assert_eq!(identity.roles, vec!["student", "admin"]);
        assert!(identity.is_admin());
        assert!(identity.real_ip.is_some());
    }

    #[test]
    fn missing_user_id_is_unauthenticated() {
        let error = Identity::from_headers(&headers(&[("x-user-roles", "admin")]))
            .expect_err("missing identity");
        assert_eq!(error.code(), "unauthenticated");
    }

    #[test]
    fn malformed_user_id_is_unauthenticated() {
        let error = Identity::from_headers(&headers(&[("x-user-id", "not-a-uuid")]))
            .expect_err("invalid identity");
        assert_eq!(error.code(), "unauthenticated");
    }

    #[test]
    fn invalid_ip_is_ignored_but_identity_is_kept() {
        let identity = Identity::from_headers(&headers(&[
            ("x-user-id", "0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d"),
            ("x-real-ip", "garbage"),
        ]))
        .expect("valid identity");
        assert!(identity.real_ip.is_none());
    }

    #[test]
    fn role_matching_is_case_insensitive() {
        let identity = Identity {
            user_id: Uuid::nil(),
            roles: vec!["ADMIN".to_string()],
            real_ip: None,
        };
        assert!(identity.is_admin());
    }
}
