//! In-memory domain store backed by a seeded question bank.
//!
//! Every method returns owned values so callers never hold the lock across an
//! await point. PostgreSQL, Redis and the Flare question bank replace this
//! store without changing the GraphQL surface.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_graphql::{Enum, SimpleObject, ID};
use chrono::{DateTime, Utc};
use common::error::{AppError, Result};
use uuid::Uuid;

#[derive(Enum, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum QuizMode {
    #[graphql(name = "PRACTICE")]
    Practice,
    #[graphql(name = "EXAM")]
    Exam,
    #[graphql(name = "CHAPTER")]
    Chapter,
}

#[derive(Enum, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Period {
    #[graphql(name = "DAILY")]
    Daily,
    #[graphql(name = "WEEKLY")]
    Weekly,
    #[graphql(name = "ALL")]
    All,
}

impl Period {
    pub const ALL_PERIODS: [Period; 3] = [Period::Daily, Period::Weekly, Period::All];

    pub fn as_str(&self) -> &'static str {
        match self {
            Period::Daily => "daily",
            Period::Weekly => "weekly",
            Period::All => "all",
        }
    }
}

#[derive(Enum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    #[graphql(name = "ACTIVE")]
    Active,
    #[graphql(name = "COMPLETED")]
    Completed,
    #[graphql(name = "ABANDONED")]
    Abandoned,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(name = "User")]
pub struct UserView {
    pub id: Uuid,
    pub email: String,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(name = "Option")]
