use std::env;
use std::sync::Arc;

use auth_service::store::AuthStore;
use auth_service::tokens::KeyRing;
use auth_service::{router, AppState};
use common::clock::SystemClock;
use common::config::ConfigLoader;
use common::defaults;
use common::settings::AuthSettings;
use common::telemetry::{self, TelemetryConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("auth-service"))?;

    let config = ConfigLoader::from_env("auth");
    let settings = AuthSettings::from_config(&config);
    let missing = config.missing_keys();
    let invalid = config.invalid_keys();
    if !missing.is_empty() {
        tracing::warn!(count = missing.len(), keys = ?missing, "config keys missing");
    }
    if !invalid.is_empty() {
        tracing::warn!(count = invalid.len(), keys = ?invalid, "config keys invalid");
    }

    let keys = match env::var("AUTH_JWT_PRIVATE_PEM") {
        Ok(pem) if !pem.trim().is_empty() => {
            KeyRing::from_pkcs8_pem(&pem, defaults::AUTH_JWT_KEY_ID)?
        }
        _ => {
            tracing::warn!("AUTH_JWT_PRIVATE_PEM is not set, generating an ephemeral key");
            KeyRing::generate(defaults::AUTH_JWT_KEY_ID)?
        }
    };

    let state = AppState::new(
        settings,
        Arc::new(AuthStore::new()),
        Arc::new(keys),
        Arc::new(SystemClock),
    );

    let bind = env::var("AUTH_BIND_ADDR").unwrap_or_else(|_| defaults::AUTH_BIND_ADDR.to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "auth-service listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
