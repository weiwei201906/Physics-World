//! Local settings for the recommendation service.
//!
//! Values are read from the `rec` namespace through [`ConfigLoader`], so the
//! environment variables are `REC_PORT`, `REC_DELTA_WRONG`,
//! `REC_DELTA_CORRECT`, `REC_TOP_K_DEFAULT`, `REC_MIN_SCORE` and
//! `REC_CHAPTER_DIMS`.

use std::collections::HashMap;
use std::env;

use common::config::ConfigLoader;

pub const DEFAULT_PORT: u16 = 8086;
pub const DEFAULT_DELTA_WRONG: f64 = 1.0;
pub const DEFAULT_DELTA_CORRECT: f64 = 0.2;
pub const DEFAULT_TOP_K: usize = 10;
pub const DEFAULT_MIN_SCORE: f64 = 0.05;
pub const DEFAULT_LOCK_TTL_SEC: u64 = 30;

pub const DEFAULT_CHAPTER_DIMS: [&str; 8] = [
    "mechanics",
    "thermodynamics",
    "optics",
    "electromagnetism",
    "modern",
    "waves",
    "fluids",
    "experiments",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub port: u16,
    pub delta_wrong: f64,
    pub delta_correct: f64,
    pub top_k_default: usize,
    pub min_score: f64,
    pub chapter_dims: Vec<String>,
    pub lock_ttl_sec: u64,
}

impl Settings {
    pub fn from_env() -> Self {
        // Only REC_* variables are considered; bare names such as PORT from the
        // host environment must not leak into the service settings.
        let env = env::vars()
            .filter(|(key, _)| key.starts_with("REC_"))
            .collect::<HashMap<_, _>>();
        Self::from_config(&ConfigLoader::new("rec", env))
    }

    pub fn from_config(config: &ConfigLoader) -> Self {
        let raw_port = config.get_u32("port", u32::from(DEFAULT_PORT));
        let port = u16::try_from(raw_port).unwrap_or(DEFAULT_PORT);
        Self {
            port,
            delta_wrong: config.get_f64("delta_wrong", DEFAULT_DELTA_WRONG),
            delta_correct: config.get_f64("delta_correct", DEFAULT_DELTA_CORRECT),
            top_k_default: config.get_usize("top_k_default", DEFAULT_TOP_K),
            min_score: config.get_f64("min_score", DEFAULT_MIN_SCORE),
            chapter_dims: config.get_list("chapter_dims", &DEFAULT_CHAPTER_DIMS),
            lock_ttl_sec: u64::from(config.get_u32("lock_ttl_sec", DEFAULT_LOCK_TTL_SEC as u32)),
        }
    }

    /// Bind address derived from the configured port.
    pub fn bind_addr(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }

    /// Resolve an optional request limit into a positive result size.
    pub fn resolve_limit(&self, requested: Option<i32>) -> usize {
        match requested {
            Some(value) if value > 0 => value as usize,
            _ => self.top_k_default.max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn loader(pairs: &[(&str, &str)]) -> ConfigLoader {
        let env = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<HashMap<_, _>>();
        ConfigLoader::new("rec", env)
    }

    #[test]
    fn defaults_match_the_architecture_table() {
        let settings = Settings::from_config(&loader(&[]));
        assert_eq!(settings.port, 8086);
        assert_eq!(settings.delta_wrong, 1.0);
        assert_eq!(settings.delta_correct, 0.2);
        assert_eq!(settings.top_k_default, 10);
        assert_eq!(settings.chapter_dims.len(), 8);
        assert_eq!(settings.bind_addr(), "0.0.0.0:8086");
    }

    #[test]
    fn env_overrides_are_read_with_rec_prefix() {
        let settings = Settings::from_config(&loader(&[
            ("REC_PORT", "9000"),
            ("REC_DELTA_WRONG", "2.5"),
            ("REC_DELTA_CORRECT", "0.1"),
            ("REC_TOP_K_DEFAULT", "3"),
            ("REC_MIN_SCORE", "0.4"),
            ("REC_CHAPTER_DIMS", "mechanics,optics"),
        ]));
        assert_eq!(settings.port, 9000);
        assert_eq!(settings.delta_wrong, 2.5);
        assert_eq!(settings.delta_correct, 0.1);
        assert_eq!(settings.top_k_default, 3);
        assert_eq!(settings.min_score, 0.4);
        assert_eq!(
            settings.chapter_dims,
            vec!["mechanics".to_string(), "optics".to_string()]
        );
    }

    #[test]
    fn invalid_port_falls_back_to_default() {
        let settings = Settings::from_config(&loader(&[("REC_PORT", "70000")]));
        assert_eq!(settings.port, 8086);
    }

    #[test]
    fn limit_resolution_uses_default_for_missing_or_non_positive() {
        let settings = Settings::from_config(&loader(&[("REC_TOP_K_DEFAULT", "7")]));
        assert_eq!(settings.resolve_limit(None), 7);
        assert_eq!(settings.resolve_limit(Some(0)), 7);
        assert_eq!(settings.resolve_limit(Some(-1)), 7);
        assert_eq!(settings.resolve_limit(Some(2)), 2);
    }
}
