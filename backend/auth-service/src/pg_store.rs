//! PostgreSQL backed identity store with Redis family-revocation markers.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use common::error::{AppError, Result};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::store::{AuthStore, RefreshToken, Rotation, User};

const FAMILY_REVOKED_TTL_SEC: u64 = 30 * 86_400;

type UserRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Vec<String>,
    DateTime<Utc>,
);

#[derive(Clone)]
pub struct PgAuthStore {
    pool: PgPool,
    redis: ConnectionManager,
}

impl PgAuthStore {
    pub fn new(pool: PgPool, redis: ConnectionManager) -> Self {
        Self { pool, redis }
    }

    async fn mark_family_revoked(&self, family_id: Uuid) {
        let mut connection = self.redis.clone();
        let key = family_key(family_id);
        let result: redis::RedisResult<()> = redis::cmd("SET")
            .arg(key)
            .arg(1)
            .arg("EX")
            .arg(FAMILY_REVOKED_TTL_SEC)
            .query_async(&mut connection)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "failed to persist revoked refresh family");
        }
    }

    async fn family_revoked(&self, family_id: Uuid) -> bool {
        let mut connection = self.redis.clone();
        let key = family_key(family_id);
        let result: redis::RedisResult<i64> = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut connection)
            .await;
        match result {
            Ok(count) => count > 0,
            Err(error) => {
                tracing::warn!(%error, "failed to read revoked refresh family");
                false
            }
        }
    }
}

fn family_key(family_id: Uuid) -> String {
    format!("auth:refresh:family:{family_id}")
}

fn user_from_row(row: UserRow) -> User {
    let (id, email, password_hash, nickname, avatar_url, roles, created_at) = row;
    User {
        id,
        email,
        password_hash,
        nickname,
        avatar_url,
        roles,
        created_at,
    }
}

fn db_error(context: &str, error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref database) = error {
        if database.code().as_deref() == Some("23505") {
            return AppError::conflict("email already registered");
        }
    }
    AppError::internal(format!("{context}: {error}"))
}

#[async_trait]
impl AuthStore for PgAuthStore {
    async fn create_user(
        &self,
        email: &str,
        password_hash: String,
        nickname: String,
        now: DateTime<Utc>,
    ) -> Result<User> {
        let id = Uuid::new_v4();
        let roles = vec!["student".to_string()];
        sqlx::query(
            "INSERT INTO auth.users \
             (id, email, password_hash, nickname, avatar_url, roles, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, NULL, $5, $6, $6)",
        )
        .bind(id)
        .bind(email)
        .bind(&password_hash)
        .bind(&nickname)
        .bind(&roles)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| db_error("create user", error))?;
        Ok(User {
            id,
            email: email.to_string(),
            password_hash,
            nickname,
            avatar_url: None,
            roles,
            created_at: now,
        })
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, nickname, avatar_url, roles, created_at \
             FROM auth.users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load user by email", error))?;
        Ok(row.map(user_from_row))
    }

    async fn find_user(&self, id: Uuid) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, nickname, avatar_url, roles, created_at \
             FROM auth.users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load user", error))?;
        Ok(row.map(user_from_row))
    }

    async fn insert_refresh(&self, token: RefreshToken) -> Result<()> {
        sqlx::query(
            "INSERT INTO auth.refresh_tokens \
             (id, user_id, token_hash, family_id, expires_at, rotated_at, revoked, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(token.user_id)
        .bind(&token.token_hash)
        .bind(token.family_id)
        .bind(token.expires_at)
        .bind(token.rotated_at)
        .bind(token.revoked)
        .bind(token.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| db_error("insert refresh token", error))?;
        Ok(())
    }

    async fn rotate_refresh(
        &self,
        token_hash: &str,
        now: DateTime<Utc>,
        grace: Duration,
    ) -> Result<Rotation> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin rotation: {error}")))?;
        let row: Option<(Uuid, Uuid, DateTime<Utc>, Option<DateTime<Utc>>, bool)> = sqlx::query_as(
            "SELECT family_id, user_id, expires_at, rotated_at, revoked \
                 FROM auth.refresh_tokens WHERE token_hash = $1 FOR UPDATE",
        )
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load refresh token", error))?;
        let (family_id, user_id, expires_at, rotated_at, revoked) =
            row.ok_or_else(|| AppError::unauthenticated("unknown refresh token"))?;
        if revoked {
            return Err(AppError::unauthenticated("refresh token revoked"));
        }
        if expires_at <= now {
            return Err(AppError::unauthenticated("refresh token expired"));
        }
        if self.family_revoked(family_id).await {
            return Err(AppError::unauthenticated("refresh token revoked"));
        }
        let rotation = Rotation { user_id, family_id };
        match rotated_at {
            None => {
                sqlx::query("UPDATE auth.refresh_tokens SET rotated_at = $2 WHERE token_hash = $1")
                    .bind(token_hash)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| db_error("rotate refresh token", error))?;
                tx.commit()
                    .await
                    .map_err(|error| AppError::internal(format!("commit rotation: {error}")))?;
                Ok(rotation)
            }
            Some(rotated_at) => {
                if now - rotated_at <= grace {
                    tx.commit()
                        .await
                        .map_err(|error| AppError::internal(format!("commit rotation: {error}")))?;
                    return Ok(rotation);
                }
                sqlx::query("UPDATE auth.refresh_tokens SET revoked = true WHERE family_id = $1")
                    .bind(family_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| db_error("revoke refresh family", error))?;
                tx.commit()
                    .await
                    .map_err(|error| AppError::internal(format!("commit rotation: {error}")))?;
                self.mark_family_revoked(family_id).await;
                Err(AppError::forbidden("refresh token reuse detected"))
            }
        }
    }

    async fn revoke_family_of(&self, token_hash: &str) -> Result<()> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT family_id FROM auth.refresh_tokens WHERE token_hash = $1")
                .bind(token_hash)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| db_error("load refresh family", error))?;
        if let Some((family_id,)) = row {
            sqlx::query("UPDATE auth.refresh_tokens SET revoked = true WHERE family_id = $1")
                .bind(family_id)
                .execute(&self.pool)
                .await
                .map_err(|error| db_error("revoke refresh family", error))?;
            self.mark_family_revoked(family_id).await;
        }
        Ok(())
    }
}
