//! Kafka consumer abstractions: manual commit, bounded retries, DLQ and
//! idempotent accumulation. The concrete rdkafka adapter lives in the
//! `kafka` feature module.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::error::{AppError, Result};

#[cfg(feature = "kafka")]
pub mod kafka;

/// Owned Kafka message. Borrowed rdkafka messages are converted immediately
/// so the consumer can commit or seek without lifetime juggling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEnvelope {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<Vec<u8>>,
    pub payload: Vec<u8>,
}

#[async_trait]
pub trait ConsumerBackend: Send + Sync {
    async fn recv(&self) -> Result<Option<MessageEnvelope>>;
    async fn commit(&self, partition: i32, offset: i64) -> Result<()>;
    async fn seek(&self, partition: i32, offset: i64) -> Result<()>;
    async fn publish_dlq(&self, envelope: &MessageEnvelope, reason: &str) -> Result<()>;
}

#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle(&self, payload: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    /// Atomically claims a marker key. Returns `true` only for the first claim.
    async fn claim(&self, key: &str) -> Result<bool>;
}

#[derive(Debug, Default)]
pub struct InMemoryIdempotencyStore {
    seen: Mutex<HashMap<String, ()>>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.seen.lock().map(|seen| seen.len()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl IdempotencyStore for InMemoryIdempotencyStore {
    async fn claim(&self, key: &str) -> Result<bool> {
        let mut seen = self
            .seen
            .lock()
            .map_err(|_| AppError::internal("idempotency store poisoned"))?;
        if seen.contains_key(key) {
            return Ok(false);
        }
        seen.insert(key.to_string(), ());
        Ok(true)
    }
}

/// Models the Lua atomic script used by attempt and battle consumers:
/// check marker, increment counter, write marker in one step.
#[async_trait]
pub trait AtomicAccumulator: Send + Sync {
    async fn accumulate_once(&self, marker: &str, counter: &str, delta: i64) -> Result<bool>;
    async fn value(&self, counter: &str) -> Result<i64>;
}

#[derive(Debug, Default)]
pub struct InMemoryAtomicAccumulator {
    markers: Mutex<HashMap<String, ()>>,
    counters: Mutex<HashMap<String, i64>>,
}

impl InMemoryAtomicAccumulator {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AtomicAccumulator for InMemoryAtomicAccumulator {
    async fn accumulate_once(&self, marker: &str, counter: &str, delta: i64) -> Result<bool> {
        let mut markers = self
            .markers
            .lock()
            .map_err(|_| AppError::internal("accumulator markers poisoned"))?;
        if markers.contains_key(marker) {
            return Ok(false);
        }
        let mut counters = self
            .counters
            .lock()
            .map_err(|_| AppError::internal("accumulator counters poisoned"))?;
        *counters.entry(counter.to_string()).or_insert(0) += delta;
        markers.insert(marker.to_string(), ());
        Ok(true)
    }

    async fn value(&self, counter: &str) -> Result<i64> {
        let counters = self
            .counters
            .lock()
            .map_err(|_| AppError::internal("accumulator counters poisoned"))?;
        Ok(counters.get(counter).copied().unwrap_or_default())
    }
}

#[async_trait]
pub trait RetryCounter: Send + Sync {
    async fn increment(&self, key: &str) -> Result<u32>;
    async fn attempts(&self, key: &str) -> Result<u32>;
    async fn clear(&self, key: &str) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryRetryCounter {
    attempts: Mutex<HashMap<String, u32>>,
}

impl InMemoryRetryCounter {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RetryCounter for InMemoryRetryCounter {
    async fn increment(&self, key: &str) -> Result<u32> {
        let mut attempts = self
            .attempts
            .lock()
            .map_err(|_| AppError::internal("retry counter poisoned"))?;
        let entry = attempts.entry(key.to_string()).or_insert(0);
        *entry = entry.saturating_add(1);
        Ok(*entry)
    }

    async fn attempts(&self, key: &str) -> Result<u32> {
        let attempts = self
            .attempts
            .lock()
            .map_err(|_| AppError::internal("retry counter poisoned"))?;
        Ok(attempts.get(key).copied().unwrap_or_default())
    }

    async fn clear(&self, key: &str) -> Result<()> {
        let mut attempts = self
            .attempts
            .lock()
            .map_err(|_| AppError::internal("retry counter poisoned"))?;
        attempts.remove(key);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    Retry,
    DeadLetter,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: Duration,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, backoff: Duration) -> Self {
        Self {
            max_retries,
            backoff,
        }
    }

    pub fn decide(&self, failures: u32) -> RetryDecision {
        if failures > self.max_retries {
            RetryDecision::DeadLetter
        } else {
            RetryDecision::Retry
        }
    }
}

pub fn retry_key(topic: &str, partition: i32, offset: i64) -> String {
    format!("retry:{topic}:{partition}:{offset}")
}

/// Manual-commit consume loop. A handler error increments the retry counter;
/// after `max_retries` failures the message goes to the DLQ and the offset is
/// committed so the consumer does not stall.
pub async fn run_loop(
    backend: Arc<dyn ConsumerBackend>,
    handler: Arc<dyn MessageHandler>,
    policy: RetryPolicy,
    retry_counter: Arc<dyn RetryCounter>,
) -> Result<()> {
    loop {
        let envelope = match backend.recv().await? {
            Some(envelope) => envelope,
            None => return Ok(()),
        };
        match handler.handle(&envelope.payload).await {
            Ok(()) => {
                backend.commit(envelope.partition, envelope.offset).await?;
                retry_counter
                    .clear(&retry_key(
                        &envelope.topic,
                        envelope.partition,
                        envelope.offset,
                    ))
                    .await?;
            }
            Err(error) => {
                let key = retry_key(&envelope.topic, envelope.partition, envelope.offset);
                let failures = retry_counter.increment(&key).await?;
                match policy.decide(failures) {
                    RetryDecision::Retry => {
                        tracing::warn!(
                            topic = %envelope.topic,
                            partition = envelope.partition,
                            offset = envelope.offset,
                            failures,
                            error = %error,
                            "consumer handler failed, retrying"
                        );
                        tokio::time::sleep(policy.backoff).await;
                        backend.seek(envelope.partition, envelope.offset).await?;
                    }
                    RetryDecision::DeadLetter => {
                        tracing::error!(
                            topic = %envelope.topic,
                            partition = envelope.partition,
                            offset = envelope.offset,
                            error = %error,
                            "consumer retries exhausted, publishing to DLQ"
                        );
                        backend.publish_dlq(&envelope, &error.to_string()).await?;
                        backend.commit(envelope.partition, envelope.offset).await?;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idempotency_store_claims_only_once() {
        let store = InMemoryIdempotencyStore::new();
        assert!(store.claim("attempt:1").await.expect("first claim"));
        assert!(!store.claim("attempt:1").await.expect("second claim"));
        assert!(store.claim("attempt:2").await.expect("other key"));
    }

    #[tokio::test]
    async fn accumulator_applies_delta_once_per_marker() {
        let accumulator = InMemoryAtomicAccumulator::new();
        assert!(accumulator
            .accumulate_once("attempt:image:1", "chapter:mechanics", 1)
            .await
            .expect("first apply"));
        assert!(!accumulator
            .accumulate_once("attempt:image:1", "chapter:mechanics", 1)
            .await
            .expect("duplicate apply"));
        assert_eq!(
            accumulator.value("chapter:mechanics").await.expect("value"),
            1
        );
    }

    #[tokio::test]
    async fn accumulator_keeps_markers_and_counters_consistent() {
        let accumulator = InMemoryAtomicAccumulator::new();
        for attempt in ["a1", "a2", "a1"] {
            accumulator
                .accumulate_once(&format!("marker:{attempt}"), "score", 10)
                .await
                .expect("accumulate");
        }
        assert_eq!(accumulator.value("score").await.expect("value"), 20);
    }

    #[test]
    fn retry_policy_honors_max_retries_boundary() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1));
        assert_eq!(policy.decide(1), RetryDecision::Retry);
        assert_eq!(policy.decide(3), RetryDecision::Retry);
        assert_eq!(policy.decide(4), RetryDecision::DeadLetter);
        let policy = RetryPolicy::new(0, Duration::from_millis(1));
        assert_eq!(policy.decide(1), RetryDecision::DeadLetter);
    }

    #[test]
    fn retry_key_matches_redis_layout() {
        assert_eq!(retry_key("attempts", 3, 42), "retry:attempts:3:42");
    }

    #[derive(Default)]
    struct FakeBackend {
        messages: Mutex<Vec<MessageEnvelope>>,
        last: Mutex<Option<MessageEnvelope>>,
        committed: Mutex<Vec<(i32, i64)>>,
        sought: Mutex<Vec<(i32, i64)>>,
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
            let mut sought = self
                .sought
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?;
            sought.push((partition, offset));
            let last = self
                .last
                .lock()
                .map_err(|_| AppError::internal("fake backend poisoned"))?
                .clone();
            if let Some(message) = last {
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
            dlq.push(format!("{}:{}", envelope.offset, reason));
            Ok(())
        }
    }

    struct FailingHandler;

    #[async_trait]
    impl MessageHandler for FailingHandler {
        async fn handle(&self, _payload: &[u8]) -> Result<()> {
            Err(AppError::invalid("boom"))
        }
    }

    #[tokio::test]
    async fn loop_retries_then_commits_after_dlq() {
        let backend = Arc::new(FakeBackend::default());
        if let Ok(mut messages) = backend.messages.lock() {
            messages.push(MessageEnvelope {
                topic: "attempts".to_string(),
                partition: 0,
                offset: 7,
                key: None,
                payload: b"bad".to_vec(),
            });
        }
        let retry_counter = Arc::new(InMemoryRetryCounter::new());
        run_loop(
            backend.clone(),
            Arc::new(FailingHandler),
            RetryPolicy::new(2, Duration::from_millis(1)),
            retry_counter,
        )
        .await
        .expect("loop completes");
        let committed = backend
            .committed
            .lock()
            .map(|committed| committed.clone())
            .unwrap_or_default();
        assert_eq!(committed, vec![(0, 7)]);
        let sought = backend
            .sought
            .lock()
            .map(|sought| sought.clone())
            .unwrap_or_default();
        assert_eq!(sought.len(), 2);
        let dlq = backend
            .dlq
            .lock()
            .map(|dlq| dlq.clone())
            .unwrap_or_default();
        assert_eq!(dlq.len(), 1);
    }
}
