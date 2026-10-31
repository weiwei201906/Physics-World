//! PostgreSQL and Redis implementations behind the pure battle processor.

use async_trait::async_trait;
use common::error::{AppError, Result};
use redis::aio::ConnectionManager;
use redis::Script;
use sqlx::PgPool;
use uuid::Uuid;

use battle_result_consumer::{
    processed_key, BattleParticipant, BattleResultEvent, BattleSink, IdempotencyStore,
};

pub struct PgBattleSink {
    pool: PgPool,
}

impl PgBattleSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BattleSink for PgBattleSink {
    async fn store_battle(&self, event: &BattleResultEvent) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin battle tx: {error}")))?;
        sqlx::query(
            "INSERT INTO battle.battles \
             (battle_id, mode, question_count, started_at, finished_at, winner_id, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now()) \
             ON CONFLICT (battle_id) DO NOTHING",
        )
        .bind(event.battle_id)
        .bind(&event.mode)
        .bind(event.question_count)
        .bind(event.started_at)
        .bind(event.finished_at)
        .bind(event.winner_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal(format!("insert battle: {error}")))?;

        for participant in &event.participants {
            sqlx::query(
                "INSERT INTO battle.battle_participants \
                 (battle_id, user_id, score, correct_count, rank) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (battle_id, user_id) DO NOTHING",
            )
            .bind(event.battle_id)
            .bind(participant.user_id)
            .bind(participant.score)
            .bind(participant.correct_count)
            .bind(participant.rank)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal(format!("insert battle participant: {error}")))?;
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit battle tx: {error}")))?;
        Ok(())
    }
}

pub struct RedisBattleStore {
    redis: ConnectionManager,
    leaderboard_key: String,
}

impl RedisBattleStore {
    pub fn new(redis: ConnectionManager, leaderboard_key: impl Into<String>) -> Self {
        Self {
            redis,
            leaderboard_key: leaderboard_key.into(),
        }
    }

    /// Atomic marker check plus one ZINCRBY per participant and SETEX.
    /// KEYS: processed marker, leaderboard zset.
    /// ARGV: ttl seconds, then (member, score) pairs.
    const APPLY_SCRIPT: &'static str = r#"
if redis.call('EXISTS', KEYS[1]) == 1 then
  return 0
end
local i = 2
while i < #ARGV do
  redis.call('ZINCRBY', KEYS[2], ARGV[i + 1], ARGV[i])
  i = i + 2
end
redis.call('SETEX', KEYS[1], ARGV[1], '1')
return 1
"#;
}

#[async_trait]
impl IdempotencyStore for RedisBattleStore {
    async fn apply_once(
        &self,
        battle_id: Uuid,
        participants: &[BattleParticipant],
        ttl_sec: i64,
    ) -> Result<bool> {
        let mut conn = self.redis.clone();
        let script = Script::new(Self::APPLY_SCRIPT);
        let mut invocation = script.prepare_invoke();
        invocation
            .key(processed_key(battle_id))
            .key(&self.leaderboard_key)
            .arg(ttl_sec);
        for participant in participants {
            invocation
                .arg(participant.user_id.to_string())
                .arg(participant.score);
        }
        let applied: i64 = invocation
            .invoke_async(&mut conn)
            .await
            .map_err(|error| AppError::upstream("redis", error.to_string()))?;
        Ok(applied > 0)
    }
}
