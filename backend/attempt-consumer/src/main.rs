//! attempt-consumer entrypoint: Kafka wiring, PostgreSQL/Redis stores and
//! graceful shutdown. Message processing lives in the library crate.

mod kafka;
mod store;

use std::env;
use std::sync::Arc;
use std::time::Duration;

use common::config::ConfigLoader;
use common::consumer::{
    run_loop, ConsumerBackend, InMemoryRetryCounter, MessageHandler, RetryCounter, RetryPolicy,
};
use common::settings::{ConsumerSettings, DatabaseSettings};
use common::telemetry::{self, TelemetryConfig};
use kafka::{KafkaBackend, KafkaSettings};
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;
use store::{PgAttemptSink, RedisAttemptStore};

use attempt_consumer::{AttemptProcessor, LEADERBOARD_KEY};

const SERVICE: &str = "attempt-consumer";
const TOPIC: &str = "attempts";
const DLQ_TOPIC: &str = "attempts_dlq";
const DEFAULT_KAFKA_BROKERS: &str = "127.0.0.1:9092";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env(SERVICE))?;

    let config = ConfigLoader::from_env("consumers");
    let settings = ConsumerSettings::from_config(&config);
    log_config_issues(&config);

    let database = DatabaseSettings::from_env("attempt_consumer");
    let pool = PgPoolOptions::new().connect(&database.database_url).await?;
    let redis_client = redis::Client::open(database.redis_url.clone())?;
    let redis = ConnectionManager::new(redis_client).await?;

    let retry_backoff = Duration::from_millis(settings.retry_backoff_ms.max(0) as u64);
    let commit_interval = Duration::from_millis(settings.commit_interval_ms.max(1) as u64);
    let processor = Arc::new(AttemptProcessor::new(
        Arc::new(PgAttemptSink::new(pool)),
        Arc::new(RedisAttemptStore::new(redis, LEADERBOARD_KEY)),
        settings.idempotency_ttl_sec,
    ));

    let brokers = env::var("KAFKA_BROKERS").unwrap_or_else(|_| DEFAULT_KAFKA_BROKERS.to_string());
    let group_id = env::var("KAFKA_GROUP_ID").unwrap_or_else(|_| SERVICE.to_string());
    let backend = Arc::new(KafkaBackend::new(KafkaSettings {
        brokers: brokers.clone(),
        group_id: group_id.clone(),
        topic: TOPIC.to_string(),
        dlq_topic: DLQ_TOPIC.to_string(),
        offset_reset_policy: settings.offset_reset_policy.clone(),
        delivery_timeout_ms: settings.delivery_timeout_ms,
    })?);

    let flush_backend = backend.clone();
    let flush_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(commit_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(error) = flush_backend.flush_commits().await {
                tracing::error!(error = %error, "commit flush failed");
            }
        }
    });

    let consumer: Arc<dyn ConsumerBackend> = backend.clone();
    let handler: Arc<dyn MessageHandler> = processor;
    let retry_counter: Arc<dyn RetryCounter> = Arc::new(InMemoryRetryCounter::new());
    let policy = RetryPolicy::new(settings.max_retries, retry_backoff);
    let run = run_loop(consumer, handler, policy, retry_counter);

    tracing::info!(
        topic = TOPIC,
        group_id = %group_id,
        brokers = %brokers,
        "attempt-consumer started"
    );

    tokio::select! {
        result = run => {
            result?;
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received");
        }
    }

    flush_task.abort();
    backend.flush_commits().await?;
    Ok(())
}

fn log_config_issues(config: &ConfigLoader) {
    let missing = config.missing_keys();
    let invalid = config.invalid_keys();
    if !missing.is_empty() {
        tracing::warn!(count = missing.len(), keys = ?missing, "config keys missing");
    }
    if !invalid.is_empty() {
        tracing::warn!(count = invalid.len(), keys = ?invalid, "config keys invalid");
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
