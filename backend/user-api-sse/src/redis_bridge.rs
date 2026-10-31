//! Redis pub/sub bridge that feeds the in-process broadcast hub.
//!
//! Redis is treated as best effort: any connection or subscription failure is
//! logged and retried with backoff while the HTTP server keeps serving SSE.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;
use uuid::Uuid;

use crate::hub::BroadcastHub;
use crate::settings::{SseSettings, CHANNEL_AUTH_REVOKED, CHANNEL_NOTIFICATION};

pub const NOTIFICATION_EVENT: &str = "notification";
pub const AUTH_REVOKED_EVENT: &str = "auth_revoked";

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct RedisEvent {
    pub event_type: &'static str,
    pub payload: Value,
}

/// Maps a Redis channel to an SSE event type.
///
/// The configured channel names are accepted along with their default values,
/// so a renamed channel keeps producing the same event type.
pub fn parse_redis_message(
    channel: &str,
    payload: &str,
    notification_channel: &str,
    auth_revoked_channel: &str,
) -> Option<RedisEvent> {
    let event_type = if channel == notification_channel || channel == CHANNEL_NOTIFICATION {
        NOTIFICATION_EVENT
    } else if channel == auth_revoked_channel || channel == CHANNEL_AUTH_REVOKED {
        AUTH_REVOKED_EVENT
    } else {
        return None;
    };
    let payload =
        serde_json::from_str(payload).unwrap_or_else(|_| Value::String(payload.to_string()));
    Some(RedisEvent {
        event_type,
        payload,
    })
}

/// Returns true when an `auth_revoked` payload targets this user.
pub fn revokes_user(payload: &Value, user_id: Uuid) -> bool {
    let target = user_id.to_string();
    let Some(object) = payload.as_object() else {
        return false;
    };
    if let Some(Value::String(candidate)) = object.get("user_id") {
        return candidate.eq_ignore_ascii_case(&target);
    }
    if let Some(Value::Array(candidates)) = object.get("user_ids") {
        return candidates
            .iter()
            .filter_map(Value::as_str)
            .any(|candidate| candidate.eq_ignore_ascii_case(&target));
    }
    false
}

/// Runs the Redis bridge forever, reconnecting after failures.
pub async fn run(hub: Arc<BroadcastHub>, settings: SseSettings) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        match subscribe_loop(&hub, &settings).await {
            Ok(()) => {
                tracing::warn!("redis pub/sub stream ended, reconnecting");
                backoff = INITIAL_BACKOFF;
            }
            Err(error) => {
                tracing::warn!(error = %error, "redis pub/sub connection failed, retrying");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn subscribe_loop(
    hub: &BroadcastHub,
    settings: &SseSettings,
) -> Result<(), redis::RedisError> {
    let client = redis::Client::open(settings.redis_url.clone())?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub
        .subscribe(settings.notification_channel.as_str())
        .await?;
    pubsub
        .subscribe(settings.auth_revoked_channel.as_str())
        .await?;
    tracing::info!(
        notification_channel = %settings.notification_channel,
        auth_revoked_channel = %settings.auth_revoked_channel,
        "redis pub/sub subscribed"
    );

    let mut messages = pubsub.on_message();
    while let Some(message) = messages.next().await {
        let channel = match message.get_channel::<String>() {
            Ok(channel) => channel,
            Err(error) => {
                tracing::warn!(error = %error, "failed to read redis channel");
                continue;
            }
        };
        let payload = match message.get_payload::<String>() {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(error = %error, channel = %channel, "failed to read redis payload");
                continue;
            }
        };
        match parse_redis_message(
            &channel,
            &payload,
            &settings.notification_channel,
            &settings.auth_revoked_channel,
        ) {
            Some(event) => {
                hub.publish(event.event_type, event.payload);
            }
            None => {
                tracing::debug!(channel = %channel, "ignoring unexpected redis channel");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_notification_channel_to_event_type() {
        let event = parse_redis_message(
            CHANNEL_NOTIFICATION,
            r#"{"attempt_id":"abc","status":"graded"}"#,
            CHANNEL_NOTIFICATION,
            CHANNEL_AUTH_REVOKED,
        )
        .expect("notification channel");
        assert_eq!(event.event_type, NOTIFICATION_EVENT);
        assert_eq!(event.payload["attempt_id"], "abc");
    }

    #[test]
    fn maps_auth_revoked_channel_to_event_type() {
        let event = parse_redis_message(
            CHANNEL_AUTH_REVOKED,
            r#"{"user_id":"0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d"}"#,
            CHANNEL_NOTIFICATION,
            CHANNEL_AUTH_REVOKED,
        )
        .expect("auth revoked channel");
        assert_eq!(event.event_type, AUTH_REVOKED_EVENT);
        assert_eq!(
            event.payload["user_id"],
            "0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d"
        );
    }

    #[test]
    fn honors_configured_channel_names() {
        let event = parse_redis_message(
            "custom:notifications",
            "{}",
            "custom:notifications",
            "custom:revoked",
        )
        .expect("custom notification channel");
        assert_eq!(event.event_type, NOTIFICATION_EVENT);
    }

    #[test]
    fn unknown_channel_is_ignored() {
        assert!(parse_redis_message(
            "some:other",
            "{}",
            CHANNEL_NOTIFICATION,
            CHANNEL_AUTH_REVOKED
        )
        .is_none());
    }

    #[test]
    fn non_json_payload_is_kept_as_text() {
        let event = parse_redis_message(
            CHANNEL_NOTIFICATION,
            "raw text",
            CHANNEL_NOTIFICATION,
            CHANNEL_AUTH_REVOKED,
        )
        .expect("notification channel");
        assert_eq!(event.payload, json!("raw text"));
    }

    #[test]
    fn revocation_matches_user_id_or_list() {
        let user_id = Uuid::parse_str("0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d").expect("uuid");
        assert!(revokes_user(
            &json!({"user_id": user_id.to_string()}),
            user_id
        ));
        assert!(revokes_user(
            &json!({"user_ids": [user_id.to_string()]}),
            user_id
        ));
        assert!(!revokes_user(
            &json!({"user_id": Uuid::nil().to_string()}),
            user_id
        ));
        assert!(!revokes_user(&json!({"reason": "logout"}), user_id));
        assert!(!revokes_user(&json!("plain"), user_id));
    }
}
