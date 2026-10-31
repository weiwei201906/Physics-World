//! Axum HTTP surface: health probe and the `/ws/battle` WebSocket endpoint.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use common::clock::Clock;
use common::error::{AppError, Result};
use futures_util::future::pending;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::engine::RoomPhase;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::settings::Settings;
use crate::store::{question_message, room_state_message, BattleStore, EnqueueResult};
use crate::vector::{VectorSource, ZeroVectorSource};

const TICK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub jti: Option<String>,
}

#[async_trait]
pub trait TokenValidator: Send + Sync {
    async fn validate(&self, token: &str) -> Result<TokenClaims>;
}

#[derive(Debug, Default)]
pub struct InMemoryTokenValidator {
    tokens: Mutex<HashMap<String, TokenClaims>>,
}

impl InMemoryTokenValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, token: impl Into<String>, claims: TokenClaims) {
        self.lock().insert(token.into(), claims);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, TokenClaims>> {
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl TokenValidator for InMemoryTokenValidator {
    async fn validate(&self, token: &str) -> Result<TokenClaims> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::unauthenticated("empty token"));
        }
        self.lock()
            .get(token)
            .cloned()
            .ok_or_else(|| AppError::unauthenticated("unknown token"))
    }
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn BattleStore>,
    pub settings: Arc<Settings>,
    pub clock: Arc<dyn Clock>,
    pub tokens: Arc<dyn TokenValidator>,
    pub vectors: Arc<dyn VectorSource>,
    pub history_pool: Option<sqlx::PgPool>,
}

impl AppState {
    pub fn new(
        store: Arc<dyn BattleStore>,
        settings: Settings,
        clock: Arc<dyn Clock>,
        tokens: Arc<dyn TokenValidator>,
    ) -> Self {
        Self {
            store,
            settings: Arc::new(settings),
            clock,
            tokens,
            vectors: Arc::new(ZeroVectorSource),
            history_pool: None,
        }
    }

    /// Uses `source` for matchmaking vectors; the zero source keeps FIFO.
    pub fn with_vector_source(mut self, source: Arc<dyn VectorSource>) -> Self {
        self.vectors = source;
        self
    }

    /// Enables the battle-history query endpoint backed by PostgreSQL.
    pub fn with_history_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.history_pool = Some(pool);
        self
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ws/battle", get(ws_handler))
        .route("/api/battles/history", get(battle_history))
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "battle-service",
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BattleHistoryEntry {
    battle_id: Uuid,
    mode: String,
    question_count: i32,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    my_score: i64,
    my_correct: i32,
    my_rank: i32,
    opponent_id: Option<Uuid>,
    opponent_nickname: Option<String>,
    opponent_score: Option<i64>,
    opponent_correct: Option<i32>,
    result: String,
}

/// Returns the authenticated user's battle history. Identity is injected by the
/// gateway via the `X-User-Id` header after verifying the bearer token.
async fn battle_history(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<BattleHistoryEntry>>> {
    let user_id = headers
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| AppError::unauthenticated("missing user identity"))?;

    let Some(pool) = state.history_pool.as_ref() else {
        return Err(AppError::not_found("battle history is not available"));
    };

    type Row = (
        Uuid,
        String,
        i32,
        DateTime<Utc>,
        DateTime<Utc>,
        i64,
        i32,
        i32,
        Option<Uuid>,
        Option<String>,
        Option<i64>,
        Option<i32>,
        String,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT b.battle_id, b.mode, b.question_count, b.started_at, b.finished_at, \
                me.score AS my_score, me.correct_count AS my_correct, me.rank AS my_rank, \
                opp.user_id AS opponent_id, \
                COALESCE(up.nickname, au.email) AS opponent_nickname, \
                opp.score AS opponent_score, opp.correct_count AS opponent_correct, \
                CASE \
                  WHEN b.winner_id = $1 THEN 'win' \
                  WHEN b.winner_id IS NULL THEN 'draw' \
                  ELSE 'lose' \
                END AS result \
         FROM battle.battles b \
         JOIN battle.battle_participants me ON me.battle_id = b.battle_id AND me.user_id = $1 \
         LEFT JOIN battle.battle_participants opp ON opp.battle_id = b.battle_id AND opp.user_id <> $1 \
         LEFT JOIN app.user_profiles up ON up.user_id = opp.user_id \
         LEFT JOIN auth.users au ON au.id = opp.user_id \
         ORDER BY b.finished_at DESC \
         LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| AppError::upstream("postgres", error.to_string()))?;

    let entries: Vec<BattleHistoryEntry> = rows
        .into_iter()
        .map(
            |(
                battle_id,
                mode,
                question_count,
                started_at,
                finished_at,
                my_score,
                my_correct,
                my_rank,
                opponent_id,
                opponent_nickname,
                opponent_score,
                opponent_correct,
                result,
            )| {
                BattleHistoryEntry {
                    battle_id,
                    mode,
                    question_count,
                    started_at,
                    finished_at,
                    my_score,
                    my_correct,
                    my_rank,
                    opponent_id,
                    opponent_nickname,
                    opponent_score,
                    opponent_correct,
                    result,
                }
            },
        )
        .collect();

    Ok(Json(entries))
}

#[derive(Debug, Clone, Copy)]
struct WsIdentity {
    user_id: Uuid,
    expires_at: DateTime<Utc>,
}

fn query_token(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "token")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl FromRequestParts<AppState> for WsIdentity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = parts
            .uri
            .query()
            .and_then(query_token)
            .or_else(|| bearer_token(&parts.headers))
            .ok_or_else(|| AppError::unauthenticated("missing access token"))?;
        let claims = state.tokens.validate(&token).await?;
        Ok(Self {
            user_id: claims.user_id,
            expires_at: claims.expires_at,
        })
    }
}

