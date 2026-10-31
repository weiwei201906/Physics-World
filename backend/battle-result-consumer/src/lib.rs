//! Pure battle settlement processing.
//!
//! The consumer persists the settled battle exactly once (PostgreSQL primary
//! keys) and applies leaderboard scores exactly once (Redis marker script).
//! Tests inject in-memory fakes, so no broker or database is required.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::consumer::MessageHandler;
use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LEADERBOARD_KEY: &str = "leaderboard:all";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BattleParticipant {
    pub user_id: Uuid,
    pub score: i64,
    pub correct_count: i32,
    pub rank: i32,
}

/// Event payload produced by battle-service on topic `battle_result`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BattleResultEvent {
    pub battle_id: Uuid,
    pub mode: String,
    pub question_count: i32,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub winner_id: Option<Uuid>,
    #[serde(default)]
    pub participants: Vec<BattleParticipant>,
}

impl BattleResultEvent {
    pub fn from_payload(payload: &[u8]) -> Result<Self> {
        serde_json::from_slice(payload)
            .map_err(|error| AppError::invalid(format!("malformed battle result: {error}")))
    }
}

pub fn processed_key(battle_id: Uuid) -> String {
    format!("battle:processed:{battle_id}")
}

/// Stores the battle row and its participants. PostgreSQL de-duplicates by
/// `battle_id` / `(battle_id, user_id)`, so replays are harmless.
#[async_trait]
pub trait BattleSink: Send + Sync {
    async fn store_battle(&self, event: &BattleResultEvent) -> Result<()>;
}

/// Applies participant scores once per battle. The production implementation
/// checks `battle:processed:{id}` and runs ZINCRBY plus SETEX atomically in Lua.
#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn apply_once(
        &self,
        battle_id: Uuid,
        participants: &[BattleParticipant],
        ttl_sec: i64,
    ) -> Result<bool>;
}

pub struct BattleResultProcessor {
    sink: Arc<dyn BattleSink>,
    store: Arc<dyn IdempotencyStore>,
    idempotency_ttl_sec: i64,
}

impl BattleResultProcessor {
    pub fn new(
        sink: Arc<dyn BattleSink>,
        store: Arc<dyn IdempotencyStore>,
        idempotency_ttl_sec: i64,
    ) -> Self {
        Self {
            sink,
            store,
            idempotency_ttl_sec,
        }
    }

