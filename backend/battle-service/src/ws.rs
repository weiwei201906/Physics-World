//! Axum HTTP surface: health probe and the `/ws/battle` WebSocket endpoint.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use common::clock::Clock;
use common::error::{AppError, Result};
use common::identity::Authenticated;
use futures_util::future::pending;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::engine::RoomPhase;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::settings::Settings;
use crate::store::{BattleStore, EnqueueResult};

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
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ws/battle", get(ws_handler))
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

async fn ws_handler(
    Authenticated(identity): Authenticated,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, identity.user_id, state))
}

async fn handle_socket(mut socket: WebSocket, user_id: Uuid, state: AppState) {
    let mut token_exp: Option<DateTime<Utc>> = None;
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
        ClientMessage::Enqueue => match state.store.enqueue(user_id).await {
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
        },
        ClientMessage::Answer {
            room_id: requested_room,
            question_index,
            option,
        } => {
            if Some(requested_room) != *room_id {
                send_error(
                    socket,
                    "not_in_room",
                    "answer does not match the active room",
                )
                .await;
                return true;
            }
            if token_exp.is_some_and(|expires| state.clock.now() >= expires) {
                send_error(
                    socket,
                    "token_expired",
                    "access token expired; send a token message to refresh",
                )
                .await;
                return true;
            }
            match state
                .store
                .answer(user_id, requested_room, question_index, option)
                .await
            {
                Ok(outcome) => {
                    if outcome.phase == RoomPhase::Finished {
                        tracing::info!(room_id = %requested_room, "room finished");
                    }
                }
                Err(error) => {
                    let code = error.code();
                    send_error(socket, code, error.to_string()).await;
                }
            }
        }
        ClientMessage::Leave => {
            if let Err(error) = state.store.leave(user_id).await {
                tracing::warn!(error = %error, user_id = %user_id, "leave failed");
            }
            *room_id = None;
            *receiver = None;
        }
        ClientMessage::Token { token } => match state.tokens.validate(&token).await {
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
    async fn websocket_handshake_requires_gateway_identity() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/ws/battle")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
