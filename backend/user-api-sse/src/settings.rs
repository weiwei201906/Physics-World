//! Service-local settings. Environment variables fall back to safe defaults.

use std::env;

use common::defaults;

pub const DEFAULT_PORT: u16 = 8085;
pub const DEFAULT_HEARTBEAT_SEC: u64 = 30;
pub const CHANNEL_NOTIFICATION: &str = "notification:broadcast";
pub const CHANNEL_AUTH_REVOKED: &str = "auth:revoked";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseSettings {
    pub port: u16,
    pub redis_url: String,
    pub heartbeat_sec: u64,
    pub notification_channel: String,
    pub auth_revoked_channel: String,
}

impl SseSettings {
    pub fn from_env() -> Self {
        let port = env::var("PORT")
            .ok()
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let redis_url = env::var("REDIS_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| defaults::REDIS_URL.to_string());
        let heartbeat_sec = env::var("SSE_HEARTBEAT_SEC")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_SEC);
        let notification_channel =
            channel_from_env("SSE_NOTIFICATION_CHANNEL", CHANNEL_NOTIFICATION);
        let auth_revoked_channel =
            channel_from_env("SSE_AUTH_REVOKED_CHANNEL", CHANNEL_AUTH_REVOKED);
        Self {
            port,
            redis_url,
            heartbeat_sec,
            notification_channel,
            auth_revoked_channel,
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}

fn channel_from_env(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_stable() {
        let settings = SseSettings {
            port: DEFAULT_PORT,
            redis_url: defaults::REDIS_URL.to_string(),
            heartbeat_sec: DEFAULT_HEARTBEAT_SEC,
            notification_channel: CHANNEL_NOTIFICATION.to_string(),
            auth_revoked_channel: CHANNEL_AUTH_REVOKED.to_string(),
        };
        assert_eq!(settings.port, 8085);
        assert_eq!(settings.heartbeat_sec, 30);
        assert_eq!(settings.bind_addr(), "0.0.0.0:8085");
        assert_eq!(settings.notification_channel, "notification:broadcast");
        assert_eq!(settings.auth_revoked_channel, "auth:revoked");
    }
}
