//! Layered configuration loader.
//!
//! Lookup order for a key such as `jwt.access_ttl_sec` inside namespace `auth`:
//!
//! 1. `AUTH_JWT_ACCESS_TTL_SEC` (namespace prefixed environment variable)
//! 2. `JWT_ACCESS_TTL_SEC` (bare environment variable)
//! 3. the built-in default supplied by the caller
//!
//! Missing and invalid keys are recorded so services can emit the
//! `config_missing_key_total` counter at startup without failing hard.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Loads configuration values from environment variables scoped by namespace.
#[derive(Debug, Clone)]
pub struct ConfigLoader {
    namespace: String,
    env: HashMap<String, String>,
    missing: Arc<Mutex<Vec<String>>>,
    invalid: Arc<Mutex<Vec<String>>>,
}

impl ConfigLoader {
    pub fn new(namespace: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            namespace: namespace.into(),
            env,
            missing: Arc::new(Mutex::new(Vec::new())),
            invalid: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn from_env(namespace: impl Into<String>) -> Self {
        let env = std::env::vars().collect();
        Self::new(namespace, env)
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Environment variable name for a namespaced key.
    pub fn env_key(&self, key: &str) -> String {
        let namespace = normalize(&self.namespace);
        let key = normalize(key);
        if namespace.is_empty() {
            key
        } else {
            format!("{namespace}_{key}")
        }
    }

    fn lookup(&self, key: &str) -> Option<&str> {
        let namespaced = self.env_key(key);
        if let Some(value) = self.env.get(&namespaced) {
            return Some(value.as_str());
        }
        let bare = normalize(key);
        self.env.get(&bare).map(String::as_str)
    }

    pub fn get_string(&self, key: &str, default: &str) -> String {
        match self.lookup(key) {
            Some(value) => value.to_string(),
            None => {
                self.note_missing(key);
                default.to_string()
            }
        }
    }

    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.parse(key, default, |raw| raw.parse::<i64>().map_err(|_| ()))
    }

    pub fn get_u32(&self, key: &str, default: u32) -> u32 {
        self.parse(key, default, |raw| raw.parse::<u32>().map_err(|_| ()))
    }

    pub fn get_usize(&self, key: &str, default: usize) -> usize {
        self.parse(key, default, |raw| raw.parse::<usize>().map_err(|_| ()))
    }

    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.parse(key, default, |raw| raw.parse::<f64>().map_err(|_| ()))
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.parse(key, default, parse_bool)
    }

    pub fn get_list(&self, key: &str, default: &[&str]) -> Vec<String> {
        match self.lookup(key) {
            Some(raw) => match parse_list(raw) {
                Ok(values) => values,
                Err(_) => {
                    self.note_invalid(key);
                    default.iter().map(|value| value.to_string()).collect()
                }
            },
            None => {
                self.note_missing(key);
                default.iter().map(|value| value.to_string()).collect()
            }
        }
    }

    pub fn missing_keys(&self) -> Vec<String> {
        self.missing
            .lock()
            .map(|keys| keys.clone())
            .unwrap_or_default()
    }

    pub fn invalid_keys(&self) -> Vec<String> {
        self.invalid
            .lock()
            .map(|keys| keys.clone())
            .unwrap_or_default()
    }

    fn parse<T>(&self, key: &str, default: T, parser: impl Fn(&str) -> Result<T, ()>) -> T
    where
        T: Copy,
    {
        match self.lookup(key) {
            Some(raw) => match parser(raw.trim()) {
                Ok(value) => value,
                Err(()) => {
                    self.note_invalid(key);
                    default
                }
            },
            None => {
                self.note_missing(key);
                default
            }
        }
    }

    fn note_missing(&self, key: &str) {
        if let Ok(mut keys) = self.missing.lock() {
            let entry = key.to_string();
            if !keys.contains(&entry) {
                keys.push(entry);
            }
        }
    }

    fn note_invalid(&self, key: &str) {
        if let Ok(mut keys) = self.invalid.lock() {
            let entry = key.to_string();
            if !keys.contains(&entry) {
                keys.push(entry);
            }
        }
    }
}

