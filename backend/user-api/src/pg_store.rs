//! PostgreSQL and Redis backed domain store.
//!
//! PostgreSQL is the source of truth for profiles, quiz sessions, attempts,
//! questions and runtime configuration. Redis mirrors config snapshots and
//! holds the daily, weekly and all-time leaderboard sorted sets.

use std::collections::HashMap;

use async_graphql::ID;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::error::{AppError, Result};
use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::store::{
    Chapter, ConfigAuditEntry, ConfigEntry, ConfigMutationPayload, ConfigSchemaEntry, DomainStore,
    LeaderboardEntry, LeaderboardPage, OptionItem, Period, Question, QuestionContent,
    QuestionVersion, QuizMode, QuizSession, RankSummary, SessionStatus, SubmitAttemptPayload,
    UserView,
};

type UserRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Vec<String>,
    DateTime<Utc>,
);

type SessionRow = (
    Uuid,
    String,
    Option<String>,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
    redis: ConnectionManager,
}

impl PgStore {
    pub fn new(pool: PgPool, redis: ConnectionManager) -> Self {
        Self { pool, redis }
    }

    /// Inserts the eight chapters and their 24 sample questions when the
    /// question bank is empty.
    pub async fn seed_if_empty(&self) -> Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM question.chapters")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| db_error("count chapters", error))?;
        if count > 0 {
            return Ok(());
        }
        let now = Utc::now();
        let chapters = chapter_seed();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin seed: {error}")))?;
        for (position, (chapter_id, name)) in chapters.iter().enumerate() {
            sqlx::query(
                "INSERT INTO question.chapters (id, name, position) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(*chapter_id)
            .bind(*name)
            .bind(position as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("seed chapter", error))?;
            for index in 1..=3u32 {
                let question_id = format!("{chapter_id}-{index}");
                let content = seed_content(name, index);
                let answer = option_id(index);
                sqlx::query(
                    "INSERT INTO question.questions \
                     (id, chapter_id, difficulty, stem, image_url, explanation, options, \
                      answer_option_id, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $8) \
                     ON CONFLICT (id) DO NOTHING",
                )
                .bind(&question_id)
                .bind(*chapter_id)
                .bind(((index - 1) as i32 % 3) + 1)
                .bind(content["stem"].as_str().unwrap_or_default())
                .bind(content["explanation"].as_str())
                .bind(&content["options"])
                .bind(&answer)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|error| db_error("seed question", error))?;
            }
        }
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit seed: {error}")))?;
        Ok(())
    }

    async fn profile_names(&self, user_ids: &[Uuid]) -> Result<HashMap<Uuid, UserView>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT u.id, u.email, \
                    COALESCE(NULLIF(p.nickname, ''), u.nickname) AS nickname, \
                    COALESCE(p.avatar_url, u.avatar_url) AS avatar_url, \
                    COALESCE(p.roles, u.roles) AS roles, \
                    u.created_at \
             FROM auth.users u \
             LEFT JOIN app.user_profiles p ON p.user_id = u.id \
             WHERE u.id = ANY($1)",
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("load profiles", error))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let profile = user_from_row(row);
                (profile.id, profile)
            })
            .collect())
    }

    async fn sync_config_namespace(&self, namespace: &str) {
        let Ok(entries) = self.configs(namespace).await else {
            return;
        };
        let snapshot = json!({
            "revision": self.config_revision().await.unwrap_or(0),
            "entries": entries
                .iter()
                .map(|entry| json!({
                    "namespace": entry.namespace,
                    "key": entry.key,
                    "value": entry.value,
                }))
                .collect::<Vec<_>>(),
        });
        let mut connection = self.redis.clone();
        let key = format!("config:{namespace}");
        let result: redis::RedisResult<()> = redis::cmd("SET")
            .arg(key)
            .arg(snapshot.to_string())
            .query_async(&mut connection)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "failed to mirror config snapshot");
        }
    }

    async fn config_revision(&self) -> Result<i64> {
        sqlx::query_scalar("SELECT count(*) FROM config.config_audit")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| db_error("count config audit", error))
    }

    async fn score_attempt(&self, user_id: Uuid, now: DateTime<Utc>) {
        let mut connection = self.redis.clone();
        let keys = [
            leaderboard_key(Period::Daily, now),
            leaderboard_key(Period::Weekly, now),
            leaderboard_key(Period::All, now),
        ];
        let mut pipeline = redis::pipe();
        for key in keys {
            pipeline.zincr(key, user_id.to_string(), 100);
        }
        let result: redis::RedisResult<()> = pipeline.query_async(&mut connection).await;
        if let Err(error) = result {
            tracing::warn!(%error, "failed to update leaderboard");
        }
    }
}