    pub async fn process(&self, payload: &[u8]) -> Result<()> {
        let event = BattleResultEvent::from_payload(payload)?;
        self.sink.store_battle(&event).await?;
        self.store
            .apply_once(
                event.battle_id,
                &event.participants,
                self.idempotency_ttl_sec,
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for BattleResultProcessor {
    async fn handle(&self, payload: &[u8]) -> Result<()> {
        self.process(payload).await
    }
}

/// In-memory stand-in for the PostgreSQL sink, mirroring `ON CONFLICT DO NOTHING`.
#[derive(Debug, Default)]
pub struct InMemoryBattleSink {
    battles: Mutex<HashSet<Uuid>>,
}

impl InMemoryBattleSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn battle_count(&self) -> usize {
        self.battles.lock().map(|set| set.len()).unwrap_or_default()
    }
}

#[async_trait]
impl BattleSink for InMemoryBattleSink {
    async fn store_battle(&self, event: &BattleResultEvent) -> Result<()> {
        let mut battles = self
            .battles
            .lock()
            .map_err(|_| AppError::internal("battle sink poisoned"))?;
        battles.insert(event.battle_id);
        Ok(())
    }
}

/// In-memory stand-in for the Redis marker script.
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyStore {
    markers: Mutex<HashSet<Uuid>>,
    leaderboard: Mutex<HashMap<Uuid, i64>>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn marker_count(&self) -> usize {
        self.markers
            .lock()
            .map(|markers| markers.len())
            .unwrap_or_default()
    }

    pub fn leaderboard_value(&self, user_id: Uuid) -> i64 {
        self.leaderboard
            .lock()
            .map(|board| board.get(&user_id).copied().unwrap_or_default())
            .unwrap_or_default()
    }
}

#[async_trait]
impl IdempotencyStore for InMemoryIdempotencyStore {
    async fn apply_once(
        &self,
        battle_id: Uuid,
        participants: &[BattleParticipant],
        _ttl_sec: i64,
    ) -> Result<bool> {
        let mut markers = self
            .markers
            .lock()
            .map_err(|_| AppError::internal("battle markers poisoned"))?;
        if markers.contains(&battle_id) {
            return Ok(false);
        }
        let mut leaderboard = self
            .leaderboard
            .lock()
            .map_err(|_| AppError::internal("leaderboard poisoned"))?;
        for participant in participants {
            *leaderboard.entry(participant.user_id).or_insert(0) += participant.score;
        }
        markers.insert(battle_id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::consumer::{
        run_loop, ConsumerBackend, InMemoryRetryCounter, MessageEnvelope, RetryPolicy,
    };

    use super::*;

    fn sample_event() -> BattleResultEvent {
        BattleResultEvent {
            battle_id: Uuid::new_v4(),
            mode: "ranked".to_string(),
            question_count: 10,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            winner_id: None,
            participants: vec![
                BattleParticipant {
                    user_id: Uuid::new_v4(),
                    score: 80,
                    correct_count: 8,
                    rank: 1,
                },
                BattleParticipant {
                    user_id: Uuid::new_v4(),
                    score: 40,
                    correct_count: 4,
                    rank: 2,
                },
            ],
        }
    }

    fn processor() -> (
        BattleResultProcessor,
        Arc<InMemoryBattleSink>,
        Arc<InMemoryIdempotencyStore>,
    ) {
        let sink = Arc::new(InMemoryBattleSink::new());
        let store = Arc::new(InMemoryIdempotencyStore::new());
        let processor = BattleResultProcessor::new(sink.clone(), store.clone(), 60);
        (processor, sink, store)
    }

    #[tokio::test]
    async fn duplicate_settlement_applies_scores_once() {
        let (processor, sink, store) = processor();
        let event = sample_event();
        let payload = serde_json::to_vec(&event).expect("serialize");
        processor.process(&payload).await.expect("first delivery");
        processor
            .process(&payload)
            .await
            .expect("duplicate delivery");

        assert_eq!(sink.battle_count(), 1);
        assert_eq!(store.marker_count(), 1);
        for participant in &event.participants {
            assert_eq!(
                store.leaderboard_value(participant.user_id),
                participant.score
            );
        }
    }

    #[tokio::test]
    async fn malformed_json_is_rejected() {
        let (processor, sink, store) = processor();
        let error = processor.process(b"{not json").await.expect_err("invalid");
        assert_eq!(error.code(), "invalid_request");
        assert_eq!(sink.battle_count(), 0);
        assert_eq!(store.marker_count(), 0);
    }

    #[derive(Default)]
    struct FakeBackend {
        messages: Mutex<Vec<MessageEnvelope>>,
        last: Mutex<Option<MessageEnvelope>>,
        committed: Mutex<Vec<(i32, i64)>>,
        dlq: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ConsumerBackend for FakeBackend {
        async fn recv(&self) -> Result<Option<MessageEnvelope>> {
            let mut messages = self
                .messages
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?;
            if messages.is_empty() {
                return Ok(None);
            }
            let message = messages.remove(0);
            if let Ok(mut last) = self.last.lock() {
                *last = Some(message.clone());
            }
            Ok(Some(message))
        }

        async fn commit(&self, partition: i32, offset: i64) -> Result<()> {
            let mut committed = self
                .committed
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?;
            committed.push((partition, offset));
            Ok(())
        }

        async fn seek(&self, _partition: i32, _offset: i64) -> Result<()> {
            let message = self
                .last
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?
                .clone();
            if let Some(message) = message {
                let mut messages = self
                    .messages
                    .lock()
                    .map_err(|_| AppError::internal("fake backend poisoned"))?;
                messages.push(message);
            }
            Ok(())
        }

        async fn publish_dlq(&self, envelope: &MessageEnvelope, reason: &str) -> Result<()> {
            let mut dlq = self
                .dlq
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?;
            dlq.push(format!("{}:{reason}", envelope.offset));
            Ok(())
        }
    }

    #[tokio::test]
    async fn malformed_message_retries_then_goes_to_dlq() {
        let backend = Arc::new(FakeBackend::default());
        backend
            .messages
            .lock()
            .expect("messages")
            .push(MessageEnvelope {
                topic: "battle_result".to_string(),
                partition: 0,
                offset: 9,
                key: None,
                payload: b"{not json".to_vec(),
            });
        let handler: Arc<dyn MessageHandler> = Arc::new(processor().0);
        run_loop(
            backend.clone(),
            handler,
            RetryPolicy::new(1, Duration::from_millis(1)),
            Arc::new(InMemoryRetryCounter::new()),
        )
        .await
        .expect("loop completes");

        assert_eq!(backend.committed.lock().expect("committed").len(), 1);
        assert_eq!(backend.dlq.lock().expect("dlq").len(), 1);
    }
}
