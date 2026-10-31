use std::sync::Arc;

use common::clock::SystemClock;
use common::telemetry::{self, TelemetryConfig};
use recommendation_service::engine::RecommendationEngine;
use recommendation_service::settings::Settings;
use recommendation_service::store::InMemoryProfileSource;
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
    let source = Arc::new(InMemoryProfileSource::new(dims));
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

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
