//! Profile storage abstractions: read-time merge source, in-memory storage,
//! user-level locks and the profile flusher.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::clock::Clock;
use common::error::{AppError, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::model::{AttemptStats, ChapterVector};

/// Source of the persisted baseline vector and the hot increments.
///
/// Production implementations read from PG and Redis respectively. Only
/// `load_baseline` and `load_increments` are required; the remaining methods
/// have safe defaults so read-only sources can still be used.
#[async_trait]
pub trait ProfileSource: Send + Sync {
    async fn load_baseline(&self, user_id: Uuid) -> Result<ChapterVector>;
    async fn load_increments(&self, user_id: Uuid) -> Result<ChapterVector>;

    async fn drain_increments(&self, _user_id: Uuid) -> Result<ChapterVector> {
        Err(AppError::unavailable(
            "profile source does not support draining increments",
        ))
    }

    async fn store_baseline(&self, _user_id: Uuid, _vector: ChapterVector) -> Result<()> {
        Err(AppError::unavailable(
            "profile source does not support baseline updates",
        ))
    }

    async fn updated_at(&self, _user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        Ok(None)
    }

    async fn attempt_stats(&self, _user_id: Uuid) -> Result<AttemptStats> {
        Ok(AttemptStats::default())
    }

    async fn record_attempt(
        &self,
        _user_id: Uuid,
        _chapter_id: &str,
        _correct: bool,
        _delta_wrong: f64,
        _delta_correct: f64,
    ) -> Result<()> {
        Err(AppError::unavailable(
            "profile source does not support attempt recording",
        ))
    }
}

#[derive(Debug, Clone)]
struct ProfileEntry {
    baseline: ChapterVector,
    increments: ChapterVector,
    total_attempts: u64,
    correct_attempts: u64,
    chapter_attempts: HashMap<String, u64>,
    updated_at: Option<DateTime<Utc>>,
}

impl ProfileEntry {
    fn new(dims: &[String]) -> Self {
        Self {
            baseline: ChapterVector::zero(dims.to_vec()),
            increments: ChapterVector::zero(dims.to_vec()),
            total_attempts: 0,
            correct_attempts: 0,
            chapter_attempts: HashMap::new(),
            updated_at: None,
        }
    }

    fn stats(&self) -> AttemptStats {
        AttemptStats {
            total_attempts: self.total_attempts,
            correct_attempts: self.correct_attempts,
            chapter_attempts: self.chapter_attempts.clone(),
        }
    }
}

/// In-memory implementation used by tests and local development.
#[derive(Debug)]
pub struct InMemoryProfileSource {
    dims: Vec<String>,
    profiles: Mutex<HashMap<Uuid, ProfileEntry>>,
}

impl InMemoryProfileSource {
    pub fn new(dims: Vec<String>) -> Self {
        Self {
            dims,
            profiles: Mutex::new(HashMap::new()),
        }
    }

    fn profiles(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, ProfileEntry>> {
        self.profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn ensure_entry<'a>(
        map: &'a mut HashMap<Uuid, ProfileEntry>,
        dims: &[String],
        user_id: Uuid,
    ) -> &'a mut ProfileEntry {
        map.entry(user_id)
            .or_insert_with(|| ProfileEntry::new(dims))
    }
}

#[async_trait]
impl ProfileSource for InMemoryProfileSource {
    async fn load_baseline(&self, user_id: Uuid) -> Result<ChapterVector> {
        let profiles = self.profiles();
        Ok(profiles
            .get(&user_id)
            .map(|entry| entry.baseline.clone())
            .unwrap_or_else(|| ChapterVector::zero(self.dims.clone())))
    }

    async fn load_increments(&self, user_id: Uuid) -> Result<ChapterVector> {
        let profiles = self.profiles();
        Ok(profiles
            .get(&user_id)
            .map(|entry| entry.increments.clone())
            .unwrap_or_else(|| ChapterVector::zero(self.dims.clone())))
    }

    async fn drain_increments(&self, user_id: Uuid) -> Result<ChapterVector> {
        let mut profiles = self.profiles();
        let entry = Self::ensure_entry(&mut profiles, &self.dims, user_id);
        let drained = entry.increments.clone();
        entry.increments = ChapterVector::zero(self.dims.clone());
        Ok(drained)
    }

    async fn store_baseline(&self, user_id: Uuid, vector: ChapterVector) -> Result<()> {
        let mut profiles = self.profiles();
        let entry = Self::ensure_entry(&mut profiles, &self.dims, user_id);
        entry.baseline = vector;
        entry.updated_at = Some(Utc::now());
        Ok(())
    }

    async fn updated_at(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        let profiles = self.profiles();
        Ok(profiles.get(&user_id).and_then(|entry| entry.updated_at))
    }

    async fn attempt_stats(&self, user_id: Uuid) -> Result<AttemptStats> {
        let profiles = self.profiles();
        Ok(profiles
            .get(&user_id)
            .map(ProfileEntry::stats)
            .unwrap_or_default())
    }

    async fn record_attempt(
        &self,
        user_id: Uuid,
        chapter_id: &str,
        correct: bool,
        delta_wrong: f64,
        delta_correct: f64,
    ) -> Result<()> {
        let mut profiles = self.profiles();
        let entry = Self::ensure_entry(&mut profiles, &self.dims, user_id);
        let delta = if correct { -delta_correct } else { delta_wrong };
        if !entry.increments.add(chapter_id, delta) {
            return Err(AppError::invalid(format!("unknown chapter {chapter_id}")));
        }
        entry.total_attempts += 1;
        if correct {
            entry.correct_attempts += 1;
        }
        *entry
            .chapter_attempts
            .entry(chapter_id.to_string())
            .or_insert(0) += 1;
        entry.updated_at = Some(Utc::now());
        Ok(())
    }
}

/// PostgreSQL-backed read-only source. Stats come from `app.attempts` and the
/// baseline vector comes from `app.user_vectors`, so a restart never loses the
/// numbers the UI shows.
pub struct PgProfileSource {
    pool: PgPool,
    dims: Vec<String>,
}

impl PgProfileSource {
    pub async fn connect(database_url: &str, dims: Vec<String>) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|error| AppError::internal(format!("connect profile store: {error}")))?;
        Ok(Self { pool, dims })
    }
}

