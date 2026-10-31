use std::sync::Arc;

use battle_service::engine::{BattleEngine, QuestionSpec};
use battle_service::settings::Settings;
use battle_service::store::{InMemoryBattleStore, InMemoryReconciler};
use battle_service::ws::{router, AppState, InMemoryTokenValidator};
use common::clock::{Clock, SystemClock};
use common::telemetry::{self, TelemetryConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("battle-service"))?;

    let settings = Settings::from_env();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let catalog = demo_questions(settings.questions_per_room);
    let engine = BattleEngine::new(settings.clone(), clock.clone(), catalog);
    let store = Arc::new(InMemoryBattleStore::new(
        engine,
        Arc::new(InMemoryReconciler::default()),
    ));
    // TODO: replace with JWT verification against auth-service JWKS plus the
    // Redis revoked:{jti} blacklist. The local validator accepts no tokens.
    let tokens = Arc::new(InMemoryTokenValidator::default());
    let state = AppState::new(store, settings.clone(), clock, tokens);

    let bind = settings.bind_addr();
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "battle-service listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

// TODO: replace the demo catalog with the question-bank gRPC client.
fn demo_questions(count: u32) -> Vec<QuestionSpec> {
    const OPTIONS: [&str; 4] = ["a", "b", "c", "d"];
    (0..count.max(1))
        .map(|index| QuestionSpec {
            id: format!("demo-{}", index + 1),
            correct_option: OPTIONS[index as usize % OPTIONS.len()].to_string(),
        })
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
