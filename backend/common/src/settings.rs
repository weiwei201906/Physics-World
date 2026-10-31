//! Typed settings for every service. Each value comes from [`ConfigLoader`];
//! defaults live in [`crate::defaults`] and are never embedded in business code.

use chrono::Duration;

use crate::config::ConfigLoader;
use crate::defaults;

#[derive(Debug, Clone)]
pub struct AuthSettings {
    pub access_ttl_sec: i64,
    pub refresh_ttl_days: i64,
    pub rotate_days: i64,
    pub leeway_sec: i64,
    pub refresh_grace_sec: i64,
    pub refresh_rate_limit_per_min: i64,
    pub login_per_email: i64,
    pub login_per_ip: i64,
    pub login_window_sec: i64,
    pub password_min_length: usize,
    pub argon2_memory_kb: u32,
    pub argon2_iterations: u32,
    pub argon2_parallelism: u32,
    pub cookie_same_site: String,
    pub cookie_max_age_days: i64,
    pub cookie_secure: bool,
    pub cookie_name: String,
    pub cookie_path: String,
    pub refresh_token_bytes: usize,
}

impl AuthSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            access_ttl_sec: config.get_i64("jwt.access_ttl_sec", defaults::AUTH_JWT_ACCESS_TTL_SEC),
            refresh_ttl_days: config
                .get_i64("jwt.refresh_ttl_days", defaults::AUTH_JWT_REFRESH_TTL_DAYS),
            rotate_days: config.get_i64("jwt.rotate_days", defaults::AUTH_JWT_ROTATE_DAYS),
            leeway_sec: config.get_i64("jwt.leeway_sec", defaults::AUTH_JWT_LEEWAY_SEC),
            refresh_grace_sec: config
                .get_i64("refresh_grace_sec", defaults::AUTH_REFRESH_GRACE_SEC),
            refresh_rate_limit_per_min: config.get_i64(
                "refresh_rate_limit_per_min",
                defaults::AUTH_REFRESH_RATE_LIMIT_PER_MIN,
            ),
            login_per_email: config.get_i64(
                "login_rate_limit.per_email",
                defaults::AUTH_LOGIN_RATE_LIMIT_PER_EMAIL,
            ),
            login_per_ip: config.get_i64(
                "login_rate_limit.per_ip",
                defaults::AUTH_LOGIN_RATE_LIMIT_PER_IP,
            ),
            login_window_sec: config.get_i64(
                "login_rate_limit.window_sec",
                defaults::AUTH_LOGIN_RATE_LIMIT_WINDOW_SEC,
            ),
            password_min_length: config.get_usize(
                "password.min_length",
                defaults::AUTH_PASSWORD_MIN_LENGTH as usize,
            ),
            argon2_memory_kb: config.get_u32("argon2.memory_kb", defaults::AUTH_ARGON2_MEMORY_KB),
            argon2_iterations: config
                .get_u32("argon2.iterations", defaults::AUTH_ARGON2_ITERATIONS),
            argon2_parallelism: config
                .get_u32("argon2.parallelism", defaults::AUTH_ARGON2_PARALLELISM),
            cookie_same_site: config
                .get_string("cookie.same_site", defaults::AUTH_COOKIE_SAME_SITE),
            cookie_max_age_days: config
                .get_i64("cookie.max_age_days", defaults::AUTH_COOKIE_MAX_AGE_DAYS),
            cookie_secure: config.get_bool("cookie.secure", defaults::AUTH_COOKIE_SECURE),
            cookie_name: config.get_string("cookie.name", defaults::AUTH_COOKIE_NAME),
            cookie_path: config.get_string("cookie.path", defaults::AUTH_COOKIE_PATH),
            refresh_token_bytes: config
                .get_usize("refresh_token_bytes", defaults::AUTH_REFRESH_TOKEN_BYTES),
        }
    }

    pub fn access_ttl(&self) -> Duration {
        Duration::seconds(self.access_ttl_sec)
    }

    pub fn refresh_ttl(&self) -> Duration {
        Duration::days(self.refresh_ttl_days)
    }

    pub fn refresh_grace(&self) -> Duration {
        Duration::seconds(self.refresh_grace_sec)
    }

    pub fn login_window(&self) -> Duration {
        Duration::seconds(self.login_window_sec)
    }

    pub fn cookie_max_age_sec(&self) -> i64 {
        self.cookie_max_age_days * 86_400
    }
}

