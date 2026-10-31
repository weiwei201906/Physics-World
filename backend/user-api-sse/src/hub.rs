//! In-process broadcast hub shared by the SSE connections and the Redis bridge.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

pub const DEFAULT_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SseEvent {
    pub event_type: String,
    pub payload: Value,
    pub ts: i64,
}

pub struct BroadcastHub {
    sender: broadcast::Sender<SseEvent>,
}

impl BroadcastHub {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event_type: impl Into<String>, payload: Value) -> usize {
        let event = SseEvent {
            event_type: event_type.into(),
            payload,
            ts: unix_ts(),
        };
        self.sender.send(event).unwrap_or(0)
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn fans_out_to_multiple_subscribers() {
        let hub = BroadcastHub::new(16);
        let mut first = hub.subscribe();
        let mut second = hub.subscribe();
        assert_eq!(hub.receiver_count(), 2);

        let delivered = hub.publish("notification", json!({"message": "hello"}));
        assert_eq!(delivered, 2);

        let first_event = first.recv().await.expect("first subscriber");
        let second_event = second.recv().await.expect("second subscriber");
        assert_eq!(first_event, second_event);
        assert_eq!(first_event.event_type, "notification");
        assert_eq!(first_event.payload["message"], "hello");
        assert!(first_event.ts > 0);
    }

    #[tokio::test]
    async fn publish_without_subscribers_does_not_fail() {
        let hub = BroadcastHub::new(8);
        assert_eq!(hub.publish("notification", json!({"message": "nobody"})), 0);
    }

    #[tokio::test]
    async fn capacity_is_bounded() {
        let hub = BroadcastHub::new(2);
        let mut receiver = hub.subscribe();
        for index in 0..5 {
            hub.publish("notification", json!({"index": index}));
        }
        let error = receiver.recv().await.expect_err("lagged receiver");
        assert!(matches!(error, broadcast::error::RecvError::Lagged(_)));
    }
}
