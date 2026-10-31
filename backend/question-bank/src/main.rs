//! Question bank service: imports the JSON bundle from RustFS into
//! PostgreSQL, caches questions in Redis and serves the gRPC contract.

mod bundle;
mod config;
mod rustfs;
mod service;
mod store;

use std::sync::Arc;
use std::time::Duration;

use common::telemetry::{self, TelemetryConfig};
use qbank_client::qbank::question_bank_server::QuestionBankServer;
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;

use crate::config::QbankConfig;
use crate::service::QuestionBankService;
use crate::store::QuestionStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("question-bank"))?;

    let config = QbankConfig::from_env();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    let redis_client = redis::Client::open(config.redis_url.clone())?;
    let redis = ConnectionManager::new(redis_client).await?;
    let store = Arc::new(QuestionStore::new(pool, redis, config.cache_ttl_sec));

    match import_bundle(&config).await {
        Ok(Some(bundle)) => {
            let count = store.import(&bundle, &config.media_base).await?;
            tracing::info!(
                questions = count,
                chapters = bundle.chapters.len(),
                "question bundle imported"
            );
        }
        Ok(None) => tracing::warn!("no question bundle configured; serving existing rows"),
        Err(error) => tracing::warn!(error = %error, "question bundle import skipped"),
    }

    let bind_addr = config.bind_addr.parse()?;
    tracing::info!(%bind_addr, "question-bank listening");
    tonic::transport::Server::builder()
        .add_service(QuestionBankServer::new(QuestionBankService::new(store)))
        .serve(bind_addr)
        .await?;
    Ok(())
}

/// Prefers RustFS; falls back to the local bundle when object storage is
/// unreachable or not configured.
async fn import_bundle(
    config: &QbankConfig,
) -> Result<Option<bundle::Bundle>, Box<dyn std::error::Error>> {
    if let Some(settings) = &config.rustfs {
        match rustfs::fetch_json(settings, Duration::from_secs(30)).await {
            Ok(raw) => return Ok(Some(bundle::parse(&raw)?)),
            Err(error) => tracing::warn!(error = %error, "rustfs bundle fetch failed"),
        }
    }
    match &config.bundle_path {
        Some(path) => Ok(Some(bundle::load(path)?)),
        None => Ok(None),
    }
}