#[derive(Debug, Clone)]
pub struct UserApiSettings {
    pub consistency_window_sec: i64,
    pub session_max_duration_sec: i64,
    pub session_max_questions: u32,
    pub leaderboard_top_cache_size: u32,
    pub leaderboard_top_cache_ttl_sec: i64,
    pub leaderboard_old_key_retention_hours: i64,
    pub leaderboard_page_size_max: u32,
    pub leaderboard_periods: Vec<String>,
    pub admin_allowed_cidrs: Vec<String>,
    pub sse_max_connections_per_pod: u32,
    pub pg_pool_size_per_pod: u32,
}

impl UserApiSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            consistency_window_sec: config.get_i64(
                "submit.consistency_window_sec",
                defaults::USER_API_SUBMIT_CONSISTENCY_WINDOW_SEC,
            ),
            session_max_duration_sec: config.get_i64(
                "session.max_duration_sec",
                defaults::USER_API_SESSION_MAX_DURATION_SEC,
            ),
            session_max_questions: config.get_u32(
                "session.max_questions",
                defaults::USER_API_SESSION_MAX_QUESTIONS,
            ),
            leaderboard_top_cache_size: config.get_u32(
                "leaderboard.top_cache_size",
                defaults::USER_API_LEADERBOARD_TOP_CACHE_SIZE,
            ),
            leaderboard_top_cache_ttl_sec: config.get_i64(
                "leaderboard.top_cache_ttl_sec",
                defaults::USER_API_LEADERBOARD_TOP_CACHE_TTL_SEC,
            ),
            leaderboard_old_key_retention_hours: config.get_i64(
                "leaderboard.old_key_retention_hours",
                defaults::USER_API_LEADERBOARD_OLD_KEY_RETENTION_HOURS,
            ),
            leaderboard_page_size_max: config.get_u32(
                "leaderboard.page_size_max",
                defaults::USER_API_LEADERBOARD_PAGE_SIZE_MAX,
            ),
            leaderboard_periods: config.get_list(
                "leaderboard.periods",
                &defaults::USER_API_LEADERBOARD_PERIODS,
            ),
            admin_allowed_cidrs: config.get_list(
                "admin.allowed_cidrs",
                &defaults::USER_API_ADMIN_ALLOWED_CIDRS,
            ),
            sse_max_connections_per_pod: config.get_u32(
                "sse.max_connections_per_pod",
                defaults::USER_API_SSE_MAX_CONNECTIONS_PER_POD,
            ),
            pg_pool_size_per_pod: config.get_u32(
                "pg.pool_size_per_pod",
                defaults::USER_API_PG_POOL_SIZE_PER_POD,
            ),
        }
    }

    pub fn consistency_window(&self) -> Duration {
        Duration::seconds(self.consistency_window_sec)
    }

    pub fn session_max_duration(&self) -> Duration {
        Duration::seconds(self.session_max_duration_sec)
    }

    pub fn top_cache_ttl(&self) -> Duration {
        Duration::seconds(self.leaderboard_top_cache_ttl_sec)
    }
}

#[derive(Debug, Clone)]
pub struct SseSettings {
    pub max_connections_per_pod: u32,
    pub drain_sec: i64,
    pub keepalive_sec: i64,
}

impl SseSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            max_connections_per_pod: config.get_u32(
                "max_connections_per_pod",
                defaults::USER_API_SSE_MAX_CONNECTIONS,
            ),
            drain_sec: config.get_i64("shutdown.drain_sec", defaults::USER_API_SSE_DRAIN_SEC),
            keepalive_sec: config.get_i64("keepalive_sec", defaults::USER_API_SSE_KEEPALIVE_SEC),
        }
    }

    pub fn drain(&self) -> Duration {
        Duration::seconds(self.drain_sec)
    }

    pub fn keepalive(&self) -> Duration {
        Duration::seconds(self.keepalive_sec)
    }
}

