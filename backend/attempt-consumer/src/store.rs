//! PostgreSQL and Redis implementations behind the pure attempt processor.

use async_trait::async_trait;
use common::error::{AppError, Result};
use redis::aio::ConnectionManager;
use redis::Script;
use sqlx::PgPool;

use attempt_consumer::{
    image_marker_key, profile_key, rank_marker_key, AttemptEvent, AttemptSink, IdempotencyStore,
};

pub struct PgAttemptSink {
    pool: PgPool,
}

impl PgAttemptSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AttemptSink for PgAttemptSink {
    async fn insert_attempt(&self, event: &AttemptEvent) -> Result<()> {
        sqlx::query(
            "INSERT INTO app.attempts \
             (attempt_id, user_id, session_id, question_id, is_correct, answer, submitted_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (attempt_id) DO NOTHING",
        )
        .bind(event.attempt_id)
        .bind(event.user_id)
        .bind(event.session_id)
        .bind(&event.question_id)
        .bind(event.is_correct)
        .bind(&event.answer)
        .bind(event.submitted_at)
        .execute(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("insert attempt: {error}")))?;
        Ok(())
    }
}

pub struct RedisAttemptStore {
    redis: ConnectionManager,
    leaderboard_key: String,
}

impl RedisAttemptStore {
    pub fn new(redis: ConnectionManager, leaderboard_key: impl Into<String>) -> Self {
        Self {
            redis,
            leaderboard_key: leaderboard_key.into(),
        }
    }

    /// A single atomic script: marker check, HINCRBY, ZINCRBY and SETEX.
    /// KEYS: image marker, rank marker, profile hash, leaderboard zset.
    /// ARGV: ttl seconds, score delta, user id, correct flag.
    const APPLY_SCRIPT: &'static str = r#"
local applied = 0
if redis.call('EXISTS', KEYS[1]) == 0 then
  redis.call('HINCRBY', KEYS[3], 'attempts_total', 1)
  if tonumber(ARGV[4]) == 1 then
    redis.call('HINCRBY', KEYS[3], 'correct_total', 1)
  end
  redis.call('SETEX', KEYS[1], ARGV[1], '1')
  applied = 1
end
if redis.call('EXISTS', KEYS[2]) == 0 then
  redis.call('ZINCRBY', KEYS[4], ARGV[2], ARGV[3])
  redis.call('SETEX', KEYS[2], ARGV[1], '1')
  applied = 1
end
return applied
"#;
}

#[async_trait]
impl IdempotencyStore for RedisAttemptStore {
    async fn apply_once(&self, event: &AttemptEvent, ttl_sec: i64) -> Result<bool> {
        let score_delta = i64::from(event.is_correct);
        let mut conn = self.redis.clone();
        let applied: i64 = Script::new(Self::APPLY_SCRIPT)
            .key(image_marker_key(event.attempt_id))
            .key(rank_marker_key(event.attempt_id))
            .key(profile_key(event.user_id))
            .key(&self.leaderboard_key)
            .arg(ttl_sec)
            .arg(score_delta)
            .arg(event.user_id.to_string())
            .arg(i64::from(event.is_correct))
            .invoke_async(&mut conn)
            .await
            .map_err(|error| AppError::upstream("redis", error.to_string()))?;
        Ok(applied > 0)
    }
}
