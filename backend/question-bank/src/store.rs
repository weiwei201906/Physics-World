//! PostgreSQL persistence and read-through Redis caching for questions.

use chrono::{DateTime, Utc};
use common::error::{AppError, Result};
use qbank_client::{Question, QuestionContent, QuestionMedia, QuestionMeta, QuestionOption};
use redis::aio::ConnectionManager;
use serde_json::Value;
use sqlx::PgPool;

use crate::bundle::Bundle;

const CACHE_PREFIX: &str = "qbank:question:";
const MAX_LIMIT: i32 = 200;

#[derive(Debug, Clone, sqlx::FromRow)]
struct QuestionRow {
    id: String,
    chapter_id: String,
    difficulty: i32,
    stem: String,
    image_url: Option<String>,
    explanation: Option<String>,
    options: Value,
    media: Value,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct QuestionStore {
    pool: PgPool,
    redis: ConnectionManager,
    cache_ttl_sec: u64,
}

impl QuestionStore {
    pub fn new(pool: PgPool, redis: ConnectionManager, cache_ttl_sec: u64) -> Self {
        Self {
            pool,
            redis,
            cache_ttl_sec,
        }
    }

    /// Upserts every chapter and question from `bundle`.
    pub async fn import(&self, bundle: &Bundle, media_base: &str) -> Result<usize> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("begin question import: {error}")))?;
        let mut imported = 0usize;
        for (index, chapter) in bundle.chapters.iter().enumerate() {
            let position = chapter.position.unwrap_or(index as i32);
            sqlx::query(
                "INSERT INTO question.chapters (id, name, position) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, \
                 position = EXCLUDED.position",
            )
            .bind(&chapter.id)
            .bind(&chapter.name)
            .bind(position)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal(format!("upsert chapter: {error}")))?;

            for question in &chapter.questions {
                let media = question.media_value(media_base);
                let image_url = question.primary_image(media_base);
                let explanation = question.explanation.clone().unwrap_or_default();
                let answer = question
                    .answer
                    .clone()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "a".to_owned());
                let scope = question
                    .scope
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({}));
                sqlx::query(
                    "INSERT INTO question.questions \
                     (id, chapter_id, difficulty, stem, image_url, explanation, options, \
                      answer_option_id, media, scope, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(), now()) \
                     ON CONFLICT (id) DO UPDATE SET chapter_id = EXCLUDED.chapter_id, \
                     difficulty = EXCLUDED.difficulty, stem = EXCLUDED.stem, \
                     image_url = EXCLUDED.image_url, explanation = EXCLUDED.explanation, \
                     options = EXCLUDED.options, answer_option_id = EXCLUDED.answer_option_id, \
                     media = EXCLUDED.media, scope = EXCLUDED.scope, updated_at = now()",
                )
                .bind(&question.id)
                .bind(&chapter.id)
                .bind(question.difficulty)
                .bind(&question.stem)
                .bind(&image_url)
                .bind(&explanation)
                .bind(question.options_value())
                .bind(&answer)
                .bind(&media)
                .bind(&scope)
                .execute(&mut *tx)
                .await
                .map_err(|error| AppError::internal(format!("upsert question: {error}")))?;
                imported += 1;
            }
        }
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("commit question import: {error}")))?;

        for chapter in &bundle.chapters {
            for question in &chapter.questions {
                self.invalidate(&question.id).await;
            }
        }
        Ok(imported)
    }

    pub async fn get(&self, question_id: &str) -> Result<Option<Question>> {
        if let Some(cached) = self.cached(question_id).await {
            return Ok(Some(cached));
        }
        let row = sqlx::query_as::<_, QuestionRow>(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
             media, updated_at FROM question.questions \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(question_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load question: {error}")))?;
        let question = row.map(into_question);
        if let Some(ref question) = question {
            self.remember(question).await;
        }
        Ok(question)
    }

    pub async fn batch(&self, question_ids: &[String]) -> Result<Vec<Question>> {
        let rows = sqlx::query_as::<_, QuestionRow>(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
             media, updated_at FROM question.questions \
             WHERE id = ANY($1) AND deleted_at IS NULL",
        )
        .bind(question_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load questions: {error}")))?;
        Ok(rows.into_iter().map(into_question).collect())
    }

    pub async fn chapter_questions(
        &self,
        chapter_id: &str,
        limit: i32,
        exclude: &[String],
    ) -> Result<Vec<Question>> {
        let limit = clamp_limit(limit, 50);
        let rows = sqlx::query_as::<_, QuestionRow>(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
             media, updated_at FROM question.questions \
             WHERE chapter_id = $1 AND deleted_at IS NULL AND NOT (id = ANY($2)) \
             ORDER BY difficulty, id LIMIT $3",
        )
        .bind(chapter_id)
        .bind(exclude)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load chapter questions: {error}")))?;
        Ok(rows.into_iter().map(into_question).collect())
    }

    pub async fn random(&self, chapter_id: &str, count: i32) -> Result<Vec<Question>> {
        let count = clamp_limit(count, 3);
        let rows = if chapter_id.is_empty() {
            sqlx::query_as::<_, QuestionRow>(
                "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
                 media, updated_at FROM question.questions \
                 WHERE deleted_at IS NULL ORDER BY random() LIMIT $1",
            )
            .bind(count)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, QuestionRow>(
                "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
                 media, updated_at FROM question.questions \
                 WHERE chapter_id = $1 AND deleted_at IS NULL \
                 ORDER BY random() LIMIT $2",
            )
            .bind(chapter_id)
            .bind(count)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|error| AppError::internal(format!("load random questions: {error}")))?;
        Ok(rows.into_iter().map(into_question).collect())
    }

    pub async fn meta(&self, question_ids: &[String]) -> Result<Vec<QuestionMeta>> {
        let rows = sqlx::query_as::<_, QuestionRow>(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options, \
             media, updated_at FROM question.questions \
             WHERE id = ANY($1) AND deleted_at IS NULL",
        )
        .bind(question_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load question metadata: {error}")))?;
        Ok(rows
            .into_iter()
            .map(|row| QuestionMeta {
                id: row.id,
                chapter_id: row.chapter_id,
                difficulty: row.difficulty,
                updated_at_epoch_ms: row.updated_at.timestamp_millis(),
            })
            .collect())
    }

    async fn cached(&self, question_id: &str) -> Option<Question> {
        let mut connection = self.redis.clone();
        let raw: Option<String> = redis::cmd("GET")
            .arg(format!("{CACHE_PREFIX}{question_id}"))
            .query_async(&mut connection)
            .await
            .ok()?;
        serde_json::from_str(raw.as_deref()?).ok()
    }

    async fn remember(&self, question: &Question) {
        let Ok(payload) = serde_json::to_string(question) else {
            return;
        };
        let mut connection = self.redis.clone();
        let result: redis::RedisResult<()> = redis::cmd("SETEX")
            .arg(format!("{CACHE_PREFIX}{}", question.id))
            .arg(self.cache_ttl_sec)
            .arg(payload)
            .query_async(&mut connection)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "failed to cache question");
        }
    }

    async fn invalidate(&self, question_id: &str) {
        let mut connection = self.redis.clone();
        let result: redis::RedisResult<()> = redis::cmd("DEL")
            .arg(format!("{CACHE_PREFIX}{question_id}"))
            .query_async(&mut connection)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "failed to invalidate question cache");
        }
    }
}

fn clamp_limit(value: i32, fallback: i32) -> i32 {
    if value <= 0 {
        fallback
    } else {
        value.min(MAX_LIMIT)
    }
}

fn into_question(row: QuestionRow) -> Question {
    let options: Vec<QuestionOption> = serde_json::from_value(row.options).unwrap_or_default();
    let media: Vec<QuestionMedia> = serde_json::from_value(row.media).unwrap_or_default();
    let image_url = row
        .image_url
        .filter(|value| !value.is_empty())
        .or_else(|| media.first().map(|entry| entry.url.clone()))
        .unwrap_or_default();
    Question {
        id: row.id,
        chapter_id: row.chapter_id,
        difficulty: row.difficulty,
        content: Some(QuestionContent {
            stem: row.stem,
            image_url,
            explanation: row.explanation.unwrap_or_default(),
            options,
            media,
        }),
    }
}