#[derive(Debug, Clone)]
pub struct FlusherSettings {
    pub idle_lag_threshold: i64,
    pub idle_qps_threshold: i64,
    pub idle_batch_size: u32,
    pub timer_interval_sec: i64,
    pub timer_batch_size: u32,
    pub emergency_threshold: i64,
    pub max_concurrency: u32,
    pub lock_ttl_sec: i64,
}

impl FlusherSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            idle_lag_threshold: config.get_i64(
                "flusher.idle.lag_threshold",
                defaults::RECOMMENDATION_FLUSHER_IDLE_LAG_THRESHOLD,
            ),
            idle_qps_threshold: config.get_i64(
                "flusher.idle.qps_threshold",
                defaults::RECOMMENDATION_FLUSHER_IDLE_QPS_THRESHOLD,
            ),
            idle_batch_size: config.get_u32(
                "flusher.idle.batch_size",
                defaults::RECOMMENDATION_FLUSHER_IDLE_BATCH_SIZE,
            ),
            timer_interval_sec: config.get_i64(
                "flusher.timer.interval_sec",
                defaults::RECOMMENDATION_FLUSHER_TIMER_INTERVAL_SEC,
            ),
            timer_batch_size: config.get_u32(
                "flusher.timer.batch_size",
                defaults::RECOMMENDATION_FLUSHER_TIMER_BATCH_SIZE,
            ),
            emergency_threshold: config.get_i64(
                "flusher.emergency.threshold",
                defaults::RECOMMENDATION_FLUSHER_EMERGENCY_THRESHOLD,
            ),
            max_concurrency: config.get_u32(
                "flusher.max_concurrency",
                defaults::RECOMMENDATION_FLUSHER_MAX_CONCURRENCY,
            ),
            lock_ttl_sec: config.get_i64(
                "flusher.lock_ttl_sec",
                defaults::RECOMMENDATION_FLUSHER_LOCK_TTL_SEC,
            ),
        }
    }

    pub fn timer_interval(&self) -> Duration {
        Duration::seconds(self.timer_interval_sec)
    }

    pub fn lock_ttl(&self) -> Duration {
        Duration::seconds(self.lock_ttl_sec)
    }
}

#[derive(Debug, Clone)]
pub struct RecommendationSettings {
    pub read_p99_ms: i64,
    pub default_count: u32,
    pub max_count: u32,
    pub chapter_dims: Vec<String>,
    pub delta_wrong: f64,
    pub delta_correct: f64,
    pub pgvector_m: u32,
    pub pgvector_ef_construction: u32,
    pub pg_pool_size_per_pod: u32,
    pub flusher: FlusherSettings,
}

impl RecommendationSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            read_p99_ms: config
                .get_i64("slo.read_p99_ms", defaults::RECOMMENDATION_SLO_READ_P99_MS),
            default_count: config.get_u32(
                "recommend.default_count",
                defaults::RECOMMENDATION_DEFAULT_COUNT,
            ),
            max_count: config.get_u32("recommend.max_count", defaults::RECOMMENDATION_MAX_COUNT),
            chapter_dims: config.get_list(
                "recommend.chapter_dims",
                &defaults::RECOMMENDATION_CHAPTER_DIMS,
            ),
            delta_wrong: config.get_f64(
                "recommend.delta.wrong",
                defaults::RECOMMENDATION_DELTA_WRONG,
            ),
            delta_correct: config.get_f64(
                "recommend.delta.correct",
                defaults::RECOMMENDATION_DELTA_CORRECT,
            ),
            pgvector_m: config.get_u32("pgvector.m", defaults::RECOMMENDATION_PGVECTOR_M),
            pgvector_ef_construction: config.get_u32(
                "pgvector.ef_construction",
                defaults::RECOMMENDATION_PGVECTOR_EF_CONSTRUCTION,
            ),
            pg_pool_size_per_pod: config.get_u32(
                "pg.pool_size_per_pod",
                defaults::RECOMMENDATION_PG_POOL_SIZE_PER_POD,
            ),
            flusher: FlusherSettings::from_config(config),
        }
    }

    pub fn clamped_count(&self, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.default_count)
            .min(self.max_count)
            .max(1)
    }
}

