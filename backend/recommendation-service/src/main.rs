use std::sync::Arc;

use common::clock::SystemClock;
use common::telemetry::{self, TelemetryConfig};
use recommendation_service::engine::RecommendationEngine;
use recommendation_service::settings::Settings;
use recommendation_service::store::{InMemoryProfileSource, PgProfileSource, ProfileSource};
use recommendation_service::{router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("recommendation-service"))?;

    let settings = Settings::from_env();

    let dims = settings.chapter_dims.clone();
    let engine = Arc::new(RecommendationEngine::from_dims(
        dims.clone(),
        settings.min_score,
    ));
    let source = profile_source(dims).await;
    let state = AppState::new(
        source,
        engine,
        Arc::new(settings.clone()),
        Arc::new(SystemClock),
    );

    let bind = settings.bind_addr();
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "recommendation-service listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

// Reads attempt statistics and the baseline vector from PostgreSQL when
// DATABASE_URL is configured; otherwise keeps the in-memory fallback.
async fn profile_source(dims: Vec<String>) -> Arc<dyn ProfileSource> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => match PgProfileSource::connect(&url, dims.clone()).await {
            Ok(source) => Arc::new(source),
            Err(error) => {
                tracing::warn!(error = %error, "profile store unavailable; using in-memory");
                Arc::new(InMemoryProfileSource::new(dims))
            }
        },
        _ => Arc::new(InMemoryProfileSource::new(dims)),
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
