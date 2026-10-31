//! user-api-sse: a read-only SSE endpoint fed by Redis pub/sub.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use common::identity::Authenticated;
use tokio::sync::broadcast::error::RecvError;

pub mod hub;
pub mod redis_bridge;
pub mod settings;

use hub::{BroadcastHub, SseEvent};
use redis_bridge::{revokes_user, AUTH_REVOKED_EVENT};
use settings::SseSettings;

#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<BroadcastHub>,
    pub heartbeat: Duration,
}

impl AppState {
    pub fn new(hub: Arc<BroadcastHub>, settings: &SseSettings) -> Self {
        Self {
            hub,
            heartbeat: Duration::from_secs(settings.heartbeat_sec),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .with_state(state)
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "user-api-sse",
    })
}

async fn events(
    Authenticated(identity): Authenticated,
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let user_id = identity.user_id;
    let heartbeat = state.heartbeat;
    let mut receiver = state.hub.subscribe();

    let stream = async_stream::stream! {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if event.event_type == AUTH_REVOKED_EVENT && revokes_user(&event.payload, user_id) {
                        tracing::info!(user_id = %user_id, "sse connection revoked");
                        break;
                    }
                    match encode_event(&event) {
                        Ok(frame) => yield Ok(frame),
                        Err(error) => tracing::warn!(error = %error, "failed to encode sse event"),
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(user_id = %user_id, skipped, "sse receiver lagged");
                }
                Err(RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(heartbeat).text("keepalive"))
}

fn encode_event(event: &SseEvent) -> Result<Event, axum::Error> {
    let payload = serde_json::json!({
        "type": event.event_type,
        "payload": event.payload,
        "ts": event.ts,
    });
    Event::default()
        .event(event.event_type.clone())
        .json_data(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let settings = SseSettings {
            port: settings::DEFAULT_PORT,
            redis_url: "redis://127.0.0.1:1/0".to_string(),
            heartbeat_sec: settings::DEFAULT_HEARTBEAT_SEC,
            notification_channel: settings::CHANNEL_NOTIFICATION.to_string(),
            auth_revoked_channel: settings::CHANNEL_AUTH_REVOKED.to_string(),
        };
        AppState::new(
            Arc::new(BroadcastHub::new(hub::DEFAULT_CAPACITY)),
            &settings,
        )
    }

    #[tokio::test]
    async fn health_returns_json_ok() {
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
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "user-api-sse");
    }

    #[tokio::test]
    async fn events_rejects_missing_identity() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(json["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn events_rejects_invalid_identity() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .header("x-user-id", "not-a-uuid")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn events_streams_for_valid_identity() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .header("x-user-id", "0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("text/event-stream"));
    }

    #[test]
    fn encodes_named_event_json_payload() {
        let event = SseEvent {
            event_type: "notification".to_string(),
            payload: serde_json::json!({"message": "hello"}),
            ts: 123,
        };
        assert!(encode_event(&event).is_ok());
    }
}
