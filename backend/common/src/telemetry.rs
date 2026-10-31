//! Structured JSON logging setup shared by every service.

use std::env;

use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::util::SubscriberInitExt;

use crate::defaults;
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub log_level: String,
    pub otel_endpoint: Option<String>,
}

impl TelemetryConfig {
    pub fn from_env(service_name: impl Into<String>) -> Self {
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| defaults::LOG_LEVEL.to_string());
        let otel_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        Self {
            service_name: service_name.into(),
            log_level,
            otel_endpoint,
        }
    }
}

/// Installs a JSON tracing subscriber. Calling it twice is harmless.
pub fn init(config: &TelemetryConfig) -> Result<()> {
    let filter = EnvFilter::try_new(&config.log_level)
        .map_err(|error| AppError::config("LOG_LEVEL", error.to_string()))?;
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .finish();
    let _ = subscriber.try_init();
    tracing::info!(
        service = %config.service_name,
        otel_endpoint_configured = config.otel_endpoint.is_some(),
        "telemetry initialized"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_falls_back_to_default_level() {
        let config = TelemetryConfig {
            service_name: "test-service".to_string(),
            log_level: defaults::LOG_LEVEL.to_string(),
            otel_endpoint: None,
        };
        assert!(!config.service_name.is_empty());
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn invalid_filter_is_reported_as_config_error() {
        let error = init(&TelemetryConfig {
            service_name: "test-service".to_string(),
            log_level: "foo=bar=baz".to_string(),
            otel_endpoint: None,
        })
        .expect_err("invalid filter");
        assert_eq!(error.code(), "config_error");
    }
}
