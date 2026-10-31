//! Local battle settings. Values come from `BATTLE_*` environment variables and
//! fall back to the defaults required by the battle service contract.

use std::env;

use chrono::Duration;

pub const DEFAULT_PORT: u16 = 8087;
pub const DEFAULT_QUESTIONS_PER_ROOM: u32 = 5;
pub const DEFAULT_PER_QUESTION_SEC: i64 = 10;
pub const DEFAULT_TOTAL_SEC: i64 = 120;
pub const DEFAULT_QUEUE_TIMEOUT_SEC: i64 = 15;
pub const DEFAULT_SCORE_CORRECT: i32 = 100;
pub const DEFAULT_SCORE_WRONG: i32 = 0;
pub const DEFAULT_SPEED_BONUS_MAX: i32 = 50;
pub const DEFAULT_ROOM_TTL_SEC: i64 = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub port: u16,
    pub questions_per_room: u32,
    pub per_question_sec: i64,
    pub total_sec: i64,
    pub queue_timeout_sec: i64,
    pub score_correct: i32,
    pub score_wrong: i32,
    pub speed_bonus_max: i32,
    pub room_ttl_sec: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            questions_per_room: DEFAULT_QUESTIONS_PER_ROOM,
            per_question_sec: DEFAULT_PER_QUESTION_SEC,
            total_sec: DEFAULT_TOTAL_SEC,
            queue_timeout_sec: DEFAULT_QUEUE_TIMEOUT_SEC,
            score_correct: DEFAULT_SCORE_CORRECT,
            score_wrong: DEFAULT_SCORE_WRONG,
            speed_bonus_max: DEFAULT_SPEED_BONUS_MAX,
            room_ttl_sec: DEFAULT_ROOM_TTL_SEC,
        }
    }
}

impl Settings {
    pub fn from_env() -> Self {
        let mut settings = Self::default();
        if let Some(port) = env_u16("BATTLE_PORT").or_else(|| env_u16("PORT")) {
            settings.port = port;
        }
        settings.questions_per_room = env_u32(
            "BATTLE_MATCH_QUESTIONS_PER_ROOM",
            settings.questions_per_room,
        );
        settings.per_question_sec =
            env_i64("BATTLE_MATCH_PER_QUESTION_SEC", settings.per_question_sec);
        settings.total_sec = env_i64("BATTLE_MATCH_TOTAL_SEC", settings.total_sec);
        settings.queue_timeout_sec =
            env_i64("BATTLE_MATCH_QUEUE_TIMEOUT_SEC", settings.queue_timeout_sec);
        settings.score_correct = env_i32("BATTLE_SCORE_CORRECT", settings.score_correct);
        settings.score_wrong = env_i32("BATTLE_SCORE_WRONG", settings.score_wrong);
        settings.speed_bonus_max =
            env_i32("BATTLE_SCORE_SPEED_BONUS_MAX", settings.speed_bonus_max);
        settings.room_ttl_sec = env_i64("BATTLE_MATCH_ROOM_TTL_SEC", settings.room_ttl_sec);
        settings
    }

    pub fn bind_addr(&self) -> String {
        env::var("BATTLE_BIND_ADDR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("0.0.0.0:{}", self.port))
    }

    pub fn per_question_duration(&self) -> Duration {
        Duration::seconds(self.per_question_sec.max(1))
    }

    pub fn total_duration(&self) -> Duration {
        Duration::seconds(self.total_sec.max(1))
    }

    pub fn queue_timeout(&self) -> Duration {
        Duration::seconds(self.queue_timeout_sec.max(1))
    }

    pub fn room_ttl(&self) -> Duration {
        Duration::seconds(self.room_ttl_sec.max(1))
    }
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_i32(key: &str, default: i32) -> i32 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str) -> Option<u16> {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_service_contract() {
        let settings = Settings::default();
        assert_eq!(settings.port, DEFAULT_PORT);
        assert_eq!(settings.questions_per_room, DEFAULT_QUESTIONS_PER_ROOM);
        assert_eq!(settings.per_question_sec, DEFAULT_PER_QUESTION_SEC);
        assert_eq!(settings.total_sec, DEFAULT_TOTAL_SEC);
        assert_eq!(settings.queue_timeout_sec, DEFAULT_QUEUE_TIMEOUT_SEC);
        assert_eq!(settings.score_correct, DEFAULT_SCORE_CORRECT);
        assert_eq!(settings.score_wrong, DEFAULT_SCORE_WRONG);
        assert_eq!(settings.speed_bonus_max, DEFAULT_SPEED_BONUS_MAX);
        assert_eq!(settings.room_ttl_sec, DEFAULT_ROOM_TTL_SEC);
        assert_eq!(settings.per_question_duration(), Duration::seconds(10));
        assert_eq!(settings.total_duration(), Duration::seconds(120));
    }
}
