//! rdkafka adapter with deferred offset commits.
//!
//! Offsets are buffered by partition and committed on the configured interval
//! (and once more during graceful shutdown), so a batch of processed messages
//! results in a single Kafka commit.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use common::consumer::{ConsumerBackend, MessageEnvelope};
use common::error::{AppError, Result};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use rdkafka::{Offset, TopicPartitionList};

#[derive(Debug, Clone)]
pub struct KafkaSettings {
    pub brokers: String,
    pub group_id: String,
    pub topic: String,
    pub dlq_topic: String,
    pub offset_reset_policy: String,
    pub delivery_timeout_ms: i64,
}

pub struct KafkaBackend {
    consumer: StreamConsumer,
    producer: FutureProducer,
    settings: KafkaSettings,
    pending: Mutex<HashMap<i32, i64>>,
}

impl KafkaBackend {
    pub fn new(settings: KafkaSettings) -> Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &settings.brokers)
            .set("group.id", &settings.group_id)
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", &settings.offset_reset_policy)
            .create()
            .map_err(kafka_error)?;
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &settings.brokers)
            .create()
            .map_err(kafka_error)?;
        let topics = [settings.topic.as_str()];
        consumer.subscribe(&topics).map_err(kafka_error)?;
        Ok(Self {
            consumer,
            producer,
            settings,
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// Commits the highest buffered offset for every partition. The stored
    /// value is the last processed offset, so Kafka receives offset + 1.
    pub async fn flush_commits(&self) -> Result<()> {
        let drained = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| AppError::internal("pending offsets poisoned"))?;
            std::mem::take(&mut *pending)
        };
        if drained.is_empty() {
            return Ok(());
        }
        let mut list = TopicPartitionList::new();
        for (partition, offset) in drained {
            list.add_partition_offset(
                &self.settings.topic,
                partition,
                Offset::Offset(offset.saturating_add(1)),
            )
            .map_err(kafka_error)?;
        }
        self.consumer
            .commit(&list, CommitMode::Sync)
            .map_err(kafka_error)
    }
}

#[async_trait]
impl ConsumerBackend for KafkaBackend {
    async fn recv(&self) -> Result<Option<MessageEnvelope>> {
        match self.consumer.recv().await {
            Ok(message) => Ok(Some(MessageEnvelope {
                topic: message.topic().to_string(),
                partition: message.partition(),
                offset: message.offset(),
                key: message.key().map(|bytes| bytes.to_vec()),
                payload: message
                    .payload()
                    .map(|bytes| bytes.to_vec())
                    .unwrap_or_default(),
            })),
            Err(KafkaError::PartitionEOF(_)) => Ok(None),
            Err(error) => Err(kafka_error(error)),
        }
    }

    async fn commit(&self, partition: i32, offset: i64) -> Result<()> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| AppError::internal("pending offsets poisoned"))?;
        pending
            .entry(partition)
            .and_modify(|current| *current = (*current).max(offset))
            .or_insert(offset);
        Ok(())
    }

    async fn seek(&self, partition: i32, offset: i64) -> Result<()> {
        let timeout = Duration::from_millis(self.settings.delivery_timeout_ms.max(0) as u64);
        self.consumer
            .seek(
                &self.settings.topic,
                partition,
                Offset::Offset(offset),
                Timeout::After(timeout),
            )
            .map_err(kafka_error)
    }

    async fn publish_dlq(&self, envelope: &MessageEnvelope, reason: &str) -> Result<()> {
        let body = serde_json::json!({
            "source_topic": envelope.topic,
            "partition": envelope.partition,
            "offset": envelope.offset,
            "reason": reason,
            "payload": &envelope.payload,
        });
        let payload = serde_json::to_vec(&body)
            .map_err(|error| AppError::internal(format!("serialize DLQ payload: {error}")))?;
        let key = envelope.key.as_deref().unwrap_or_default();
        let record = FutureRecord::to(&self.settings.dlq_topic)
            .payload(payload.as_slice())
            .key(key);
        let timeout = Duration::from_millis(self.settings.delivery_timeout_ms.max(0) as u64);
        self.producer
            .send(record, Timeout::After(timeout))
            .await
            .map_err(|(error, _message)| kafka_error(error))?;
        Ok(())
    }
}

fn kafka_error(error: KafkaError) -> AppError {
    AppError::upstream("kafka", error.to_string())
}