#[derive(Debug, Clone)]
pub struct ReconcileSettings {
    pub interval_sec: i64,
    pub lock_ttl_sec: i64,
    pub batch_size: u32,
    pub stale_threshold_sec: i64,
    pub zset_retention_sec: i64,
}

impl ReconcileSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            interval_sec: config.get_i64(
                "reconcile.interval_sec",
                defaults::BATTLE_RECONCILE_INTERVAL_SEC,
            ),
            lock_ttl_sec: config.get_i64(
                "reconcile.lock_ttl_sec",
                defaults::BATTLE_RECONCILE_LOCK_TTL_SEC,
            ),
            batch_size: config.get_u32(
                "reconcile.batch_size",
                defaults::BATTLE_RECONCILE_BATCH_SIZE,
            ),
            stale_threshold_sec: config.get_i64(
                "reconcile.stale_threshold_sec",
                defaults::BATTLE_RECONCILE_STALE_THRESHOLD_SEC,
            ),
            zset_retention_sec: config.get_i64(
                "reconcile.zset_retention_sec",
                defaults::BATTLE_RECONCILE_ZSET_RETENTION_SEC,
            ),
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::seconds(self.interval_sec)
    }

    pub fn lock_ttl(&self) -> Duration {
        Duration::seconds(self.lock_ttl_sec)
    }
}

#[derive(Debug, Clone)]
pub struct BattleSettings {
    pub questions_per_room: u32,
    pub per_question_sec: i64,
    pub total_sec: i64,
    pub room_ttl_sec: i64,
    pub queue_timeout_sec: i64,
    pub max_rooms_per_pod: u32,
    pub token_refresh_ahead_sec: i64,
    pub token_expired_grace_sec: i64,
    pub revoked_version_poll_sec: i64,
    pub ws_max_connections_per_pod: u32,
    pub ws_ping_interval_sec: i64,
    pub score_correct: i32,
    pub score_wrong: i32,
    pub speed_bonus_max: i32,
    pub reconcile: ReconcileSettings,
}

impl BattleSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            questions_per_room: config.get_u32(
                "match.questions_per_room",
                defaults::BATTLE_MATCH_QUESTIONS_PER_ROOM,
            ),
            per_question_sec: config.get_i64(
                "match.per_question_sec",
                defaults::BATTLE_MATCH_PER_QUESTION_SEC,
            ),
            total_sec: config.get_i64("match.total_sec", defaults::BATTLE_MATCH_TOTAL_SEC),
            room_ttl_sec: config.get_i64("match.room_ttl_sec", defaults::BATTLE_MATCH_ROOM_TTL_SEC),
            queue_timeout_sec: config.get_i64(
                "match.queue_timeout_sec",
                defaults::BATTLE_MATCH_QUEUE_TIMEOUT_SEC,
            ),
            max_rooms_per_pod: config.get_u32(
                "match.max_rooms_per_pod",
                defaults::BATTLE_MATCH_MAX_ROOMS_PER_POD,
            ),
            token_refresh_ahead_sec: config.get_i64(
                "token_refresh_ahead_sec",
                defaults::BATTLE_TOKEN_REFRESH_AHEAD_SEC,
            ),
            token_expired_grace_sec: config.get_i64(
                "token_expired_grace_sec",
                defaults::BATTLE_TOKEN_EXPIRED_GRACE_SEC,
            ),
            revoked_version_poll_sec: config.get_i64(
                "revoked_version_poll_sec",
                defaults::BATTLE_REVOKED_VERSION_POLL_SEC,
            ),
            ws_max_connections_per_pod: config.get_u32(
                "ws.max_connections_per_pod",
                defaults::BATTLE_WS_MAX_CONNECTIONS_PER_POD,
            ),
            ws_ping_interval_sec: config.get_i64(
                "ws.ping_interval_sec",
                defaults::BATTLE_WS_PING_INTERVAL_SEC,
            ),
            score_correct: config.get_i64_as_i32("score.correct", defaults::BATTLE_SCORE_CORRECT),
            score_wrong: config.get_i64_as_i32("score.wrong", defaults::BATTLE_SCORE_WRONG),
            speed_bonus_max: config.get_i64_as_i32(
                "score.speed_bonus_max",
                defaults::BATTLE_SCORE_SPEED_BONUS_MAX,
            ),
            reconcile: ReconcileSettings::from_config(config),
        }
    }

    pub fn per_question_duration(&self) -> Duration {
        Duration::seconds(self.per_question_sec)
    }

    pub fn total_duration(&self) -> Duration {
        Duration::seconds(self.total_sec)
    }

    pub fn room_ttl(&self) -> Duration {
        Duration::seconds(self.room_ttl_sec)
    }

    pub fn queue_timeout(&self) -> Duration {
        Duration::seconds(self.queue_timeout_sec)
    }

    pub fn token_refresh_ahead(&self) -> Duration {
        Duration::seconds(self.token_refresh_ahead_sec)
    }

    pub fn token_expired_grace(&self) -> Duration {
        Duration::seconds(self.token_expired_grace_sec)
    }

    pub fn revoked_version_poll(&self) -> Duration {
        Duration::seconds(self.revoked_version_poll_sec)
    }

    pub fn ping_interval(&self) -> Duration {
        Duration::seconds(self.ws_ping_interval_sec)
    }
}