fn leaderboard_key(period: Period, now: DateTime<Utc>) -> String {
    match period {
        Period::Daily => format!("leaderboard:daily:{}", now.format("%Y-%m-%d")),
        Period::Weekly => format!("leaderboard:weekly:{}", now.format("%G-W%V")),
        Period::All => "leaderboard:all".to_string(),
    }
}

fn db_error(context: &str, error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref database) = error {
        if database.code().as_deref() == Some("23505") {
            return AppError::conflict("record already exists");
        }
    }
    AppError::internal(format!("{context}: {error}"))
}

fn user_from_row(row: UserRow) -> UserView {
    let (id, email, nickname, avatar_url, roles, created_at) = row;
    UserView {
        id,
        email,
        nickname,
        avatar_url,
        roles,
        created_at,
    }
}

fn status_from_str(value: &str) -> SessionStatus {
    SessionStatus::from_db_str(value).unwrap_or(SessionStatus::Abandoned)
}

fn options_from_value(value: &Value) -> Vec<OptionItem> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| OptionItem {
                    id: ID::from(item["id"].as_str().unwrap_or_default()),
                    label: item["label"].as_str().unwrap_or_default().to_string(),
                    text: item["text"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn content_to_value(content: &QuestionContent) -> Value {
    json!({
        "stem": &content.stem,
        "image_url": &content.image_url,
        "explanation": &content.explanation,
        "options": content
            .options
            .iter()
            .map(|option| json!({
                "id": option.id.to_string(),
                "label": &option.label,
                "text": &option.text,
            }))
            .collect::<Vec<_>>(),
    })
}

fn session_from_row(row: SessionRow) -> QuizSession {
    let (id, mode, chapter_id, status, current_index, total_questions, started_at, completed_at) =
        row;
    QuizSession {
        id,
        mode: QuizMode::from_db_str(&mode).unwrap_or(QuizMode::Practice),
        chapter_id: chapter_id.map(ID::from),
        status: status_from_str(&status),
        current_index,
        total_questions,
        started_at,
        completed_at,
    }
}

fn question_ids_from_value(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl DomainStore for PgStore {
    async fn chapters(&self) -> Result<Vec<Chapter>> {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT c.id, c.name, count(q.id) FILTER (WHERE q.deleted_at IS NULL) \
             FROM question.chapters c \
             LEFT JOIN question.questions q ON q.chapter_id = c.id \
             GROUP BY c.id, c.name, c.position ORDER BY c.position",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("list chapters", error))?;
        Ok(rows
            .into_iter()
            .map(|(id, name, count)| Chapter {
                id: ID::from(id),
                name,
                question_count: count as i32,
            })
            .collect())
    }

    async fn question(&self, id: &str) -> Result<Option<Question>> {
        let row: Option<(
            String,
            String,
            i32,
            String,
            Option<String>,
            Option<String>,
            Value,
        )> = sqlx::query_as(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options \
                 FROM question.questions WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load question", error))?;
        Ok(row.map(
            |(id, chapter_id, difficulty, stem, image_url, explanation, options)| Question {
                id: ID::from(id),
                chapter_id,
                difficulty,
                content: QuestionContent {
                    stem,
                    image_url,
                    explanation,
                    options: options_from_value(&options),
                },
            },
        ))
    }

    async fn profile(
        &self,
        user_id: Uuid,
        email: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<UserView> {
        let email = email
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("{user_id}@physics-world.local"));
        sqlx::query(
            "INSERT INTO app.user_profiles \
             (user_id, email, nickname, avatar_url, roles, created_at, updated_at) \
             VALUES ($1, $2, 'player', NULL, ARRAY['student']::text[], $3, $3) \
             ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(email)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| db_error("ensure profile", error))?;
        let row: UserRow = sqlx::query_as(
            "SELECT user_id, email, nickname, avatar_url, roles, created_at \
             FROM app.user_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| db_error("load profile", error))?;
        Ok(user_from_row(row))
    }

    async fn update_profile(
        &self,
        user_id: Uuid,
        email: Option<&str>,
        nickname: Option<String>,
        avatar_url: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<UserView> {
        let email = email
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("{user_id}@physics-world.local"));
        let nickname = nickname
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let row: UserRow = sqlx::query_as(
            "INSERT INTO app.user_profiles \
             (user_id, email, nickname, avatar_url, roles, created_at, updated_at) \
             VALUES ($1, $2, COALESCE($3, 'player'), $4, ARRAY['student']::text[], $5, $5) \
             ON CONFLICT (user_id) DO UPDATE SET \
               nickname = COALESCE($3, app.user_profiles.nickname), \
               avatar_url = COALESCE($4, app.user_profiles.avatar_url), \
               updated_at = $5 \
             RETURNING user_id, email, nickname, avatar_url, roles, created_at",
        )
        .bind(user_id)
        .bind(email)
        .bind(nickname)
        .bind(avatar_url)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| db_error("update profile", error))?;
        Ok(user_from_row(row))
    }

    async fn start_session(
        &self,
        user_id: Uuid,
        mode: QuizMode,
        chapter_id: Option<String>,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<QuizSession> {
        if mode == QuizMode::Chapter && chapter_id.is_none() {
            return Err(AppError::invalid("chapterId is required for CHAPTER mode"));
        }
        if let Some(chapter) = chapter_id.as_deref() {
            let exists: i64 =
                sqlx::query_scalar("SELECT count(*) FROM question.chapters WHERE id = $1")
                    .bind(chapter)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|error| db_error("load chapter", error))?;
            if exists == 0 {
                return Err(AppError::not_found("chapter not found"));
            }
        }
        let question_ids: Vec<String> = match chapter_id.as_deref() {
            Some(chapter) => sqlx::query_scalar(
                "SELECT q.id FROM question.questions q \
                 JOIN question.chapters c ON c.id = q.chapter_id \
                 WHERE q.deleted_at IS NULL AND q.chapter_id = $2 \
                 ORDER BY q.difficulty, q.id LIMIT $1",
            )
            .bind(i64::from(limit.max(1)))
            .bind(chapter)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| db_error("select session questions", error))?,
            // PRACTICE mode: surface the dimensions with the highest error
            // weight first, then shuffle within the same weight.
            None => sqlx::query_scalar(
                "SELECT q.id FROM question.questions q \
                 LEFT JOIN app.user_vectors uv ON uv.user_id = $2 \
                 WHERE q.deleted_at IS NULL \
                 ORDER BY coalesce( \
                   uv.weights[array_position(uv.dims, q.chapter_id || ':' || q.difficulty)], 0 \
                 ) DESC, random() LIMIT $1",
            )
            .bind(i64::from(limit.max(1)))
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| db_error("select practice questions", error))?,
        };
        if question_ids.is_empty() {
            return Err(AppError::not_found("no questions available"));
        }
        let session = QuizSession {
            id: Uuid::new_v4(),
            mode,
            chapter_id: chapter_id.as_ref().map(|id| ID::from(id.as_str())),
            status: SessionStatus::Active,
            current_index: 0,
            total_questions: question_ids.len() as i32,
            started_at: now,
            completed_at: None,
        };
        let question_ids_json = serde_json::to_value(&question_ids)
            .map_err(|error| AppError::internal(format!("encode question ids: {error}")))?;
        sqlx::query(
            "INSERT INTO app.quiz_sessions \
             (id, user_id, mode, chapter_id, status, current_index, total_questions, \
              question_ids, started_at, completed_at) \
             VALUES ($1, $2, $3, $4, $5, 0, $6, $7, $8, NULL)",
        )
        .bind(session.id)
        .bind(user_id)
        .bind(mode.as_db_str())
        .bind(chapter_id.as_deref())
        .bind(SessionStatus::Active.as_db_str())
        .bind(session.total_questions)
        .bind(question_ids_json)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| db_error("insert session", error))?;
        Ok(session)
    }

    async fn session(&self, id: Uuid, user_id: Uuid) -> Result<Option<QuizSession>> {
        let row: Option<SessionRow> = sqlx::query_as(
            "SELECT id, mode, chapter_id, status, current_index, total_questions, \
             started_at, completed_at \
             FROM app.quiz_sessions WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load session", error))?;
        Ok(row.map(session_from_row))
    }

    async fn question_in_session(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        index: i32,
    ) -> Result<Question> {
        let row: Option<(Value,)> = sqlx::query_as(
            "SELECT question_ids FROM app.quiz_sessions WHERE id = $1 AND user_id = $2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load session", error))?;
        let (question_ids,) = row.ok_or_else(|| AppError::not_found("session not found"))?;
        let ids = question_ids_from_value(&question_ids);
        let question_id = ids
            .get(index.max(0) as usize)
            .ok_or_else(|| AppError::invalid("index out of range"))?;
        self.question(question_id)
            .await?
            .ok_or_else(|| AppError::not_found("question not found"))
    }

    async fn submit_attempt(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        question_id: &str,
        answer: &str,
        now: DateTime<Utc>,
    ) -> Result<SubmitAttemptPayload> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin attempt: {error}")))?;
        let session: Option<(Value, String, i32)> = sqlx::query_as(
            "SELECT question_ids, status, total_questions FROM app.quiz_sessions \
             WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load session", error))?;
        let (question_ids, status, _total_questions) =
            session.ok_or_else(|| AppError::not_found("session not found"))?;
        let ids = question_ids_from_value(&question_ids);
        if !ids.iter().any(|id| id == question_id) {
            return Err(AppError::invalid(
                "question does not belong to this session",
            ));
        }
        let question: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT answer_option_id, explanation FROM question.questions \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(question_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load answer key", error))?;
        let (correct_option, explanation) =
            question.ok_or_else(|| AppError::not_found("question not found"))?;
        let existing: Option<(Uuid, bool)> = sqlx::query_as(
            "SELECT attempt_id, is_correct FROM app.attempts \
             WHERE session_id = $1 AND question_id = $2",
        )
        .bind(session_id)
        .bind(question_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load attempt", error))?;
        if let Some((attempt_id, correct)) = existing {
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("commit attempt: {error}")))?;
            return Ok(SubmitAttemptPayload {
                accepted: true,
                attempt_id,
                correct,
                correct_option_id: Some(ID::from(correct_option)),
                explanation,
            });
        }
        if status_from_str(&status) != SessionStatus::Active {
            return Err(AppError::invalid("session is not active"));
        }
        let correct = answer == correct_option;
        let attempt_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO app.attempts \
             (attempt_id, user_id, session_id, question_id, is_correct, answer, submitted_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(attempt_id)
        .bind(user_id)
        .bind(session_id)
        .bind(question_id)
        .bind(correct)
        .bind(answer)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("insert attempt", error))?;
        // Refresh the per-user error vector in the same transaction so the
        // next practice session and battle matchmaking see one consistent view.
        sqlx::query("SELECT app.refresh_user_vector($1)")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("refresh user vector", error))?;
        let answered: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.attempts WHERE session_id = $1")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| db_error("count attempts", error))?;
        sqlx::query(
            "UPDATE app.quiz_sessions SET \
               current_index = $2, \
               status = CASE WHEN $2 >= total_questions THEN 'COMPLETED' ELSE status END, \
               completed_at = CASE WHEN $2 >= total_questions THEN $3 ELSE completed_at END \
             WHERE id = $1",
        )
        .bind(session_id)
        .bind(answered as i32)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("update session", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit attempt: {error}")))?;
        if correct {
            self.score_attempt(user_id, now).await;
        }
        Ok(SubmitAttemptPayload {
            accepted: true,
            attempt_id,
            correct,
            correct_option_id: Some(ID::from(correct_option)),
            explanation,
        })
    }

    async fn leaderboard(&self, period: Period, page: i32, size: i32) -> Result<LeaderboardPage> {
        let page = page.max(1);
        let size = size.clamp(1, 100);
        let mut connection = self.redis.clone();
        let key = leaderboard_key(period, Utc::now());
        let total: i64 = redis::cmd("ZCARD")
            .arg(&key)
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::internal(format!("read leaderboard size: {error}")))?;
        let start = i64::from((page - 1) * size);
        let stop = start + i64::from(size) - 1;
        let rows: Vec<(String, f64)> = redis::cmd("ZREVRANGE")
            .arg(&key)
            .arg(start)
            .arg(stop)
            .arg("WITHSCORES")
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::internal(format!("read leaderboard: {error}")))?;
        let user_ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|(id, _)| Uuid::parse_str(id).ok())
            .collect();
        let profiles = self.profile_names(&user_ids).await?;
        let entries = rows
            .iter()
            .enumerate()
            .filter_map(|(offset, (id, score))| {
                let user_id = Uuid::parse_str(id).ok()?;
                let profile = profiles.get(&user_id);
                Some(LeaderboardEntry {
                    rank: start as i32 + offset as i32 + 1,
                    user_id,
                    nickname: profile
                        .map(|view| view.nickname.clone())
                        .unwrap_or_else(|| "player".to_string()),
                    avatar_url: profile.and_then(|view| view.avatar_url.clone()),
                    score: *score as i32,
                })
            })
            .collect();
        Ok(LeaderboardPage {
            entries,
            period,
            page,
            page_size: size,
            total: total as i32,
        })
    }

    async fn my_rank(&self, period: Period, user_id: Uuid) -> Result<RankSummary> {
        let mut connection = self.redis.clone();
        let key = leaderboard_key(period, Utc::now());
        let rank: Option<i64> = redis::cmd("ZREVRANK")
            .arg(&key)
            .arg(user_id.to_string())
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::internal(format!("read rank: {error}")))?;
        let score: Option<f64> = redis::cmd("ZSCORE")
            .arg(&key)
            .arg(user_id.to_string())
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::internal(format!("read score: {error}")))?;
        Ok(RankSummary {
            period,
            rank: rank.map(|value| value as i32 + 1),
            score: score.map(|value| value as i32).unwrap_or(0),
        })
    }

    async fn configs(&self, namespace: &str) -> Result<Vec<ConfigEntry>> {
        let rows: Vec<(String, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT namespace, key, value, updated_at FROM config.configs \
             WHERE namespace = $1 ORDER BY key",
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("list configs", error))?;
        Ok(rows
            .into_iter()
            .map(|(namespace, key, value, updated_at)| ConfigEntry {
                namespace,
                key,
                value,
                updated_at,
            })
            .collect())
    }

    async fn config_schemas(&self, namespace: Option<&str>) -> Result<Vec<ConfigSchemaEntry>> {
        let rows: Vec<(String, String, String, bool, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT namespace, key, type_name, required, default_value, description \
                 FROM config.schemas \
                 WHERE ($1::text IS NULL OR namespace = $1) \
                 ORDER BY namespace, key",
            )
            .bind(namespace)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| db_error("list config schemas", error))?;
        Ok(rows
            .into_iter()
            .map(
                |(namespace, key, type_name, required, default_value, description)| {
                    ConfigSchemaEntry {
                        namespace,
                        key,
                        type_name,
                        required,
                        default_value,
                        description,
                    }
                },
            )
            .collect())
    }

    async fn upsert_config(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ConfigMutationPayload> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin config write: {error}")))?;
        let old: Option<(String,)> =
            sqlx::query_as("SELECT value FROM config.configs WHERE namespace = $1 AND key = $2")
                .bind(namespace)
                .bind(key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| db_error("load config", error))?;
        sqlx::query(
            "INSERT INTO config.configs (namespace, key, value, updated_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (namespace, key) DO UPDATE SET value = $3, updated_at = $4",
        )
        .bind(namespace)
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("upsert config", error))?;
        sqlx::query(
            "INSERT INTO config.config_audit \
             (namespace, key, old_value, new_value, changed_by, changed_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(namespace)
        .bind(key)
        .bind(old.map(|(value,)| value))
        .bind(value)
        .bind(changed_by)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("write config audit", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit config write: {error}")))?;
        self.sync_config_namespace(namespace).await;
        Ok(ConfigMutationPayload {
            applied: true,
            degraded: false,
            entry: Some(ConfigEntry {
                namespace: namespace.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                updated_at: now,
            }),
            error_message: None,
        })
    }

    async fn delete_config(
        &self,
        namespace: &str,
        key: &str,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ConfigMutationPayload> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin config delete: {error}")))?;
        let old: Option<(String,)> =
            sqlx::query_as("SELECT value FROM config.configs WHERE namespace = $1 AND key = $2")
                .bind(namespace)
                .bind(key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| db_error("load config", error))?;
        sqlx::query("DELETE FROM config.configs WHERE namespace = $1 AND key = $2")
            .bind(namespace)
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("delete config", error))?;
        sqlx::query(
            "INSERT INTO config.config_audit \
             (namespace, key, old_value, new_value, changed_by, changed_at) \
             VALUES ($1, $2, $3, NULL, $4, $5)",
        )
        .bind(namespace)
        .bind(key)
        .bind(old.map(|(value,)| value))
        .bind(changed_by)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("write config audit", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit config delete: {error}")))?;
        self.sync_config_namespace(namespace).await;
        Ok(ConfigMutationPayload {
            applied: true,
            degraded: false,
            entry: None,
            error_message: None,
        })
    }

    async fn rollback_config(
        &self,
        namespace: &str,
        key: &str,
        version: i32,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ConfigMutationPayload> {
        if version < 1 {
            return Err(AppError::not_found("config version not found"));
        }
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT old_value FROM config.config_audit \
             WHERE namespace = $1 AND key = $2 ORDER BY id ASC OFFSET $3 LIMIT 1",
        )
        .bind(namespace)
        .bind(key)
        .bind(i64::from(version - 1))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load config history", error))?;
        let (old_value,) = row.ok_or_else(|| AppError::not_found("config version not found"))?;
        match old_value {
            Some(value) => {
                self.upsert_config(namespace, key, &value, changed_by, now)
                    .await
            }
            None => self.delete_config(namespace, key, changed_by, now).await,
        }
    }

    async fn config_history(
        &self,
        namespace: &str,
        key: &str,
        limit: i32,
    ) -> Result<Vec<ConfigAuditEntry>> {
        let rows: Vec<(
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<Uuid>,
            DateTime<Utc>,
        )> = sqlx::query_as(
            "SELECT id, namespace, key, old_value, new_value, changed_by, changed_at \
             FROM config.config_audit WHERE namespace = $1 AND key = $2 \
             ORDER BY id DESC LIMIT $3",
        )
        .bind(namespace)
        .bind(key)
        .bind(i64::from(limit.max(1)))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("load config history", error))?;
        Ok(rows
            .into_iter()
            .map(
                |(id, namespace, key, old_value, new_value, changed_by, changed_at)| {
                    ConfigAuditEntry {
                        id: ID::from(id.to_string()),
                        namespace,
                        key,
                        old_value,
                        new_value,
                        changed_by,
                        changed_at,
                    }
                },
            )
            .collect())
    }

    async fn register_schema(&self, schema: ConfigSchemaEntry) -> Result<ConfigSchemaEntry> {
        sqlx::query(
            "INSERT INTO config.schemas \
             (namespace, key, type_name, required, default_value, description) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&schema.namespace)
        .bind(&schema.key)
        .bind(&schema.type_name)
        .bind(schema.required)
        .bind(&schema.default_value)
        .bind(&schema.description)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if let sqlx::Error::Database(ref database) = error {
                if database.code().as_deref() == Some("23505") {
                    return AppError::conflict("config schema already registered");
                }
            }
            db_error("register config schema", error)
        })?;
        Ok(schema)
    }

    async fn upload_question(
        &self,
        chapter_id: &str,
        difficulty: i32,
        content: QuestionContent,
        correct_option_id: String,
        now: DateTime<Utc>,
    ) -> Result<QuestionVersion> {
        let exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM question.chapters WHERE id = $1")
                .bind(chapter_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| db_error("load chapter", error))?;
        if exists == 0 {
            return Err(AppError::not_found("chapter not found"));
        }
        let question_id = format!("q-{}", Uuid::new_v4());
        let options = content_to_value(&content);
        let version_id = Uuid::new_v4();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin upload: {error}")))?;
        sqlx::query(
            "INSERT INTO question.questions \
             (id, chapter_id, difficulty, stem, image_url, explanation, options, \
              answer_option_id, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)",
        )
        .bind(&question_id)
        .bind(chapter_id)
        .bind(difficulty)
        .bind(&content.stem)
        .bind(&content.image_url)
        .bind(&content.explanation)
        .bind(options["options"].clone())
        .bind(&correct_option_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("insert question", error))?;
        sqlx::query(
            "INSERT INTO question.question_versions \
             (id, question_id, version, content, correct_option_id, deleted_at, created_at) \
             VALUES ($1, $2, 1, $3, $4, NULL, $5)",
        )
        .bind(version_id)
        .bind(&question_id)
        .bind(options.clone())
        .bind(&correct_option_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("insert question version", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit upload: {error}")))?;
        Ok(QuestionVersion {
            id: ID::from(version_id.to_string()),
            question_id: ID::from(question_id),
            version: 1,
            deleted_at: None,
            created_at: now,
        })
    }

    async fn update_question(
        &self,
        question_id: &str,
        chapter_id: &str,
        difficulty: i32,
        content: QuestionContent,
        correct_option_id: String,
        now: DateTime<Utc>,
    ) -> Result<QuestionVersion> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM question.questions WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(question_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| db_error("load question", error))?;
        if exists == 0 {
            return Err(AppError::not_found("question not found"));
        }
        let current: i32 = sqlx::query_scalar(
            "SELECT COALESCE(max(version), 0) FROM question.question_versions \
             WHERE question_id = $1",
        )
        .bind(question_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| db_error("load question version", error))?;
        let options = content_to_value(&content);
        let version_id = Uuid::new_v4();
        let version = current + 1;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin update: {error}")))?;
        sqlx::query(
            "UPDATE question.questions SET chapter_id = $2, difficulty = $3, stem = $4, \
             image_url = $5, explanation = $6, options = $7, answer_option_id = $8, \
             updated_at = $9 WHERE id = $1",
        )
        .bind(question_id)
        .bind(chapter_id)
        .bind(difficulty)
        .bind(&content.stem)
        .bind(&content.image_url)
        .bind(&content.explanation)
        .bind(options["options"].clone())
        .bind(&correct_option_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("update question", error))?;
        sqlx::query(
            "INSERT INTO question.question_versions \
             (id, question_id, version, content, correct_option_id, deleted_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, NULL, $6)",
        )
        .bind(version_id)
        .bind(question_id)
        .bind(version)
        .bind(options.clone())
        .bind(&correct_option_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("insert question version", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit update: {error}")))?;
        Ok(QuestionVersion {
            id: ID::from(version_id.to_string()),
            question_id: ID::from(question_id.to_string()),
            version,
            deleted_at: None,
            created_at: now,
        })
    }

    async fn soft_delete_question(&self, question_id: &str, now: DateTime<Utc>) -> Result<bool> {
        let row: Option<(
            String,
            String,
            i32,
            String,
            Option<String>,
            Option<String>,
            Value,
        )> = sqlx::query_as(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options \
                 FROM question.questions WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(question_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| db_error("load question", error))?;
        let (id, _, _, stem, image_url, explanation, options) =
            row.ok_or_else(|| AppError::not_found("question not found"))?;
        let current: i32 = sqlx::query_scalar(
            "SELECT COALESCE(max(version), 0) FROM question.question_versions \
             WHERE question_id = $1",
        )
        .bind(question_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| db_error("load question version", error))?;
        let content = json!({
            "stem": stem,
            "image_url": image_url,
            "explanation": explanation,
            "options": options,
        });
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin delete: {error}")))?;
        sqlx::query("UPDATE question.questions SET deleted_at = $2 WHERE id = $1")
            .bind(&id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("delete question", error))?;
        sqlx::query(
            "INSERT INTO question.question_versions \
             (id, question_id, version, content, correct_option_id, deleted_at, created_at) \
             VALUES ($1, $2, $3, $4, '', $5, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(&id)
        .bind(current + 1)
        .bind(&content)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("insert question version", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit delete: {error}")))?;
        Ok(true)
    }

    async fn question_versions(&self, question_id: &str) -> Result<Vec<QuestionVersion>> {
        let rows: Vec<(Uuid, String, i32, Option<DateTime<Utc>>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, question_id, version, deleted_at, created_at \
                 FROM question.question_versions WHERE question_id = $1 ORDER BY version",
        )
        .bind(question_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("list question versions", error))?;
        Ok(rows
            .into_iter()
            .map(
                |(id, question_id, version, deleted_at, created_at)| QuestionVersion {
                    id: ID::from(id.to_string()),
                    question_id: ID::from(question_id),
                    version,
                    deleted_at,
                    created_at,
                },
            )
            .collect())
    }

    async fn config_snapshot(&self) -> Result<Value> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT namespace, key, value FROM config.configs ORDER BY namespace, key",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| db_error("load config snapshot", error))?;
        Ok(json!({
            "revision": self.config_revision().await?,
            "entries": rows
                .into_iter()
                .map(|(namespace, key, value)| json!({
                    "namespace": namespace,
                    "key": key,
                    "value": value,
                }))
                .collect::<Vec<_>>(),
        }))
    }
}

fn option_id(index: u32) -> String {
    ["a", "b", "c", "d"][((index - 1) % 4) as usize].to_string()
}

fn option_label(index: u32) -> String {
    ["A", "B", "C", "D"][((index - 1) % 4) as usize].to_string()
}

fn seed_content(chapter_name: &str, index: u32) -> Value {
    json!({
        "stem": format!("{chapter_name} problem {index}: choose the correct statement."),
        "image_url": null,
        "explanation": format!(
            "Statement {} is the only consistent choice.",
            option_label(index)
        ),
        "options": (["a", "b", "c", "d"]
            .iter()
            .enumerate()
            .map(|(position, id)| json!({
                "id": id,
                "label": (["A", "B", "C", "D"][position]),
                "text": format!("Statement {} for problem {}", position + 1, index),
            }))
            .collect::<Vec<_>>()),
    })
}

fn chapter_seed() -> [(&'static str, &'static str); 8] {
    [
        ("mechanics", "Mechanics"),
        ("thermodynamics", "Thermodynamics"),
        ("optics", "Optics"),
        ("electromagnetism", "Electromagnetism"),
        ("modern", "Modern Physics"),
        ("waves", "Waves"),
        ("fluids", "Fluids"),
        ("experiments", "Experiments"),
    ]
}