async fn ws_handler(
    WsIdentity {
        user_id,
        expires_at,
    }: WsIdentity,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, user_id, expires_at, state))
}

async fn handle_socket(
    mut socket: WebSocket,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    state: AppState,
) {
    let mut token_exp: Option<DateTime<Utc>> = Some(expires_at);
    let mut room_id: Option<Uuid> = None;
    let mut receiver: Option<broadcast::Receiver<ServerMessage>> = None;
    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match ClientMessage::parse(text.as_str()) {
                            Ok(message) => {
                                if !handle_client_message(
                                    &mut socket,
                                    &state,
                                    user_id,
                                    &mut token_exp,
                                    &mut room_id,
                                    &mut receiver,
                                    message,
                                )
                                .await
                                {
                                    break;
                                }
                            }
                            Err(error) => {
                                let code = error.code();
                                send_error(&mut socket, code, error.to_string()).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Binary(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(_)) => break,
                }
            }
            received = async {
                match receiver.as_mut() {
                    Some(room_receiver) => room_receiver.recv().await,
                    None => pending::<std::result::Result<ServerMessage, broadcast::error::RecvError>>().await,
                }
            } => {
                match received {
                    Ok(message) => {
                        let queue_timeout = matches!(
                            &message,
                            ServerMessage::Error { code, .. } if code == "queue_timeout"
                        );
                        if !send_message(&mut socket, &message).await {
                            break;
                        }
                        if queue_timeout {
                            room_id = None;
                            receiver = None;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(user_id = %user_id, skipped, "websocket broadcast lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        send_error(&mut socket, "room_closed", "room is no longer available").await;
                        room_id = None;
                        receiver = None;
                    }
                }
            }
            _ = ticker.tick() => {
                if let Err(error) = state.store.tick().await {
                    tracing::warn!(error = %error, "battle tick failed");
                }
            }
        }
    }

    if let Err(error) = state.store.leave(user_id).await {
        tracing::warn!(error = %error, user_id = %user_id, "disconnect cleanup failed");
    }
}

