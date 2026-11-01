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

use crate::config::{QbankConfig, RustfsSettings};
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

    seed_storage(&config).await;
    match import_bundle(&config).await {
        Ok(Some(bundle)) => {
            let count = store.import(&bundle, &config.media_base).await?;
            tracing::info!(
                questions = count,
                chapters = bundle.chapters.len(),
                "question bundle synced to postgres"
            );
        }
        Ok(None) => tracing::warn!("no question bundle available; serving existing rows"),
        Err(error) => tracing::warn!(error = %error, "question bundle sync skipped"),
    }

    let bind_addr = config.bind_addr.parse()?;
    tracing::info!(%bind_addr, "question-bank listening");
    tonic::transport::Server::builder()
        .add_service(QuestionBankServer::new(QuestionBankService::new(store)))
        .serve(bind_addr)
        .await?;
    Ok(())
}

/// Seeds RustFS with the bundle and images shipped alongside the service.
///
/// Object storage is the source of truth; the local copies only bootstrap it.
async fn seed_storage(config: &QbankConfig) {
    let Some(settings) = &config.rustfs else {
        tracing::warn!("no rustfs endpoint configured; cannot seed question bundle");
        return;
    };
    publish_bundle(settings, config).await;
    publish_media(settings, config).await;
}

/// Uploads the shipped JSON bundle so RustFS always holds the canonical
/// document that PostgreSQL is then synced from.
async fn publish_bundle(settings: &RustfsSettings, config: &QbankConfig) {
    let Some(path) = &config.bundle_path else {
        return;
    };
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes::Bytes::from(bytes),
        Err(error) => {
            tracing::warn!(error = %error, path = %path.display(), "question bundle unreadable");
            return;
        }
    };
    if let Err(error) =
        rustfs::ensure_bucket(settings, &settings.bucket, Duration::from_secs(10)).await
    {
        tracing::warn!(error = %error, bucket = %settings.bucket, "rustfs bundle bucket setup failed");
        return;
    }
    match rustfs::put_object(
        settings,
        &settings.bucket,
        &settings.object,
        bytes,
        "application/json",
        Duration::from_secs(30),
    )
    .await
    {
        Ok(()) => tracing::info!(
            bucket = %settings.bucket,
            object = %settings.object,
            "question bundle published"
        ),
        Err(error) => tracing::warn!(error = %error, "question bundle upload failed"),
    }
}

/// Fetches the bundle from RustFS. Object storage is the only source; the
/// service never reads the local file into PostgreSQL directly.
async fn import_bundle(
    config: &QbankConfig,
) -> Result<Option<bundle::Bundle>, Box<dyn std::error::Error>> {
    let Some(settings) = &config.rustfs else {
        return Ok(None);
    };
    let raw = rustfs::fetch_json(settings, Duration::from_secs(30)).await?;
    Ok(Some(bundle::parse(&raw)?))
}

/// Publishes locally bundled question images to RustFS so the gateway streams
/// `/media/*` from object storage instead of the frontend build.
///
/// Failures are logged, never fatal: a stale media set still lets the rest of
/// the question bank import and serve questions.
async fn publish_media(settings: &RustfsSettings, config: &QbankConfig) {
    let Some(dir) = &config.media_dir else {
        return;
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(error = %error, dir = %dir.display(), "question media directory unreadable");
            return;
        }
    };
    if let Err(error) =
        rustfs::ensure_public_bucket(settings, &config.media_bucket, Duration::from_secs(10)).await
    {
        tracing::warn!(error = %error, bucket = %config.media_bucket, "rustfs media bucket setup failed");
        return;
    }

    let mut uploaded = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes::Bytes::from(bytes),
            Err(error) => {
                tracing::warn!(error = %error, file, "question media file unreadable");
                continue;
            }
        };
        let key = format!("media/{file}");
        match rustfs::put_object(
            settings,
            &config.media_bucket,
            &key,
            bytes,
            media_content_type(file),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(()) => uploaded += 1,
            Err(error) => tracing::warn!(error = %error, file, "question media upload failed"),
        }
    }
    tracing::info!(
        uploaded,
        bucket = %config.media_bucket,
        "question media published"
    );
}

fn media_content_type(file: &str) -> &'static str {
    match file
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}