trait I32FromI64 {
    fn get_i64_as_i32(&self, key: &str, default: i32) -> i32;
}

impl I32FromI64 for ConfigLoader {
    fn get_i64_as_i32(&self, key: &str, default: i32) -> i32 {
        self.get_i64(key, i64::from(default)) as i32
    }
}

#[derive(Debug, Clone)]
pub struct ConsumerSettings {
    pub max_retries: u32,
    pub retry_ttl_sec: i64,
    pub idempotency_ttl_sec: i64,
    pub batch_size: u32,
    pub commit_interval_ms: i64,
    pub offset_reset_policy: String,
    pub keda_lag_threshold: i64,
    pub retry_backoff_ms: i64,
    pub delivery_timeout_ms: i64,
}

impl ConsumerSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            max_retries: config.get_u32("max_retries", defaults::CONSUMERS_MAX_RETRIES),
            retry_ttl_sec: config.get_i64("retry_ttl_sec", defaults::CONSUMERS_RETRY_TTL_SEC),
            idempotency_ttl_sec: config.get_i64(
                "idempotency_ttl_sec",
                defaults::CONSUMERS_IDEMPOTENCY_TTL_SEC,
            ),
            batch_size: config.get_u32("batch_size", defaults::CONSUMERS_BATCH_SIZE),
            commit_interval_ms: config
                .get_i64("commit_interval_ms", defaults::CONSUMERS_COMMIT_INTERVAL_MS),
            offset_reset_policy: config.get_string(
                "offset_reset_policy",
                defaults::CONSUMERS_OFFSET_RESET_POLICY,
            ),
            keda_lag_threshold: config
                .get_i64("keda.lag_threshold", defaults::CONSUMERS_KEDA_LAG_THRESHOLD),
            retry_backoff_ms: config
                .get_i64("retry_backoff_ms", defaults::CONSUMERS_RETRY_BACKOFF_MS),
            delivery_timeout_ms: config.get_i64(
                "delivery_timeout_ms",
                defaults::CONSUMERS_DELIVERY_TIMEOUT_MS,
            ),
        }
    }

    pub fn retry_ttl(&self) -> Duration {
        Duration::seconds(self.retry_ttl_sec)
    }

    pub fn idempotency_ttl(&self) -> Duration {
        Duration::seconds(self.idempotency_ttl_sec)
    }
}

#[derive(Debug, Clone)]
pub struct QbankClientSettings {
    pub grpc_addr: String,
    pub timeout_sec: i64,
}

impl QbankClientSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            grpc_addr: config.get_string("grpc_addr", defaults::QBANK_GRPC_ADDR),
            timeout_sec: config.get_i64("grpc.timeout_sec", defaults::QBANK_GRPC_TIMEOUT_SEC),
        }
    }

    pub fn timeout(&self) -> Duration {
        Duration::seconds(self.timeout_sec)
    }
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub bind_addr: String,
}

