//! rdkafka adapter implementing [`crate::consumer::ConsumerBackend`].

use std::time::Duration;

use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use rdkafka::{Offset, TopicPartitionList};

use crate::consumer::{ConsumerBackend, MessageEnvelope};
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct KafkaBackendSettings {
    pub brokers: String,
    pub group_id: String,
    pub topic: String,
    pub dlq_topic: String,
    pub offset_reset_policy: String,
    pub delivery_timeout_ms: i64,
}

pub struct RdkafkaBackend {
    consumer: StreamConsumer,
    producer: FutureProducer,
    settings: KafkaBackendSettings,
}

impl RdkafkaBackend {
    pub fn new(settings: KafkaBackendSettings) -> Result<Self> {
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
        })
    }

    fn partition_list(&self, partition: i32, offset: i64) -> Result<TopicPartitionList> {
        let mut list = TopicPartitionList::new();
        list.add_partition_offset(&self.settings.topic, partition, Offset::Offset(offset))
            .map_err(kafka_error)?;
        Ok(list)
    }
}

#[async_trait]
impl ConsumerBackend for RdkafkaBackend {
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
        let list = self.partition_list(partition, offset)?;
        self.consumer
            .commit(&list, CommitMode::Async)
            .map_err(kafka_error)
    }

    async fn seek(&self, partition: i32, offset: i64) -> Result<()> {
        let list = self.partition_list(partition, offset)?;
        let timeout = Duration::from_millis(self.settings.delivery_timeout_ms.max(0) as u64);
        self.consumer.seek(&list, timeout).map_err(kafka_error)
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
            .map_err(|error| AppError::internal(list_serialization_error(error)))?;
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

fn list_serialization_error(error: serde_json::Error) -> String {
    format!("failed to serialize DLQ payload: {error}")
}
