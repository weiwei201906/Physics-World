use std::env;
use std::sync::Arc;

use common::clock::SystemClock;
use common::config::ConfigLoader;
use common::defaults;
use common::settings::UserApiSettings;
use common::telemetry::{self, TelemetryConfig};
use user_api::store::Store;
use user_api::{router, App, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("user-api"))?;

    let config = ConfigLoader::from_env("user_api");
    let settings = UserApiSettings::from_config(&config);
    let missing = config.missing_keys();
    let invalid = config.invalid_keys();
    if !missing.is_empty() {
        tracing::warn!(count = missing.len(), keys = ?missing, "config keys missing");
    }
    if !invalid.is_empty() {
        tracing::warn!(count = invalid.len(), keys = ?invalid, "config keys invalid");
    }

    let state = AppState::new(Store::seeded(), settings, Arc::new(SystemClock));
    let app = App::new(state);

    let bind =
        env::var("USER_API_BIND_ADDR").unwrap_or_else(|_| defaults::USER_API_BIND_ADDR.to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "user-api listening");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