impl ServerSettings {
    pub fn from_config(config: &ConfigLoader, fallback: &str) -> Self {
        Self {
            bind_addr: config.get_string("server.bind_addr", fallback),
        }
    }
}

/// PostgreSQL and Redis connection targets for a single service.
///
/// Resolution order for each URL: `<SERVICE>_<NAME>_URL` (for example
/// `AUTH_DATABASE_URL`), then the shared `<NAME>_URL`, then the local
/// development default. Deployments set the scoped variables explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseSettings {
    pub database_url: String,
    pub redis_url: String,
}

impl DatabaseSettings {
    pub fn from_env(namespace: &str) -> Self {
        let prefix = namespace.to_ascii_uppercase().replace('-', "_");
        let database_url = std::env::var(format!("{prefix}_DATABASE_URL"))
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok();
        let redis_url = std::env::var(format!("{prefix}_REDIS_URL"))
            .or_else(|_| std::env::var("REDIS_URL"))
            .ok();
        Self::from_values(database_url, redis_url)
    }

    fn from_values(database_url: Option<String>, redis_url: Option<String>) -> Self {
        Self {
            database_url: database_url.unwrap_or_else(|| defaults::DATABASE_URL.to_string()),
            redis_url: redis_url.unwrap_or_else(|| defaults::REDIS_URL.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigSettings {
    pub write_retry_times: u32,
    pub reconcile_interval_sec: i64,
    pub effective_slo_sec: i64,
    pub snapshot_age_warn_sec: i64,
}

impl ConfigSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            write_retry_times: config
                .get_u32("write_retry_times", defaults::CONFIG_WRITE_RETRY_TIMES),
            reconcile_interval_sec: config.get_i64(
                "reconcile_interval_sec",
                defaults::CONFIG_RECONCILE_INTERVAL_SEC,
            ),
            effective_slo_sec: config
                .get_i64("effective_slo_sec", defaults::CONFIG_EFFECTIVE_SLO_SEC),
            snapshot_age_warn_sec: config.get_i64(
                "snapshot_age_warn_sec",
                defaults::CONFIG_SNAPSHOT_AGE_WARN_SEC,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlertSettings {
    pub auth_refresh_failure_ratio: f64,
    pub frontend_refresh_failure_ratio: f64,
    pub login_failure_ratio: f64,
    pub jwt_reject_ratio: f64,
    pub dlq_message_total: i64,
    pub config_effective_slo_multiplier: f64,
    pub profile_flush_lag_sec: i64,
    pub outbox_pending_count: i64,
    pub consumer_lag_warn: i64,
    pub redis_evicted_keys_warn: i64,
    pub battle_reconcile_pending_warn: i64,
}

impl AlertSettings {
    pub fn from_config(config: &ConfigLoader) -> Self {
        Self {
            auth_refresh_failure_ratio: config.get_f64(
                "auth_refresh_failure_ratio",
                defaults::ALERTS_AUTH_REFRESH_FAILURE_RATIO,
            ),
            frontend_refresh_failure_ratio: config.get_f64(
                "frontend_refresh_failure_ratio",
                defaults::ALERTS_FRONTEND_REFRESH_FAILURE_RATIO,
            ),
            login_failure_ratio: config
                .get_f64("login_failure_ratio", defaults::ALERTS_LOGIN_FAILURE_RATIO),
            jwt_reject_ratio: config.get_f64("jwt_reject_ratio", defaults::ALERTS_JWT_REJECT_RATIO),
            dlq_message_total: config
                .get_i64("dlq_message_total", defaults::ALERTS_DLQ_MESSAGE_TOTAL),
            config_effective_slo_multiplier: config.get_f64(
                "config_effective_slo_multiplier",
                defaults::ALERTS_CONFIG_EFFECTIVE_SLO_MULTIPLIER,
            ),
            profile_flush_lag_sec: config.get_i64(
                "profile_flush_lag_sec",
                defaults::ALERTS_PROFILE_FLUSH_LAG_SEC,
            ),
            outbox_pending_count: config.get_i64(
                "outbox_pending_count",
                defaults::ALERTS_OUTBOX_PENDING_COUNT,
            ),
            consumer_lag_warn: config
                .get_i64("consumer_lag_warn", defaults::ALERTS_CONSUMER_LAG_WARN),
            redis_evicted_keys_warn: config.get_i64(
                "redis_evicted_keys_warn",
                defaults::ALERTS_REDIS_EVICTED_KEYS_WARN,
            ),
            battle_reconcile_pending_warn: config.get_i64(
                "battle_reconcile_pending_warn",
                defaults::ALERTS_BATTLE_RECONCILE_PENDING_WARN,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn loader(namespace: &str, pairs: &[(&str, &str)]) -> ConfigLoader {
        let env = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<HashMap<_, _>>();
        ConfigLoader::new(namespace, env)
    }

    #[test]
    fn auth_settings_read_env_and_fall_back() {
        let loader = loader(
            "auth",
            &[
                ("AUTH_JWT_ACCESS_TTL_SEC", "1200"),
                ("AUTH_REFRESH_GRACE_SEC", "7"),
            ],
        );
        let settings = AuthSettings::from_config(&loader);
        assert_eq!(settings.access_ttl_sec, 1200);
        assert_eq!(settings.refresh_grace_sec, 7);
        assert_eq!(settings.access_ttl(), Duration::seconds(1200));
        assert_eq!(settings.refresh_grace(), Duration::seconds(7));
        assert_eq!(
            settings.refresh_ttl_days,
            defaults::AUTH_JWT_REFRESH_TTL_DAYS
        );
        assert!(settings.password_min_length >= 1);
        assert_eq!(settings.cookie_name, defaults::AUTH_COOKIE_NAME);
    }

    #[test]
    fn user_api_settings_cap_page_size_and_cidrs() {
        let settings = UserApiSettings::from_config(&loader("user_api", &[]));
        assert_eq!(
            settings.leaderboard_page_size_max,
            defaults::USER_API_LEADERBOARD_PAGE_SIZE_MAX
        );
        assert_eq!(
            settings.admin_allowed_cidrs,
            defaults::USER_API_ADMIN_ALLOWED_CIDRS
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            settings.leaderboard_periods,
            defaults::USER_API_LEADERBOARD_PERIODS
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            settings.consistency_window(),
            Duration::seconds(defaults::USER_API_SUBMIT_CONSISTENCY_WINDOW_SEC)
        );
    }

    #[test]
    fn recommendation_count_is_clamped() {
        let settings = RecommendationSettings::from_config(&loader("recommendation", &[]));
        assert_eq!(
            settings.clamped_count(None),
            defaults::RECOMMENDATION_DEFAULT_COUNT
        );
        assert_eq!(settings.clamped_count(Some(0)), 1);
        assert_eq!(
            settings.clamped_count(Some(defaults::RECOMMENDATION_MAX_COUNT + 10)),
            defaults::RECOMMENDATION_MAX_COUNT
        );
    }

    #[test]
    fn battle_score_settings_are_signed() {
        let settings = BattleSettings::from_config(&loader("battle", &[]));
        assert_eq!(settings.score_wrong, defaults::BATTLE_SCORE_WRONG);
        assert!(settings.score_wrong < 0);
        assert!(settings.speed_bonus_max > 0);
        assert_eq!(settings.total_sec, defaults::BATTLE_MATCH_TOTAL_SEC);
    }

    #[test]
    fn database_settings_fall_back_to_local_defaults() {
        let settings = DatabaseSettings::from_values(None, None);
        assert_eq!(settings.database_url, defaults::DATABASE_URL);
        assert_eq!(settings.redis_url, defaults::REDIS_URL);

        let settings = DatabaseSettings::from_values(
            Some("postgres://example/db".to_string()),
            Some("redis://example:6379/1".to_string()),
        );
        assert_eq!(settings.database_url, "postgres://example/db");
        assert_eq!(settings.redis_url, "redis://example:6379/1");
    }

    #[test]
    fn consumer_settings_expose_retry_policy() {
        let settings = ConsumerSettings::from_config(&loader("consumers", &[]));
        assert_eq!(settings.max_retries, defaults::CONSUMERS_MAX_RETRIES);
        assert_eq!(
            settings.offset_reset_policy,
            defaults::CONSUMERS_OFFSET_RESET_POLICY
        );
    }
}