/// Collapses `chapter_id:difficulty` dimensions onto the configured chapter set.
fn chapter_vector(config_dims: &[String], dims: &[String], weights: &[f64]) -> ChapterVector {
    let mut vector = ChapterVector::zero(config_dims.to_vec());
    for (dim, weight) in dims.iter().zip(weights.iter()) {
        let chapter = dim
            .split_once(':')
            .map(|(chapter, _)| chapter)
            .unwrap_or(dim);
        vector.add(chapter, *weight);
    }
    vector
}

#[async_trait]
impl ProfileSource for PgProfileSource {
    async fn load_baseline(&self, user_id: Uuid) -> Result<ChapterVector> {
        match sqlx::query_as::<_, (Vec<String>, Vec<f64>)>(
            "SELECT dims, weights FROM app.user_vectors WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(Some((dims, weights))) => Ok(chapter_vector(&self.dims, &dims, &weights)),
            Ok(None) => Ok(ChapterVector::zero(self.dims.clone())),
            Err(error) => {
                tracing::warn!(%error, user_id = %user_id, "load user vector failed");
                Ok(ChapterVector::zero(self.dims.clone()))
            }
        }
    }

    async fn load_increments(&self, _user_id: Uuid) -> Result<ChapterVector> {
        Ok(ChapterVector::zero(self.dims.clone()))
    }

    async fn drain_increments(&self, _user_id: Uuid) -> Result<ChapterVector> {
        Ok(ChapterVector::zero(self.dims.clone()))
    }

    async fn updated_at(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        let row: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT updated_at FROM app.user_vectors WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| AppError::internal(format!("load vector timestamp: {error}")))?;
        Ok(row.map(|(updated_at,)| updated_at))
    }

    async fn attempt_stats(&self, user_id: Uuid) -> Result<AttemptStats> {
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT count(*) AS total, \
                    count(*) FILTER (WHERE is_correct) AS correct \
             FROM app.attempts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load attempt stats: {error}")))?;
        let chapters: Vec<(String, i64)> = sqlx::query_as(
            "SELECT q.chapter_id, count(*) \
             FROM app.attempts a \
             JOIN question.questions q ON q.id = a.question_id \
             WHERE a.user_id = $1 \
             GROUP BY q.chapter_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load chapter attempts: {error}")))?;
        Ok(AttemptStats {
            total_attempts: u64::try_from(counts.0).unwrap_or(0),
            correct_attempts: u64::try_from(counts.1).unwrap_or(0),
            chapter_attempts: chapters
                .into_iter()
                .map(|(chapter_id, count)| (chapter_id, u64::try_from(count).unwrap_or(0)))
                .collect(),
        })
    }
}

/// In-memory per-user lock map used to serialize flushes inside one pod.
#[derive(Debug)]
pub struct UserLocks {
    entries: Mutex<HashMap<Uuid, Instant>>,
    ttl: Duration,
}

impl UserLocks {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    fn try_acquire(&self, user_id: Uuid) -> bool {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        if let Some(acquired_at) = entries.get(&user_id) {
            if now.duration_since(*acquired_at) < self.ttl {
                return false;
            }
        }
        entries.insert(user_id, now);
        true
    }

    fn release(&self, user_id: Uuid) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.remove(&user_id);
    }
}

