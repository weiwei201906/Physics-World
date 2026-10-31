//! In-process identity store.
//!
//! The production deployment swaps this for PostgreSQL plus Redis. The trait
//! shape stays the same: users keyed by id and normalized email, refresh tokens
//! keyed by their SHA-256 digest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Duration, Utc};
use common::error::{AppError, Result};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub token_hash: String,
    pub family_id: Uuid,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation {
    pub user_id: Uuid,
    pub family_id: Uuid,
}

#[derive(Debug, Default)]
struct Inner {
    users: HashMap<Uuid, User>,
    emails: HashMap<String, Uuid>,
    refresh: HashMap<String, RefreshToken>,
}

#[derive(Debug, Clone, Default)]
pub struct AuthStore {
    inner: Arc<Mutex<Inner>>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|_| AppError::internal("auth store poisoned"))
    }

    pub fn create_user(
        &self,
        email: &str,
        password_hash: String,
        nickname: String,
        now: DateTime<Utc>,
    ) -> Result<User> {
        let mut inner = self.lock()?;
        if inner.emails.contains_key(email) {
            return Err(AppError::conflict("email already registered"));
        }
        let user = User {
            id: Uuid::new_v4(),
            email: email.to_string(),
            password_hash,
            nickname,
            avatar_url: None,
            roles: vec!["student".to_string()],
            created_at: now,
        };
        inner.emails.insert(email.to_string(), user.id);
        inner.users.insert(user.id, user.clone());
        Ok(user)
    }

    pub fn find_user_by_email(&self, email: &str) -> Option<User> {
        let inner = self.lock().ok()?;
        let id = inner.emails.get(email)?;
        inner.users.get(id).cloned()
    }

    pub fn find_user(&self, id: Uuid) -> Option<User> {
        self.lock().ok()?.users.get(&id).cloned()
    }

    pub fn insert_refresh(&self, token: RefreshToken) -> Result<()> {
        let mut inner = self.lock()?;
        inner.refresh.insert(token.token_hash.clone(), token);
        Ok(())
    }

    /// Marks the presented token as rotated. Tokens replayed inside the grace
    /// window are tolerated; replays after the window revoke the whole family.
    pub fn rotate_refresh(
        &self,
        token_hash: &str,
        now: DateTime<Utc>,
        grace: Duration,
    ) -> Result<Rotation> {
        let mut inner = self.lock()?;
        let record = inner
            .refresh
            .get(token_hash)
            .cloned()
            .ok_or_else(|| AppError::unauthenticated("unknown refresh token"))?;
        if record.revoked {
            return Err(AppError::unauthenticated("refresh token revoked"));
        }
        if record.expires_at <= now {
            return Err(AppError::unauthenticated("refresh token expired"));
        }
        let rotation = Rotation {
            user_id: record.user_id,
            family_id: record.family_id,
        };
        match record.rotated_at {
            None => {
                if let Some(stored) = inner.refresh.get_mut(token_hash) {
                    stored.rotated_at = Some(now);
                }
                Ok(rotation)
            }
            Some(rotated_at) => {
                if now - rotated_at <= grace {
                    return Ok(rotation);
                }
                for token in inner.refresh.values_mut() {
                    if token.family_id == record.family_id {
                        token.revoked = true;
                    }
                }
                Err(AppError::forbidden("refresh token reuse detected"))
            }
        }
    }

    pub fn revoke_family_of(&self, token_hash: &str) -> Result<()> {
        let mut inner = self.lock()?;
        if let Some(family_id) = inner.refresh.get(token_hash).map(|token| token.family_id) {
            for token in inner.refresh.values_mut() {
                if token.family_id == family_id {
                    token.revoked = true;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn token(store: &AuthStore, family: Uuid, hash: &str, now: DateTime<Utc>) {
        store
            .insert_refresh(RefreshToken {
                token_hash: hash.to_string(),
                family_id: family,
                user_id: Uuid::nil(),
                created_at: now,
                expires_at: now + Duration::days(30),
                rotated_at: None,
                revoked: false,
            })
            .expect("insert token");
    }

    #[test]
    fn duplicate_email_is_rejected_case_insensitively_by_caller() {
        let store = AuthStore::new();
        store
            .create_user(
                "player@example.com",
                "hash".to_string(),
                "player".to_string(),
                now(),
            )
            .expect("create");
        let error = store
            .create_user(
                "player@example.com",
                "hash".to_string(),
                "player".to_string(),
                now(),
            )
            .expect_err("duplicate");
        assert_eq!(error.code(), "conflict");
    }

    #[test]
    fn replay_inside_grace_is_allowed_but_after_grace_revokes_family() {
        let store = AuthStore::new();
        let start = now();
        token(&store, Uuid::new_v4(), "t1", start);
        store
            .rotate_refresh("t1", start, Duration::seconds(5))
            .expect("first rotation");
        store
            .rotate_refresh("t1", start + Duration::seconds(2), Duration::seconds(5))
            .expect("replay within grace");
        let error = store
            .rotate_refresh("t1", start + Duration::seconds(10), Duration::seconds(5))
            .expect_err("replay outside grace");
        assert_eq!(error.code(), "forbidden");
        let revoked = store
            .rotate_refresh("t1", start + Duration::seconds(11), Duration::seconds(5))
            .expect_err("revoked");
        assert_eq!(revoked.code(), "unauthenticated");
    }

    #[test]
    fn expired_tokens_are_rejected() {
        let store = AuthStore::new();
        let start = now();
        store
            .insert_refresh(RefreshToken {
                token_hash: "t1".to_string(),
                family_id: Uuid::new_v4(),
                user_id: Uuid::nil(),
                created_at: start,
                expires_at: start - Duration::seconds(1),
                rotated_at: None,
                revoked: false,
            })
            .expect("insert");
        let error = store
            .rotate_refresh("t1", start, Duration::seconds(5))
            .expect_err("expired");
        assert_eq!(error.code(), "unauthenticated");
    }
}
