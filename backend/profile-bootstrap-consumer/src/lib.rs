//! Pure user registration processing.
//!
//! `profile-bootstrap-consumer` is the only writer of `app.user_profiles`.
//! Kafka and PostgreSQL adapters are injected through [ProfileSink], so the
//! unit tests run against an in-memory fake and need no broker.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use common::consumer::MessageHandler;
use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Event payload produced by auth-service or by user-api's lazy profile
/// compensation on topic `user_registered`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserRegisteredEvent {
    pub user_id: Uuid,
    pub email: String,
    pub nickname: String,
}

impl UserRegisteredEvent {
    pub fn from_payload(payload: &[u8]) -> Result<Self> {
        serde_json::from_slice(payload)
            .map_err(|error| AppError::invalid(format!("malformed user_registered event: {error}")))
    }
}

/// Stores the profile row. PostgreSQL de-duplicates by `user_id`, so a replayed
/// registration never overwrites an existing profile.
#[async_trait]
pub trait ProfileSink: Send + Sync {
    async fn insert_profile(&self, event: &UserRegisteredEvent) -> Result<()>;
}

pub struct ProfileProcessor {
    sink: Arc<dyn ProfileSink>,
}

impl ProfileProcessor {
    pub fn new(sink: Arc<dyn ProfileSink>) -> Self {
        Self { sink }
    }

    pub async fn process(&self, payload: &[u8]) -> Result<()> {
        let event = UserRegisteredEvent::from_payload(payload)?;
        self.sink.insert_profile(&event).await
    }
}

#[async_trait]
impl MessageHandler for ProfileProcessor {
    async fn handle(&self, payload: &[u8]) -> Result<()> {
        self.process(payload).await
    }
}

/// In-memory stand-in for the PostgreSQL sink, mirroring
/// `ON CONFLICT (user_id) DO NOTHING`.
#[derive(Debug, Default)]
pub struct InMemoryProfileSink {
    profiles: Mutex<HashMap<Uuid, (String, String)>>,
}

impl InMemoryProfileSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn profile_count(&self) -> usize {
        self.profiles
            .lock()
            .map(|set| set.len())
            .unwrap_or_default()
    }

    pub fn profile(&self, user_id: Uuid) -> Option<(String, String)> {
        self.profiles
            .lock()
            .map(|profiles| profiles.get(&user_id).cloned())
            .unwrap_or_default()
    }
}

#[async_trait]
impl ProfileSink for InMemoryProfileSink {
    async fn insert_profile(&self, event: &UserRegisteredEvent) -> Result<()> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| AppError::internal("profile sink poisoned"))?;
        profiles
            .entry(event.user_id)
            .or_insert_with(|| (event.email.clone(), event.nickname.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::consumer::{
        run_loop, ConsumerBackend, InMemoryRetryCounter, MessageEnvelope, RetryPolicy,
    };

    use super::*;

    fn sample_event() -> UserRegisteredEvent {
        UserRegisteredEvent {
            user_id: Uuid::new_v4(),
            email: "student@example.com".to_string(),
            nickname: "student".to_string(),
        }
    }

    fn processor() -> (ProfileProcessor, Arc<InMemoryProfileSink>) {
        let sink = Arc::new(InMemoryProfileSink::new());
        let processor = ProfileProcessor::new(sink.clone());
        (processor, sink)
    }

    #[tokio::test]
    async fn duplicate_registration_creates_one_profile() {
        let (processor, sink) = processor();
        let event = sample_event();
        let payload = serde_json::to_vec(&event).expect("serialize");
        processor.process(&payload).await.expect("first delivery");
        processor
            .process(&payload)
            .await
            .expect("duplicate delivery");

        assert_eq!(sink.profile_count(), 1);
        assert_eq!(
            sink.profile(event.user_id),
            Some((event.email.clone(), event.nickname.clone()))
        );
    }

    #[tokio::test]
    async fn malformed_json_is_rejected() {
        let (processor, sink) = processor();
        let error = processor.process(b"{not json").await.expect_err("invalid");
        assert_eq!(error.code(), "invalid_request");
        assert_eq!(sink.profile_count(), 0);
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
                topic: "user_registered".to_string(),
                partition: 0,
                offset: 3,
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
        let dlq = backend.dlq.lock().expect("dlq").clone();
        assert!(dlq[0].starts_with("3:"));
    }
}
