//! Pure attempt-event processing.
//!
//! The processor never touches Kafka, PostgreSQL or Redis directly. Storage
//! is injected through [AttemptSink] and [IdempotencyStore], so unit tests run
//! with in-memory fakes and need no external services.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::consumer::MessageHandler;
use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LEADERBOARD_KEY: &str = "leaderboard:all";

/// Event payload produced by user-api's outbox on topic `attempts`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AttemptEvent {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub question_id: String,
    pub is_correct: bool,
    pub answer: String,
    pub submitted_at: DateTime<Utc>,
}

impl AttemptEvent {
    pub fn from_payload(payload: &[u8]) -> Result<Self> {
        serde_json::from_slice(payload)
            .map_err(|error| AppError::invalid(format!("malformed attempt event: {error}")))
    }
}

pub fn image_marker_key(attempt_id: Uuid) -> String {
    format!("attempt:redis:image:{attempt_id}")
}

pub fn rank_marker_key(attempt_id: Uuid) -> String {
    format!("attempt:redis:rank:{attempt_id}")
}

pub fn profile_key(user_id: Uuid) -> String {
    format!("profile:delta:{user_id}")
}

/// Stores the authoritative attempt row. PostgreSQL de-duplicates by
/// `attempt_id`, so the same event may be replayed safely.
#[async_trait]
pub trait AttemptSink: Send + Sync {
    async fn insert_attempt(&self, event: &AttemptEvent) -> Result<()>;
}

/// Applies profile and leaderboard increments once per attempt.
///
/// The production implementation performs the marker check, HINCRBY, ZINCRBY
/// and SETEX inside a single Redis Lua invocation.
#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn apply_once(&self, event: &AttemptEvent, ttl_sec: i64) -> Result<bool>;
}

pub struct AttemptProcessor {
    sink: Arc<dyn AttemptSink>,
    store: Arc<dyn IdempotencyStore>,
    idempotency_ttl_sec: i64,
}

impl AttemptProcessor {
    pub fn new(
        sink: Arc<dyn AttemptSink>,
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
        let event = AttemptEvent::from_payload(payload)?;
        self.sink.insert_attempt(&event).await?;
        self.store
            .apply_once(&event, self.idempotency_ttl_sec)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for AttemptProcessor {
    async fn handle(&self, payload: &[u8]) -> Result<()> {
        self.process(payload).await
    }
}

/// In-memory stand-in for the PostgreSQL sink. It mirrors the
/// `ON CONFLICT (attempt_id) DO NOTHING` primary-key behavior.
#[derive(Debug, Default)]
pub struct InMemoryAttemptSink {
    attempts: Mutex<HashSet<Uuid>>,
}

impl InMemoryAttemptSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attempt_count(&self) -> usize {
        self.attempts
            .lock()
            .map(|set| set.len())
            .unwrap_or_default()
    }
}

#[async_trait]
impl AttemptSink for InMemoryAttemptSink {
    async fn insert_attempt(&self, event: &AttemptEvent) -> Result<()> {
        let mut attempts = self
            .attempts
            .lock()
            .map_err(|_| AppError::internal("attempt sink poisoned"))?;
        attempts.insert(event.attempt_id);
        Ok(())
    }
}

/// In-memory stand-in for the Redis Lua store. Markers model the SETEX keys
/// and the two counters model HINCRBY / ZINCRBY.
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyStore {
    markers: Mutex<HashSet<String>>,
    profile: Mutex<HashMap<String, i64>>,
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

    pub fn profile_value(&self, user_id: Uuid, field: &str) -> i64 {
        let key = format!("{}:{field}", profile_key(user_id));
        self.profile
            .lock()
            .map(|profile| profile.get(&key).copied().unwrap_or_default())
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
    async fn apply_once(&self, event: &AttemptEvent, _ttl_sec: i64) -> Result<bool> {
        let marker_image = image_marker_key(event.attempt_id);
        let marker_rank = rank_marker_key(event.attempt_id);
        let mut markers = self
            .markers
            .lock()
            .map_err(|_| AppError::internal("idempotency markers poisoned"))?;
        let mut profile = self
            .profile
            .lock()
            .map_err(|_| AppError::internal("profile counters poisoned"))?;
        let mut leaderboard = self
            .leaderboard
            .lock()
            .map_err(|_| AppError::internal("leaderboard poisoned"))?;

        let mut applied = false;
        if !markers.contains(&marker_image) {
            let attempts = format!("{}:attempts_total", profile_key(event.user_id));
            *profile.entry(attempts).or_insert(0) += 1;
            if event.is_correct {
                let correct = format!("{}:correct_total", profile_key(event.user_id));
                *profile.entry(correct).or_insert(0) += 1;
            }
            markers.insert(marker_image);
            applied = true;
        }
        if !markers.contains(&marker_rank) {
            let score = i64::from(event.is_correct);
            *leaderboard.entry(event.user_id).or_insert(0) += score;
            markers.insert(marker_rank);
            applied = true;
        }
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::consumer::{
        run_loop, ConsumerBackend, InMemoryRetryCounter, MessageEnvelope, RetryPolicy,
    };

    use super::*;

    fn sample_event() -> AttemptEvent {
        AttemptEvent {
            attempt_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            question_id: "mechanics-1".to_string(),
            is_correct: true,
            answer: "a".to_string(),
            submitted_at: Utc::now(),
        }
    }

    fn processor() -> (
        AttemptProcessor,
        Arc<InMemoryAttemptSink>,
        Arc<InMemoryIdempotencyStore>,
    ) {
        let sink = Arc::new(InMemoryAttemptSink::new());
        let store = Arc::new(InMemoryIdempotencyStore::new());
        let processor = AttemptProcessor::new(sink.clone(), store.clone(), 60);
        (processor, sink, store)
    }

    #[tokio::test]
    async fn duplicate_attempt_accumulates_once() {
        let (processor, sink, store) = processor();
        let event = sample_event();
        let payload = serde_json::to_vec(&event).expect("serialize");
        processor.process(&payload).await.expect("first delivery");
        processor
            .process(&payload)
            .await
            .expect("duplicate delivery");

        assert_eq!(sink.attempt_count(), 1);
        assert_eq!(store.marker_count(), 2);
        assert_eq!(store.profile_value(event.user_id, "attempts_total"), 1);
        assert_eq!(store.profile_value(event.user_id, "correct_total"), 1);
        assert_eq!(store.leaderboard_value(event.user_id), 1);
    }

    #[tokio::test]
    async fn malformed_json_is_rejected() {
        let (processor, sink, store) = processor();
        let error = processor.process(b"{not json").await.expect_err("invalid");
        assert_eq!(error.code(), "invalid_request");
        assert_eq!(sink.attempt_count(), 0);
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

        async fn seek(&self, partition: i32, offset: i64) -> Result<()> {
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
            let mut sought = self
                .last
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?;
            *sought = None;
            let _ = (partition, offset);
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
                topic: "attempts".to_string(),
                partition: 0,
                offset: 5,
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