fn normalize(value: &str) -> String {
    value.trim().replace(['.', '-'], "_").to_ascii_uppercase()
}

fn parse_bool(raw: &str) -> Result<bool, ()> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(()),
    }
}

fn parse_list(raw: &str) -> Result<Vec<String>, ()> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        let parsed: Vec<String> = serde_json::from_str(trimmed).map_err(|_| ())?;
        return Ok(parsed);
    }
    let values: Vec<String> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect();
    if values.is_empty() {
        return Err(());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(namespace: &str, pairs: &[(&str, &str)]) -> ConfigLoader {
        let env = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        ConfigLoader::new(namespace, env)
    }

    #[test]
    fn env_key_flattens_namespace_and_dots() {
        let loader = ConfigLoader::new("user_api", HashMap::new());
        assert_eq!(
            loader.env_key("submit.consistency_window_sec"),
            "USER_API_SUBMIT_CONSISTENCY_WINDOW_SEC"
        );
        let loader = ConfigLoader::new("", HashMap::new());
        assert_eq!(loader.env_key("server.bind-addr"), "SERVER_BIND_ADDR");
    }

    #[test]
    fn namespaced_value_wins_over_bare_value() {
        let loader = loader(
            "auth",
            &[
                ("AUTH_JWT_ACCESS_TTL_SEC", "120"),
                ("JWT_ACCESS_TTL_SEC", "999"),
            ],
        );
        assert_eq!(loader.get_i64("jwt.access_ttl_sec", 900), 120);
    }

    #[test]
    fn missing_key_falls_back_and_is_recorded() {
        let loader = loader("auth", &[]);
        assert_eq!(loader.get_i64("refresh_grace_sec", 5), 5);
        assert_eq!(loader.missing_keys(), vec!["refresh_grace_sec"]);
    }

    #[test]
    fn invalid_value_falls_back_and_is_recorded() {
        let loader = loader("auth", &[("AUTH_REFRESH_GRACE_SEC", "abc")]);
        assert_eq!(loader.get_i64("refresh_grace_sec", 5), 5);
        assert_eq!(loader.invalid_keys(), vec!["refresh_grace_sec"]);
    }

    #[test]
    fn namespace_isolation_keeps_keys_separate() {
        let loader = loader(
            "user_api",
            &[
                ("AUTH_JWT_ACCESS_TTL_SEC", "30"),
                ("USER_API_SUBMIT_CONSISTENCY_WINDOW_SEC", "3"),
            ],
        );
        assert_eq!(loader.get_i64("submit.consistency_window_sec", 3), 3);
        assert_eq!(loader.get_i64("jwt.access_ttl_sec", 900), 900);
    }

    #[test]
    fn bool_values_accept_common_spellings() {
        assert!(parse_bool("TRUE").is_ok());
        assert!(parse_bool("0").is_ok());
        assert!(parse_bool("maybe").is_err());
        let loader = loader("auth", &[("AUTH_COOKIE_SECURE", "off")]);
        assert!(!loader.get_bool("cookie.secure", true));
    }

    #[test]
    fn list_values_accept_json_and_csv() {
        let period_loader = loader(
            "user_api",
            &[("USER_API_LEADERBOARD_PERIODS", "[\"daily\",\"weekly\"]")],
        );
        assert_eq!(
            period_loader.get_list("leaderboard.periods", &["daily"]),
            vec!["daily".to_string(), "weekly".to_string()]
        );
        let cidr_loader = loader(
            "user_api",
            &[("USER_API_ADMIN_ALLOWED_CIDRS", "10.0.0.0/8, 192.168.0.0/16")],
        );
        assert_eq!(
            cidr_loader.get_list("admin.allowed_cidrs", &["10.0.0.0/8"]),
            vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()]
        );
    }

    #[test]
    fn malformed_json_list_falls_back() {
        let loader = loader(
            "user_api",
            &[("USER_API_LEADERBOARD_PERIODS", "[daily,weekly]")],
        );
        assert_eq!(
            loader.get_list("leaderboard.periods", &["daily"]),
            vec!["daily".to_string()]
        );
        assert_eq!(loader.invalid_keys(), vec!["leaderboard.periods"]);
    }
}
