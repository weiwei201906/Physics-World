use std::env;
use std::sync::Arc;

use common::telemetry::{self, TelemetryConfig};
use user_api_sse::hub::BroadcastHub;
use user_api_sse::redis_bridge;
use user_api_sse::settings::SseSettings;
use user_api_sse::{router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("user-api-sse"))?;

    let settings = SseSettings::from_env();
    let hub = Arc::new(BroadcastHub::new(user_api_sse::hub::DEFAULT_CAPACITY));
    tokio::spawn(redis_bridge::run(hub.clone(), settings.clone()));

    let bind = env::var("SSE_BIND_ADDR").unwrap_or_else(|_| settings.bind_addr());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "user-api-sse listening");
    axum::serve(listener, router(AppState::new(hub, &settings)))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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
    tracing::info!("shutdown signal received");
}
