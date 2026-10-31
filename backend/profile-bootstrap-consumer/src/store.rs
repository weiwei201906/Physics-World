//! PostgreSQL implementation behind the pure profile processor.

use async_trait::async_trait;
use common::error::{AppError, Result};
use sqlx::PgPool;

use profile_bootstrap_consumer::{ProfileSink, UserRegisteredEvent};

pub struct PgProfileSink {
    pool: PgPool,
}

impl PgProfileSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProfileSink for PgProfileSink {
    async fn insert_profile(&self, event: &UserRegisteredEvent) -> Result<()> {
        sqlx::query(
            "INSERT INTO app.user_profiles \
             (user_id, email, nickname, avatar_url, roles, created_at, updated_at) \
             VALUES ($1, $2, $3, NULL, ARRAY['student']::text[], now(), now()) \
             ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(event.user_id)
        .bind(&event.email)
        .bind(&event.nickname)
        .execute(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("insert profile: {error}")))?;
        Ok(())
    }
}
