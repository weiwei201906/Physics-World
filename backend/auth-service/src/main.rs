use std::env;
use std::sync::Arc;

use auth_service::pg_store::PgAuthStore;
use auth_service::store::AuthStore;
use auth_service::tokens::KeyRing;
use auth_service::{router, AppState};
use common::clock::SystemClock;
use common::config::ConfigLoader;
use common::defaults;
use common::settings::{AuthSettings, DatabaseSettings};
use common::telemetry::{self, TelemetryConfig};
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;

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

    let database = DatabaseSettings::from_env("auth");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database.database_url)
        .await?;
    sqlx::migrate!("../migrations").run(&pool).await?;
    let redis_client = redis::Client::open(database.redis_url.clone())?;
    let redis = ConnectionManager::new(redis_client).await?;
    let store: Arc<dyn AuthStore> = Arc::new(PgAuthStore::new(pool, redis));

    let state = AppState::new(settings, store, Arc::new(keys), Arc::new(SystemClock));

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
