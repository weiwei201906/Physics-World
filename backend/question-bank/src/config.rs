//! Runtime configuration for the question bank service.
//!
//! Values come from the environment with the workspace defaults as fallback,
//! so the same binary works locally, in containers and in Kubernetes.

use std::path::PathBuf;

use common::defaults;

#[derive(Debug, Clone)]
pub struct QbankConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub redis_url: String,
    /// Local copy uploaded to RustFS to seed object storage.
    pub bundle_path: Option<PathBuf>,
    pub media_base: String,
    pub media_dir: Option<PathBuf>,
    pub media_bucket: String,
    pub cache_ttl_sec: u64,
    pub rustfs: Option<RustfsSettings>,
}

/// Connection details for the RustFS S3-compatible object storage that holds
/// the question bundle and its media.
#[derive(Debug, Clone)]
pub struct RustfsSettings {
    pub endpoint: String,
    pub bucket: String,
    pub object: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

impl QbankConfig {
    pub fn from_env() -> Self {
        let bundle_path = match std::env::var("QBANK_BUNDLE") {
            Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value)),
            _ => {
                let fallback = PathBuf::from(defaults::QBANK_BUNDLE_PATH);
                fallback.exists().then_some(fallback)
            }
        };
        Self {
            bind_addr: env_or("QBANK_BIND_ADDR", defaults::QBANK_BIND_ADDR),
            database_url: env_or("DATABASE_URL", defaults::DATABASE_URL),
            redis_url: env_or("REDIS_URL", defaults::REDIS_URL),
            bundle_path,
            media_base: env_or("QBANK_MEDIA_BASE", defaults::QBANK_MEDIA_BASE),
            media_dir: media_dir_from_env(),
            media_bucket: env_or("QBANK_RUSTFS_MEDIA_BUCKET", "qbank-media"),
            cache_ttl_sec: std::env::var("QBANK_CACHE_TTL_SEC")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults::QBANK_CACHE_TTL_SEC),
            rustfs: rustfs_from_env(),
        }
    }
}

fn media_dir_from_env() -> Option<PathBuf> {
    match std::env::var("QBANK_MEDIA_DIR") {
        Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value)),
        _ => {
            let fallback = PathBuf::from("deploy/qbank/media");
            fallback.exists().then_some(fallback)
        }
    }
}

fn rustfs_from_env() -> Option<RustfsSettings> {
    let endpoint = std::env::var("QBANK_RUSTFS_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    let access_key = std::env::var("QBANK_RUSTFS_ACCESS_KEY").ok()?;
    let secret_key = std::env::var("QBANK_RUSTFS_SECRET_KEY").ok()?;
    Some(RustfsSettings {
        endpoint,
        bucket: env_or("QBANK_RUSTFS_BUCKET", "questions"),
        object: env_or("QBANK_RUSTFS_OBJECT", "questions.json"),
        region: env_or("QBANK_RUSTFS_REGION", "us-east-1"),
        access_key,
        secret_key,
    })
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_owned())
}