pub struct OptionItem {
    pub id: ID,
    pub label: String,
    pub text: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct QuestionContent {
    pub stem: String,
    pub image_url: Option<String>,
    pub explanation: Option<String>,
    pub options: Vec<OptionItem>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Question {
    pub id: ID,
    pub chapter_id: String,
    pub difficulty: i32,
    pub content: QuestionContent,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Chapter {
    pub id: ID,
    pub name: String,
    pub question_count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct QuizSession {
    pub id: Uuid,
    pub mode: QuizMode,
    pub chapter_id: Option<ID>,
    pub status: SessionStatus,
    pub current_index: i32,
    pub total_questions: i32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct SubmitAttemptPayload {
    pub accepted: bool,
    pub attempt_id: Uuid,
    pub correct: bool,
    pub correct_option_id: Option<ID>,
    pub explanation: Option<String>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct LeaderboardEntry {
    pub rank: i32,
    pub user_id: Uuid,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub score: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct LeaderboardPage {
    pub entries: Vec<LeaderboardEntry>,
    pub period: Period,
    pub page: i32,
    pub page_size: i32,
    pub total: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct RankSummary {
    pub period: Period,
    pub rank: Option<i32>,
    pub score: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ConfigEntry {
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ConfigMutationPayload {
    pub applied: bool,
    pub degraded: bool,
    pub entry: Option<ConfigEntry>,
    pub error_message: Option<String>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ConfigSchemaEntry {
    pub namespace: String,
    pub key: String,
    #[graphql(name = "type")]
    pub type_name: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub description: Option<String>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ConfigAuditEntry {
    pub id: ID,
    pub namespace: String,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub changed_by: Option<Uuid>,
    pub changed_at: DateTime<Utc>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct QuestionVersion {
    pub id: ID,
    pub question_id: ID,
    pub version: i32,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    user_id: Uuid,
    question_ids: Vec<String>,
    view: QuizSession,
}

#[derive(Debug, Clone)]
struct AttemptRecord {
    attempt_id: Uuid,
    session_id: Uuid,
    question_id: String,
    correct: bool,
}

#[derive(Debug)]
struct Inner {
    chapters: Vec<Chapter>,
    questions: HashMap<String, Question>,
    answers: HashMap<String, String>,
    order: Vec<String>,
    profiles: HashMap<Uuid, UserView>,
    sessions: HashMap<Uuid, SessionRecord>,
    attempts: Vec<AttemptRecord>,
    scores: HashMap<Period, HashMap<Uuid, i32>>,
    configs: HashMap<(String, String), ConfigEntry>,
    schemas: Vec<ConfigSchemaEntry>,
    audits: Vec<ConfigAuditEntry>,
    versions: HashMap<String, Vec<QuestionVersion>>,
    revision: u64,
    next_question: u32,
    next_audit: i64,
}

#[derive(Debug, Clone)]
pub struct Store {
    inner: Arc<Mutex<Inner>>,
}

impl Store {
    pub fn seeded() -> Self {
        let chapters = chapter_seed();
        let mut questions = HashMap::new();
        let mut answers = HashMap::new();
        let mut order = Vec::new();
        for chapter in &chapters {
            for index in 1..=3u32 {
                let id = format!("{}-{}", chapter.id.0, index);
                questions.insert(
                    id.clone(),
                    Question {
                        id: ID::from(id.clone()),
                        chapter_id: chapter.id.to_string(),
                        difficulty: ((index - 1) as i32 % 3) + 1,
                        content: QuestionContent {
                            stem: format!(
                                "{} problem {}: choose the correct statement.",
                                chapter.name, index
                            ),
                            image_url: None,
                            explanation: Some(format!(
                                "Statement {} is the only consistent choice.",
                                option_label(index)
                            )),
                            options: option_seed(index),
                        },
                    },
                );
                answers.insert(id.clone(), option_id(index));
                order.push(id);
            }
        }

        let mut configs = HashMap::new();
        let now = Utc::now();
        for (namespace, key, value) in default_config_entries() {
            let entry = ConfigEntry {
                namespace: namespace.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                updated_at: now,
            };
            configs.insert((namespace.to_string(), key.to_string()), entry);
        }

        let mut schemas = Vec::new();
        for (namespace, key, type_name, default_value, description) in default_config_schemas() {
            schemas.push(ConfigSchemaEntry {
                namespace: namespace.to_string(),
                key: key.to_string(),
                type_name: type_name.to_string(),
                required: true,
                default_value: Some(default_value.to_string()),
                description: Some(description.to_string()),
            });
        }

        Self {
            inner: Arc::new(Mutex::new(Inner {
                chapters,
                questions,
                answers,
                order,
                profiles: HashMap::new(),
                sessions: HashMap::new(),
                attempts: Vec::new(),
                scores: HashMap::new(),
                configs,
                schemas,
                audits: Vec::new(),
                versions: HashMap::new(),
                revision: 0,
                next_question: 0,
                next_audit: 0,
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn chapters(&self) -> Vec<Chapter> {
        let inner = self.lock();
        inner
            .chapters
            .iter()
            .map(|chapter| Chapter {
                id: chapter.id.clone(),
                name: chapter.name.clone(),
                question_count: inner
                    .questions
                    .values()
                    .filter(|question| question.chapter_id == chapter.id.to_string())
                    .count() as i32,
            })
            .collect()
    }

    pub fn question(&self, id: &str) -> Option<Question> {
        let inner = self.lock();
        inner.questions.get(id).cloned()
    }

    pub fn correct_answer(&self, id: &str) -> Option<String> {
        self.lock().answers.get(id).cloned()
    }

    pub fn profile(&self, user_id: Uuid, email: Option<&str>, now: DateTime<Utc>) -> UserView {
        let mut inner = self.lock();
        inner
            .profiles
            .entry(user_id)
            .or_insert(UserView {
                id: user_id,
                email: email
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("{user_id}@physics-world.local")),
                nickname: "player".to_string(),
                avatar_url: None,
                roles: vec!["student".to_string()],
                created_at: now,
            })
            .clone()
    }

    pub fn update_profile(
        &self,
        user_id: Uuid,
        email: Option<&str>,
        nickname: Option<String>,
        avatar_url: Option<String>,
        now: DateTime<Utc>,
    ) -> UserView {
        let mut inner = self.lock();
        let mut profile = inner
            .profiles
            .entry(user_id)
            .or_insert(UserView {
                id: user_id,
                email: email
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("{user_id}@physics-world.local")),
                nickname: "player".to_string(),
                avatar_url: None,
                roles: vec!["student".to_string()],
                created_at: now,
            })
            .clone();
        if let Some(nickname) = nickname.map(|value| value.trim().to_string()) {
            if !nickname.is_empty() {
                profile.nickname = nickname;
            }
        }
        if let Some(avatar_url) = avatar_url {
            profile.avatar_url = Some(avatar_url);
        }
        inner.profiles.insert(user_id, profile.clone());
        profile
    }

    pub fn start_session(
        &self,
        user_id: Uuid,
        mode: QuizMode,
        chapter_id: Option<String>,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<QuizSession> {
        let mut inner = self.lock();
        if mode == QuizMode::Chapter && chapter_id.is_none() {
            return Err(AppError::invalid("chapterId is required for CHAPTER mode"));
        }
        if let Some(chapter) = chapter_id.as_deref() {
            if !inner
                .chapters
                .iter()
                .any(|entry| entry.id.as_str() == chapter)
            {
                return Err(AppError::not_found("chapter not found"));
            }
        }
        let question_ids: Vec<String> = inner
            .order
            .iter()
            .filter(|id| match inner.questions.get(*id) {
                Some(question) => chapter_id
                    .as_deref()
                    .map(|chapter| question.chapter_id == chapter)
                    .unwrap_or(true),
                None => false,
            })
            .take(limit.max(1) as usize)
            .cloned()
            .collect();
        if question_ids.is_empty() {
            return Err(AppError::not_found("no questions available"));
        }
        let session = QuizSession {
            id: Uuid::new_v4(),
            mode,
            chapter_id: chapter_id.map(ID::from),
            status: SessionStatus::Active,
            current_index: 0,
            total_questions: question_ids.len() as i32,
            started_at: now,
            completed_at: None,
        };
        inner.sessions.insert(
            session.id,
            SessionRecord {
                user_id,
                question_ids,
                view: session.clone(),
            },
        );
        Ok(session)
    }

    pub fn session(&self, id: Uuid, user_id: Uuid) -> Option<QuizSession> {
        let inner = self.lock();
        inner
            .sessions
            .get(&id)
            .filter(|record| record.user_id == user_id)
            .map(|record| record.view.clone())
    }

    pub fn question_in_session(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        index: i32,
    ) -> Result<Question> {
        let inner = self.lock();
        let record = inner
            .sessions
            .get(&session_id)
            .filter(|record| record.user_id == user_id)
            .ok_or_else(|| AppError::not_found("session not found"))?;
        let position = index.max(0) as usize;
        let question_id = record
            .question_ids
            .get(position)
            .ok_or_else(|| AppError::invalid("index out of range"))?;
        inner
            .questions
            .get(question_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("question not found"))
    }

    pub fn submit_attempt(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        question_id: &str,
        answer: &str,
        now: DateTime<Utc>,
    ) -> Result<SubmitAttemptPayload> {
        let mut inner = self.lock();
        let record = inner
            .sessions
            .get(&session_id)
            .filter(|record| record.user_id == user_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("session not found"))?;
        if !record.question_ids.iter().any(|id| id == question_id) {
            return Err(AppError::invalid(
                "question does not belong to this session",
            ));
        }
        let question = inner
            .questions
            .get(question_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("question not found"))?;
        let correct_option = inner
            .answers
            .get(question_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("answer key missing"))?;

        if let Some(existing) = inner
            .attempts
            .iter()
            .find(|attempt| attempt.session_id == session_id && attempt.question_id == question_id)
            .cloned()
        {
            return Ok(SubmitAttemptPayload {
                accepted: true,
                attempt_id: existing.attempt_id,
                correct: existing.correct,
                correct_option_id: Some(ID::from(correct_option)),
                explanation: question.content.explanation.clone(),
            });
        }

        if record.view.status != SessionStatus::Active {
            return Err(AppError::invalid("session is not active"));
        }
        let correct = answer == correct_option;
        let attempt_id = Uuid::new_v4();
        inner.attempts.push(AttemptRecord {
            attempt_id,
            session_id,
            question_id: question_id.to_string(),
            correct,
        });
        if correct {
            for period in Period::ALL_PERIODS {
                *inner
                    .scores
                    .entry(period)
                    .or_default()
                    .entry(user_id)
                    .or_insert(0) += 100;
            }
        }
        let answered = inner
            .attempts
            .iter()
            .filter(|attempt| attempt.session_id == session_id)
            .count() as i32;
        if let Some(stored) = inner.sessions.get_mut(&session_id) {
            stored.view.current_index = answered;
            if answered >= stored.view.total_questions {
                stored.view.status = SessionStatus::Completed;
                stored.view.completed_at = Some(now);
            }
        }
        Ok(SubmitAttemptPayload {
            accepted: true,
            attempt_id,
            correct,
            correct_option_id: Some(ID::from(correct_option)),
            explanation: question.content.explanation.clone(),
        })
    }

    pub fn leaderboard(&self, period: Period, page: i32, size: i32) -> LeaderboardPage {
        let inner = self.lock();
        let page = page.max(1);
        let size = size.clamp(1, 100);
        let rows = ranked(&inner, period);
        let start = ((page - 1) * size) as usize;
        let entries = rows
            .iter()
            .skip(start)
            .take(size as usize)
            .enumerate()
            .map(|(offset, (user_id, score))| LeaderboardEntry {
                rank: start as i32 + offset as i32 + 1,
                user_id: *user_id,
                nickname: inner
                    .profiles
                    .get(user_id)
                    .map(|profile| profile.nickname.clone())
                    .unwrap_or_else(|| "player".to_string()),
                avatar_url: inner
                    .profiles
                    .get(user_id)
                    .and_then(|profile| profile.avatar_url.clone()),
                score: *score,
            })
            .collect();
        LeaderboardPage {
            entries,
            period,
            page,
            page_size: size,
            total: rows.len() as i32,
        }
    }

    pub fn my_rank(&self, period: Period, user_id: Uuid) -> RankSummary {
        let inner = self.lock();
        let rows = ranked(&inner, period);
        let score = inner
            .scores
            .get(&period)
            .and_then(|scores| scores.get(&user_id))
            .copied()
            .unwrap_or(0);
        let rank = rows
            .iter()
            .position(|(id, _)| *id == user_id)
            .map(|position| position as i32 + 1);
        RankSummary {
            period,
            rank,
            score,
        }
    }

    pub fn configs(&self, namespace: &str) -> Vec<ConfigEntry> {
        let inner = self.lock();
        let mut entries: Vec<ConfigEntry> = inner
            .configs
            .values()
            .filter(|entry| entry.namespace == namespace)
            .cloned()
            .collect();
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        entries
    }

    pub fn config_schemas(&self, namespace: Option<&str>) -> Vec<ConfigSchemaEntry> {
        let inner = self.lock();
        let mut schemas: Vec<ConfigSchemaEntry> = inner
            .schemas
            .iter()
            .filter(|schema| namespace.map(|ns| schema.namespace == ns).unwrap_or(true))
            .cloned()
            .collect();
        schemas.sort_by(|left, right| {
            (&left.namespace, &left.key).cmp(&(&right.namespace, &right.key))
        });
        schemas
    }

    pub fn upsert_config(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> ConfigMutationPayload {
        let mut inner = self.lock();
        let namespace_key = (namespace.to_string(), key.to_string());
        let old_value = inner
            .configs
            .get(&namespace_key)
            .map(|entry| entry.value.clone());
        let entry = ConfigEntry {
            namespace: namespace.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            updated_at: now,
        };
        inner.configs.insert(namespace_key, entry.clone());
        inner.next_audit += 1;
        let audit_id = inner.next_audit.to_string();
        inner.audits.push(ConfigAuditEntry {
            id: ID::from(audit_id),
            namespace: namespace.to_string(),
            key: key.to_string(),
            old_value,
            new_value: Some(value.to_string()),
            changed_by: Some(changed_by),
            changed_at: now,
        });
        inner.revision += 1;
        ConfigMutationPayload {
            applied: true,
            degraded: false,
            entry: Some(entry),
            error_message: None,
        }
    }

    pub fn delete_config(
        &self,
        namespace: &str,
        key: &str,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> ConfigMutationPayload {
        let mut inner = self.lock();
        let namespace_key = (namespace.to_string(), key.to_string());
        let old_value = inner
            .configs
            .remove(&namespace_key)
            .map(|entry| entry.value);
        inner.next_audit += 1;
        let audit_id = inner.next_audit.to_string();
        inner.audits.push(ConfigAuditEntry {
            id: ID::from(audit_id),
            namespace: namespace.to_string(),
            key: key.to_string(),
            old_value,
            new_value: None,
            changed_by: Some(changed_by),
            changed_at: now,
        });
        inner.revision += 1;
        ConfigMutationPayload {
            applied: true,
            degraded: false,
            entry: None,
            error_message: None,
        }
    }

    pub fn rollback_config(
        &self,
        namespace: &str,
        key: &str,
        version: i32,
        changed_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ConfigMutationPayload> {
        let mut inner = self.lock();
        let history: Vec<ConfigAuditEntry> = inner
            .audits
            .iter()
            .filter(|entry| entry.namespace == namespace && entry.key == key)
            .cloned()
            .collect();
        let index = version - 1;
        if index < 0 || index as usize >= history.len() {
            return Err(AppError::not_found("config version not found"));
        }
        match history[index as usize].old_value.clone() {
            Some(value) => Ok(self_upsert(
                &mut inner, namespace, key, &value, changed_by, now,
            )),
            None => Ok(self_delete(&mut inner, namespace, key, changed_by, now)),
        }
    }

    pub fn config_history(&self, namespace: &str, key: &str, limit: i32) -> Vec<ConfigAuditEntry> {
        let inner = self.lock();
        let mut history: Vec<ConfigAuditEntry> = inner
            .audits
            .iter()
            .filter(|entry| entry.namespace == namespace && entry.key == key)
            .cloned()
            .collect();
        history.reverse();
        history.truncate(limit.max(1) as usize);
        history
    }

    pub fn register_schema(&self, schema: ConfigSchemaEntry) -> Result<ConfigSchemaEntry> {
        let mut inner = self.lock();
        if inner
            .schemas
            .iter()
            .any(|entry| entry.namespace == schema.namespace && entry.key == schema.key)
        {
            return Err(AppError::conflict("config schema already registered"));
        }
        inner.schemas.push(schema.clone());
        Ok(schema)
    }

    pub fn upload_question(
        &self,
        chapter_id: &str,
        difficulty: i32,
        content: QuestionContent,
        correct_option_id: String,
        now: DateTime<Utc>,
    ) -> Result<QuestionVersion> {
        let mut inner = self.lock();
        if !inner
            .chapters
            .iter()
            .any(|chapter| chapter.id.as_str() == chapter_id)
        {
            return Err(AppError::not_found("chapter not found"));
        }
        inner.next_question += 1;
        let question_id = format!("q-{}", inner.next_question);
        let question = Question {
            id: ID::from(question_id.clone()),
            chapter_id: chapter_id.to_string(),
            difficulty,
            content,
        };
        inner.questions.insert(question_id.clone(), question);
        inner.order.push(question_id.clone());
        inner.answers.insert(question_id.clone(), correct_option_id);
        let version = QuestionVersion {
            id: ID::from(Uuid::new_v4().to_string()),
            question_id: ID::from(question_id.clone()),
            version: 1,
            deleted_at: None,
            created_at: now,
        };
        inner
            .versions
            .entry(question_id)
            .or_default()
            .push(version.clone());
        Ok(version)
    }

    pub fn update_question(
        &self,
        question_id: &str,
        chapter_id: &str,
        difficulty: i32,
        content: QuestionContent,
        correct_option_id: String,
        now: DateTime<Utc>,
    ) -> Result<QuestionVersion> {
        let mut inner = self.lock();
        let current_version = inner
            .versions
            .get(question_id)
            .and_then(|versions| versions.last())
            .map(|version| version.version)
            .unwrap_or(0);
        if !inner.questions.contains_key(question_id) {
            return Err(AppError::not_found("question not found"));
        }
        let question = Question {
            id: ID::from(question_id.to_string()),
            chapter_id: chapter_id.to_string(),
            difficulty,
            content,
        };
        inner.questions.insert(question_id.to_string(), question);
        inner
            .answers
            .insert(question_id.to_string(), correct_option_id);
        let version = QuestionVersion {
            id: ID::from(Uuid::new_v4().to_string()),
            question_id: ID::from(question_id.to_string()),
            version: current_version + 1,
            deleted_at: None,
            created_at: now,
        };
        inner
            .versions
            .entry(question_id.to_string())
            .or_default()
            .push(version.clone());
        Ok(version)
    }

    pub fn soft_delete_question(&self, question_id: &str, now: DateTime<Utc>) -> Result<bool> {
        let mut inner = self.lock();
        if !inner.questions.contains_key(question_id) {
            return Err(AppError::not_found("question not found"));
        }
        let next_version = inner
            .versions
            .get(question_id)
            .and_then(|versions| versions.last())
            .map(|version| version.version + 1)
            .unwrap_or(1);
        inner
            .versions
            .entry(question_id.to_string())
            .or_default()
            .push(QuestionVersion {
                id: ID::from(Uuid::new_v4().to_string()),
                question_id: ID::from(question_id.to_string()),
                version: next_version,
                deleted_at: Some(now),
                created_at: now,
            });
        inner.questions.remove(question_id);
        inner.answers.remove(question_id);
        inner.order.retain(|id| id != question_id);
        Ok(true)
    }

    pub fn question_versions(&self, question_id: &str) -> Vec<QuestionVersion> {
        let inner = self.lock();
        inner.versions.get(question_id).cloned().unwrap_or_default()
    }

    pub fn config_snapshot(&self) -> serde_json::Value {
        let inner = self.lock();
        let mut entries: Vec<&ConfigEntry> = inner.configs.values().collect();
        entries.sort_by(|left, right| {
            (&left.namespace, &left.key).cmp(&(&right.namespace, &right.key))
        });
        serde_json::json!({
            "revision": inner.revision,
            "entries": entries
                .iter()
                .map(|entry| serde_json::json!({
                    "namespace": entry.namespace,
                    "key": entry.key,
                    "value": entry.value,
                }))
                .collect::<Vec<_>>(),
        })
    }
}

fn ranked(inner: &Inner, period: Period) -> Vec<(Uuid, i32)> {
    let scores = inner.scores.get(&period).cloned().unwrap_or_default();
    let mut rows: Vec<(Uuid, i32)> = scores.into_iter().collect();
    rows.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.to_string().cmp(&right.0.to_string()))
    });
    rows
}

fn option_id(index: u32) -> String {
    ["a", "b", "c", "d"][((index - 1) % 4) as usize].to_string()
}

fn option_label(index: u32) -> String {
    ["A", "B", "C", "D"][((index - 1) % 4) as usize].to_string()
}

fn option_seed(index: u32) -> Vec<OptionItem> {
    ["a", "b", "c", "d"]
        .iter()
        .enumerate()
        .map(|(position, id)| OptionItem {
            id: ID::from((*id).to_string()),
            label: ["A", "B", "C", "D"][position].to_string(),
            text: format!("Statement {} for problem {}", position + 1, index),
        })
        .collect()
}

fn chapter_seed() -> Vec<Chapter> {
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
    .iter()
    .map(|(id, name)| Chapter {
        id: ID::from((*id).to_string()),
        name: (*name).to_string(),
        question_count: 3,
    })
    .collect()
}

fn default_config_entries() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("quiz", "session.max_questions", "50"),
        ("quiz", "submit.consistency_window_sec", "3"),
        ("leaderboard", "page.size_max", "100"),
        ("battle", "match.questions_per_room", "10"),
        ("recommendation", "default_count", "10"),
    ]
}

fn default_config_schemas() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    vec![
        (
            "quiz",
            "session.max_questions",
            "int",
            "50",
            "Maximum questions per quiz session",
        ),
        (
            "quiz",
            "submit.consistency_window_sec",
            "int",
            "3",
            "Idempotency window for attempt submission",
        ),
        (
            "leaderboard",
            "page.size_max",
            "int",
            "100",
            "Maximum leaderboard page size",
        ),
    ]
}

fn self_upsert(
    inner: &mut Inner,
    namespace: &str,
    key: &str,
    value: &str,
    changed_by: Uuid,
    now: DateTime<Utc>,
) -> ConfigMutationPayload {
    let namespace_key = (namespace.to_string(), key.to_string());
    let old_value = inner
        .configs
        .get(&namespace_key)
        .map(|entry| entry.value.clone());
    let entry = ConfigEntry {
        namespace: namespace.to_string(),
        key: key.to_string(),
        value: value.to_string(),
        updated_at: now,
    };
    inner.configs.insert(namespace_key, entry.clone());
    inner.next_audit += 1;
    let audit_id = inner.next_audit.to_string();
    inner.audits.push(ConfigAuditEntry {
        id: ID::from(audit_id),
        namespace: namespace.to_string(),
        key: key.to_string(),
        old_value,
        new_value: Some(value.to_string()),
        changed_by: Some(changed_by),
        changed_at: now,
    });
    inner.revision += 1;
    ConfigMutationPayload {
        applied: true,
        degraded: false,
        entry: Some(entry),
        error_message: None,
    }
}

fn self_delete(
    inner: &mut Inner,
    namespace: &str,
    key: &str,
    changed_by: Uuid,
    now: DateTime<Utc>,
) -> ConfigMutationPayload {
    let namespace_key = (namespace.to_string(), key.to_string());
    let old_value = inner
        .configs
        .remove(&namespace_key)
        .map(|entry| entry.value);
    inner.next_audit += 1;
    let audit_id = inner.next_audit.to_string();
    inner.audits.push(ConfigAuditEntry {
        id: ID::from(audit_id),
        namespace: namespace.to_string(),
        key: key.to_string(),
        old_value,
        new_value: None,
        changed_by: Some(changed_by),
        changed_at: now,
    });
    inner.revision += 1;
    ConfigMutationPayload {
        applied: true,
        degraded: false,
        entry: None,
        error_message: None,
    }
}
