//! Chapter and question rows read straight from PostgreSQL.
//!
//! Recommendations rank chapters, but the question text itself lives in
//! `question.questions`. Reading it from the shared database keeps the
//! recommendation payload identical to what practice and battle serve.

use common::error::{AppError, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatalogChapter {
    pub id: String,
    pub name: String,
    pub position: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatalogQuestion {
    pub id: String,
    pub chapter_id: String,
    pub difficulty: i32,
    pub stem: String,
    pub image_url: Option<String>,
    pub explanation: Option<String>,
    pub options: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogOption {
    pub id: String,
    pub label: String,
    pub text: String,
}

impl CatalogQuestion {
    pub fn parsed_options(&self) -> Vec<CatalogOption> {
        serde_json::from_value(self.options.clone()).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct PgCatalog {
    pool: PgPool,
}

impl PgCatalog {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(database_url)
            .await
            .map_err(|error| AppError::internal(format!("connect question catalog: {error}")))?;
        Ok(Self { pool })
    }

    pub async fn chapters(&self) -> Result<Vec<CatalogChapter>> {
        sqlx::query_as::<_, CatalogChapter>(
            "SELECT id, name, position FROM question.chapters ORDER BY position, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load chapters: {error}")))
    }

    pub async fn questions(&self, chapter_id: &str, limit: i64) -> Result<Vec<CatalogQuestion>> {
        sqlx::query_as::<_, CatalogQuestion>(
            "SELECT id, chapter_id, difficulty, stem, image_url, explanation, options \
             FROM question.questions \
             WHERE chapter_id = $1 AND deleted_at IS NULL \
             ORDER BY difficulty, id LIMIT $2",
        )
        .bind(chapter_id)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| AppError::internal(format!("load chapter questions: {error}")))
    }
}
