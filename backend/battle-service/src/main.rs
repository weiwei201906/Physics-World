use std::sync::Arc;

use battle_service::engine::{BattleEngine, BattleOption, QuestionSpec};
use battle_service::settings::Settings;
use battle_service::store::{InMemoryBattleStore, InMemoryReconciler};
use battle_service::validator::HttpTokenValidator;
use battle_service::vector::{PgVectorSource, VectorSource, ZeroVectorSource};
use battle_service::ws::{router, AppState, TokenValidator};
use common::clock::{Clock, SystemClock};
use common::telemetry::{self, TelemetryConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init(&TelemetryConfig::from_env("battle-service"))?;

    let settings = Settings::from_env();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let catalog = load_question_catalog(settings.questions_per_room).await;
    let engine = BattleEngine::new(settings.clone(), clock.clone(), catalog);
    let store = Arc::new(InMemoryBattleStore::new(
        engine,
        Arc::new(InMemoryReconciler::default()),
    ));
    // WebSocket clients pass the access token in the query string because
    // browsers cannot set handshake headers; auth-service verifies it.
    let tokens: Arc<dyn TokenValidator> = Arc::new(HttpTokenValidator::new(&auth_base_url()));
    let vectors = vector_source().await;
    let state = AppState::new(store, settings.clone(), clock, tokens).with_vector_source(vectors);

    let bind = settings.bind_addr();
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "battle-service listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

// auth-service base URL used for WebSocket token verification.
fn auth_base_url() -> String {
    std::env::var("AUTH_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8081".to_string())
}

// Matchmaking uses the PG error vectors when DATABASE_URL is configured and
// falls back to FIFO pairing otherwise.
async fn vector_source() -> Arc<dyn VectorSource> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => match PgVectorSource::connect(&url).await {
            Ok(source) => Arc::new(source),
            Err(error) => {
                tracing::warn!(error = %error, "vector store unavailable; using FIFO matchmaking");
                Arc::new(ZeroVectorSource)
            }
        },
        _ => Arc::new(ZeroVectorSource),
    }
}

// Reads real questions from PostgreSQL when DATABASE_URL is configured.
// Rooms need stems and options, which the demo catalog cannot provide.
async fn load_question_catalog(count: u32) -> Vec<QuestionSpec> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => match load_catalog_from_pg(&url, count).await {
            Some(catalog) => catalog,
            None => {
                tracing::warn!("question catalog unavailable; using demo questions");
                demo_questions(count)
            }
        },
        _ => demo_questions(count),
    }
}

async fn load_catalog_from_pg(database_url: &str, count: u32) -> Option<Vec<QuestionSpec>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await
        .ok()?;
    let rows: Vec<(String, String, serde_json::Value, String, String)> = sqlx::query_as(
        "SELECT id, stem, options, answer_option_id, COALESCE(explanation, '') \
         FROM question.questions WHERE deleted_at IS NULL \
         ORDER BY random() LIMIT $1",
    )
    .bind(i64::from(count.max(1)))
    .fetch_all(&pool)
    .await
    .ok()?;
    let catalog: Vec<QuestionSpec> = rows
        .into_iter()
        .filter_map(|(id, stem, options, correct_option, explanation)| {
            let options = parse_options(&options);
            if options.is_empty() {
                return None;
            }
            Some(QuestionSpec::new(
                id,
                correct_option,
                stem,
                options,
                explanation,
            ))
        })
        .collect();
    if catalog.is_empty() {
        None
    } else {
        Some(catalog)
    }
}

fn parse_options(value: &serde_json::Value) -> Vec<BattleOption> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?;
                    let text = item
                        .get("text")
                        .or_else(|| item.get("label"))
                        .and_then(serde_json::Value::as_str)?;
                    Some(BattleOption {
                        id: id.to_string(),
                        text: text.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// TODO: replace the demo catalog with the question-bank gRPC client.
fn demo_questions(count: u32) -> Vec<QuestionSpec> {
    const OPTIONS: [&str; 4] = ["a", "b", "c", "d"];
    (0..count.max(1))
        .map(|index| {
            QuestionSpec::demo(
                format!("demo-{}", index + 1),
                OPTIONS[index as usize % OPTIONS.len()],
            )
        })
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