async fn handle_client_message(
    socket: &mut WebSocket,
    state: &AppState,
    user_id: Uuid,
    token_exp: &mut Option<DateTime<Utc>>,
    room_id: &mut Option<Uuid>,
    receiver: &mut Option<broadcast::Receiver<ServerMessage>>,
    message: ClientMessage,
) -> bool {
    match message {
        ClientMessage::FindMatch { mode } => {
            let profile = state.vectors.profile(user_id).await;
            match state
                .store
                .enqueue_with_profile(user_id, profile.nickname, mode, profile.vector)
                .await
            {
                Ok(EnqueueResult::Waiting {
                    room_id: waiting_room,
                    receiver: room_receiver,
                }) => {
                    *room_id = Some(waiting_room);
                    *receiver = Some(room_receiver);
                }
                Ok(EnqueueResult::Matched {
                    room_id: matched_room,
                    receiver: room_receiver,
                    ..
                }) => {
                    *room_id = Some(matched_room);
                    *receiver = Some(room_receiver);
                }
                Err(error) => {
                    let code = error.code();
                    send_error(socket, code, error.to_string()).await;
                }
            }
        }
        ClientMessage::CancelMatch => {
            if let Err(error) = state.store.leave(user_id).await {
                tracing::warn!(error = %error, user_id = %user_id, "leave failed");
            }
            *room_id = None;
            *receiver = None;
        }
        ClientMessage::JoinRoom {
            room_id: requested_room,
        } => {
            let room_receiver = match state.store.subscribe(requested_room).await {
                Ok(room_receiver) => room_receiver,
                Err(error) => {
                    let code = error.code();
                    send_error(socket, code, error.to_string()).await;
                    return true;
                }
            };
            match state.store.join(user_id, requested_room).await {
                Ok((view, question)) => {
                    *room_id = Some(requested_room);
                    *receiver = Some(room_receiver);
                    if !send_message(socket, &room_state_message(&view)).await {
                        return false;
                    }
                    if let Some(question) = question {
                        if !send_message(socket, &question_message(&question)).await {
                            return false;
                        }
                    }
                }
                Err(error) => {
                    let code = error.code();
                    send_error(socket, code, error.to_string()).await;
                }
            }
        }
        ClientMessage::Answer {
            question_index,
            option_id,
        } => {
            let Some(active_room) = *room_id else {
                send_error(socket, "not_in_room", "join a room before answering").await;
                return true;
            };
            if token_exp.is_some_and(|expires| state.clock.now() >= expires) {
                send_error(
                    socket,
                    "token_expired",
                    "access token expired; send a token update to refresh",
                )
                .await;
                return true;
            }
            match state
                .store
                .answer(user_id, active_room, question_index, option_id)
                .await
            {
                Ok(outcome) => {
                    // In 1v1 the answer result is private: the opponent must
                    // not learn the correct option from your submission. Send
                    // it directly to this socket instead of broadcasting.
                    if outcome.mode == "1v1" {
                        let _ = send_message(
                            socket,
                            &ServerMessage::AnswerResult {
                                index: outcome.question_index,
                                correct: outcome.correct,
                                correct_option_id: Some(outcome.correct_option_id),
                                score: outcome.points,
                                total_score: outcome.total_score,
                                explanation: Some(outcome.explanation),
                            },
                        )
                        .await;
                    }
                    if outcome.phase == RoomPhase::Finished {
                        tracing::info!(room_id = %active_room, "room finished");
                    }
                }
                Err(error) => {
                    let code = error.code();
                    send_error(socket, code, error.to_string()).await;
                }
            }
        }
        ClientMessage::TokenUpdate { token } => match state.tokens.validate(&token).await {
            Ok(claims) if claims.user_id == user_id => {
                *token_exp = Some(claims.expires_at);
            }
            Ok(_) => {
                send_error(
                    socket,
                    "invalid_token",
                    "token does not belong to this connection",
                )
                .await;
            }
            Err(error) => {
                let code = error.code();
                send_error(socket, code, error.to_string()).await;
            }
        },
    }
    true
}

async fn send_message(socket: &mut WebSocket, message: &ServerMessage) -> bool {
    match message.to_json() {
        Ok(text) => socket.send(Message::Text(text.into())).await.is_ok(),
        Err(error) => {
            tracing::error!(error = %error, "failed to serialize server message");
            false
        }
    }
}

async fn send_error(socket: &mut WebSocket, code: impl Into<String>, message: impl Into<String>) {
    let message = ServerMessage::error(code, message);
    let _ = send_message(socket, &message).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use common::clock::FakeClock;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::engine::BattleEngine;
    use crate::store::{InMemoryBattleStore, InMemoryReconciler};

    fn test_state() -> AppState {
        let clock: Arc<dyn Clock> = Arc::new(FakeClock::new(Utc::now()));
        let settings = Settings::default();
        let engine = BattleEngine::new(settings.clone(), clock.clone(), Vec::new());
        let store = Arc::new(InMemoryBattleStore::new(
            engine,
            Arc::new(InMemoryReconciler::default()),
        ));
        AppState::new(
            store,
            settings,
            clock,
            Arc::new(InMemoryTokenValidator::default()),
        )
    }

    #[tokio::test]
    async fn health_endpoint_reports_ok() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["service"], "battle-service");
    }

    #[tokio::test]
    async fn websocket_handshake_requires_access_token() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/ws/battle")
                    .header("connection", "upgrade")
                    .header("upgrade", "websocket")
                    .header("sec-websocket-version", "13")
                    .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "got {:?}",
            response.status()
        );
    }

    #[tokio::test]
    async fn in_memory_token_validator_checks_membership_and_owner() {
        let validator = InMemoryTokenValidator::default();
        let user_id = Uuid::new_v4();
        assert!(validator.validate("missing").await.is_err());
        validator.insert(
            "token",
            TokenClaims {
                user_id,
                expires_at: Utc::now(),
                jti: Some("jti".to_string()),
            },
        );
        let claims = validator.validate("token").await.expect("claims");
        assert_eq!(claims.user_id, user_id);
    }
}