/// Flusher abstraction: drain increments, merge with the baseline, normalize
/// and persist the new baseline.
#[async_trait]
pub trait Flusher: Send + Sync {
    async fn flush_user(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>>;
}

#[derive(Clone)]
pub struct VectorFlusher {
    source: Arc<dyn ProfileSource>,
    clock: Arc<dyn Clock>,
    locks: Arc<UserLocks>,
}

impl VectorFlusher {
    pub fn new(source: Arc<dyn ProfileSource>, clock: Arc<dyn Clock>, lock_ttl: Duration) -> Self {
        Self {
            source,
            clock,
            locks: Arc::new(UserLocks::new(lock_ttl)),
        }
    }

    async fn flush_locked(&self, user_id: Uuid) -> Result<DateTime<Utc>> {
        // Drain first: losing a batch after the Redis clear is acceptable.
        let increments = self.source.drain_increments(user_id).await?;
        let baseline = self.source.load_baseline(user_id).await?;
        let merged = baseline.merge(&increments).normalized();
        self.source.store_baseline(user_id, merged).await?;
        Ok(self.clock.now())
    }
}

#[async_trait]
impl Flusher for VectorFlusher {
    async fn flush_user(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        if !self.locks.try_acquire(user_id) {
            return Ok(None);
        }
        let result = self.flush_locked(user_id).await;
        self.locks.release(user_id);
        result.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use common::clock::FakeClock;

    fn dims() -> Vec<String> {
        vec!["mechanics".to_string(), "optics".to_string()]
    }

    fn source() -> InMemoryProfileSource {
        InMemoryProfileSource::new(dims())
    }

    fn user() -> Uuid {
        Uuid::from_u128(7)
    }

    #[tokio::test]
    async fn reads_return_zero_vectors_for_unknown_users() {
        let source = source();
        let baseline = source.load_baseline(user()).await.unwrap();
        let increments = source.load_increments(user()).await.unwrap();
        assert_eq!(baseline, ChapterVector::zero(dims()));
        assert_eq!(increments, ChapterVector::zero(dims()));
        assert_eq!(source.updated_at(user()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn record_attempt_applies_wrong_delta_and_correct_decay() {
        let source = source();
        source
            .record_attempt(user(), "mechanics", false, 1.0, 0.2)
            .await
            .unwrap();
        source
            .record_attempt(user(), "mechanics", true, 1.0, 0.2)
            .await
            .unwrap();
        let increments = source.load_increments(user()).await.unwrap();
        assert!((increments.get("mechanics") - 0.8).abs() < 1e-9);
        let stats = source.attempt_stats(user()).await.unwrap();
        assert_eq!(stats.total_attempts, 2);
        assert_eq!(stats.correct_attempts, 1);
        assert_eq!(stats.correct_rate(), 0.5);
        assert_eq!(stats.chapter_attempts.get("mechanics"), Some(&2));
    }

    #[tokio::test]
    async fn record_attempt_rejects_unknown_chapters() {
        let source = source();
        let error = source
            .record_attempt(user(), "unknown", false, 1.0, 0.2)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request");
    }

    #[tokio::test]
    async fn drain_removes_increments_atomically() {
        let source = source();
        source
            .record_attempt(user(), "optics", false, 1.0, 0.2)
            .await
            .unwrap();
        let drained = source.drain_increments(user()).await.unwrap();
        assert!((drained.get("optics") - 1.0).abs() < 1e-9);
        let after = source.load_increments(user()).await.unwrap();
        assert_eq!(after, ChapterVector::zero(dims()));
    }

    #[tokio::test]
    async fn flusher_merges_increments_into_normalized_baseline() {
        let source = Arc::new(source());
        source
            .record_attempt(user(), "mechanics", false, 1.0, 0.2)
            .await
            .unwrap();
        source
            .record_attempt(user(), "optics", false, 1.0, 0.2)
            .await
            .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let clock = Arc::new(FakeClock::new(now));
        let flusher = VectorFlusher::new(source.clone(), clock, Duration::from_secs(30));

        let updated_at = flusher.flush_user(user()).await.unwrap().unwrap();
        assert_eq!(updated_at, now);
        let baseline = source.load_baseline(user()).await.unwrap();
        assert!((baseline.l2_norm() - 1.0).abs() < 1e-9);
        assert!((baseline.get("mechanics") - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        let increments = source.load_increments(user()).await.unwrap();
        assert_eq!(increments, ChapterVector::zero(dims()));
    }

    #[test]
    fn user_lock_rejects_a_second_holder_until_released() {
        let locks = UserLocks::new(Duration::from_secs(30));
        assert!(locks.try_acquire(user()));
        assert!(!locks.try_acquire(user()));
        locks.release(user());
        assert!(locks.try_acquire(user()));
    }

    #[test]
    fn expired_lock_can_be_reacquired() {
        let locks = UserLocks::new(Duration::from_millis(0));
        assert!(locks.try_acquire(user()));
        assert!(locks.try_acquire(user()));
    }
}
