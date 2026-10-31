// Client bindings for the question bank gRPC contract.

use async_trait::async_trait;
use common::error::{AppError, Result};
use tonic::transport::Channel;
use tonic::Code;

#[allow(clippy::result_large_err)]
pub mod qbank {
    include!(concat!(env!("OUT_DIR"), "/qbank.v1.rs"));
}

pub use qbank::question_bank_client::QuestionBankClient;

pub type Connection = Channel;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionMedia {
    pub url: String,
    pub alt: String,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionContent {
    pub stem: String,
    pub image_url: String,
    pub explanation: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub media: Vec<QuestionMedia>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    pub id: String,
    pub chapter_id: String,
    pub difficulty: i32,
    pub content: Option<QuestionContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionMeta {
    pub id: String,
    pub chapter_id: String,
    pub difficulty: i32,
    pub updated_at_epoch_ms: i64,
}

impl From<qbank::QuestionOption> for QuestionOption {
    fn from(value: qbank::QuestionOption) -> Self {
        Self {
            id: value.id,
            label: value.label,
            text: value.text,
        }
    }
}

impl From<QuestionOption> for qbank::QuestionOption {
    fn from(value: QuestionOption) -> Self {
        Self {
            id: value.id,
            label: value.label,
            text: value.text,
        }
    }
}

impl From<qbank::QuestionMedia> for QuestionMedia {
    fn from(value: qbank::QuestionMedia) -> Self {
        Self {
            url: value.url,
            alt: value.alt,
            variant: value.variant,
            width: value.width,
            height: value.height,
            mime: value.mime,
        }
    }
}

impl From<QuestionMedia> for qbank::QuestionMedia {
    fn from(value: QuestionMedia) -> Self {
        Self {
            url: value.url,
            alt: value.alt,
            variant: value.variant,
            width: value.width,
            height: value.height,
            mime: value.mime,
        }
    }
}

impl From<qbank::QuestionContent> for QuestionContent {
    fn from(value: qbank::QuestionContent) -> Self {
        Self {
            stem: value.stem,
            image_url: value.image_url,
            explanation: value.explanation,
            options: value.options.into_iter().map(Into::into).collect(),
            media: value.media.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<QuestionContent> for qbank::QuestionContent {
    fn from(value: QuestionContent) -> Self {
        Self {
            stem: value.stem,
            image_url: value.image_url,
            explanation: value.explanation,
            options: value.options.into_iter().map(Into::into).collect(),
            media: value.media.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<qbank::Question> for Question {
    fn from(value: qbank::Question) -> Self {
        Self {
            id: value.id,
            chapter_id: value.chapter_id,
            difficulty: value.difficulty,
            content: value.content.map(Into::into),
        }
    }
}

impl From<Question> for qbank::Question {
    fn from(value: Question) -> Self {
        Self {
            id: value.id,
            chapter_id: value.chapter_id,
            difficulty: value.difficulty,
            content: value.content.map(Into::into),
        }
    }
}

impl From<qbank::QuestionMeta> for QuestionMeta {
    fn from(value: qbank::QuestionMeta) -> Self {
        Self {
            id: value.id,
            chapter_id: value.chapter_id,
            difficulty: value.difficulty,
            updated_at_epoch_ms: value.updated_at_epoch_ms,
        }
    }
}

impl From<QuestionMeta> for qbank::QuestionMeta {
    fn from(value: QuestionMeta) -> Self {
        Self {
            id: value.id,
            chapter_id: value.chapter_id,
            difficulty: value.difficulty,
            updated_at_epoch_ms: value.updated_at_epoch_ms,
        }
    }
}

fn map_status(status: tonic::Status) -> AppError {
    let message = status.message().to_owned();
    match status.code() {
        Code::NotFound => AppError::not_found(message),
        Code::InvalidArgument => AppError::invalid(message),
        Code::Unavailable | Code::DeadlineExceeded => AppError::internal(message),
        code => AppError::internal(format!("qbank grpc {code}: {message}")),
    }
}

#[async_trait]
pub trait QuestionBank: Send + Sync {
    async fn get_question(&self, question_id: &str) -> Result<Question>;
    async fn batch_get_questions(&self, question_ids: &[String]) -> Result<Vec<Question>>;
    async fn get_chapter_questions(&self, chapter_id: &str, limit: i32) -> Result<Vec<Question>>;
    async fn get_question_meta(&self, question_ids: &[String]) -> Result<Vec<QuestionMeta>>;
    async fn get_random_questions(&self, chapter_id: &str, count: i32) -> Result<Vec<Question>>;
    async fn search_questions(
        &self,
        query: &str,
        chapter_id: &str,
        difficulty: i32,
        limit: i32,
    ) -> Result<Vec<Question>>;
}

pub struct QbankClient {
    inner: QuestionBankClient<Connection>,
}

impl QbankClient {
    pub async fn connect(addr: impl Into<String>) -> Result<Self> {
        let channel = Channel::from_shared(addr.into())
            .map_err(|err| AppError::invalid(format!("invalid qbank address: {err}")))?
            .connect()
            .await
            .map_err(|err| AppError::internal(format!("failed to connect to qbank: {err}")))?;
        Ok(Self {
            inner: QuestionBankClient::new(channel),
        })
    }
}

#[async_trait]
impl QuestionBank for QbankClient {
    async fn get_question(&self, question_id: &str) -> Result<Question> {
        let mut client = self.inner.clone();
        let response = client
            .get_question(qbank::GetQuestionRequest {
                question_id: question_id.to_owned(),
            })
            .await
            .map_err(map_status)?;
        response
            .into_inner()
            .question
            .map(Into::into)
            .ok_or_else(|| AppError::not_found(format!("question {question_id}")))
    }

    async fn batch_get_questions(&self, question_ids: &[String]) -> Result<Vec<Question>> {
        let mut client = self.inner.clone();
        let response = client
            .batch_get_questions(qbank::BatchGetQuestionsRequest {
                question_ids: question_ids.to_vec(),
            })
            .await
            .map_err(map_status)?;
        Ok(response
            .into_inner()
            .questions
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_chapter_questions(&self, chapter_id: &str, limit: i32) -> Result<Vec<Question>> {
        let mut client = self.inner.clone();
        let mut stream = client
            .get_chapter_questions(qbank::GetChapterQuestionsRequest {
                chapter_id: chapter_id.to_owned(),
                limit,
                user_id: String::new(),
                exclude_question_ids: Vec::new(),
            })
            .await
            .map_err(map_status)?
            .into_inner();

        let mut questions = Vec::new();
        while let Some(item) = stream.message().await.map_err(map_status)? {
            questions.push(item.into());
        }
        Ok(questions)
    }

    async fn get_question_meta(&self, question_ids: &[String]) -> Result<Vec<QuestionMeta>> {
        let mut client = self.inner.clone();
        let response = client
            .get_question_meta(qbank::GetQuestionMetaRequest {
                question_ids: question_ids.to_vec(),
            })
            .await
            .map_err(map_status)?;
        Ok(response
            .into_inner()
            .metas
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_random_questions(&self, chapter_id: &str, count: i32) -> Result<Vec<Question>> {
        let mut client = self.inner.clone();
        let response = client
            .get_random_questions(qbank::GetRandomQuestionsRequest {
                chapter_id: chapter_id.to_owned(),
                count,
            })
            .await
            .map_err(map_status)?;
        Ok(response
            .into_inner()
            .questions
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn search_questions(
        &self,
        query: &str,
        chapter_id: &str,
        difficulty: i32,
        limit: i32,
    ) -> Result<Vec<Question>> {
        let mut client = self.inner.clone();
        let response = client
            .search_questions(qbank::SearchQuestionsRequest {
                query: query.to_owned(),
                chapter_id: chapter_id.to_owned(),
                difficulty,
                limit,
            })
            .await
            .map_err(map_status)?;
        Ok(response
            .into_inner()
            .questions
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

// Deterministic in-memory bank used by tests and local wiring.
pub struct InMemoryQuestionBank {
    questions: Vec<Question>,
}

const CHAPTERS: [&str; 8] = [
    "mechanics",
    "thermodynamics",
    "electromagnetism",
    "optics",
    "waves",
    "relativity",
    "quantum",
    "nuclear",
];

impl InMemoryQuestionBank {
    pub fn new() -> Self {
        let mut questions = Vec::new();
        for chapter in CHAPTERS {
            for number in 1..=3 {
                questions.push(Question {
                    id: format!("{chapter}-{number}"),
                    chapter_id: chapter.to_owned(),
                    difficulty: number,
                    content: Some(QuestionContent {
                        stem: format!("Question {number} of {chapter}"),
                        image_url: String::new(),
                        explanation: format!("Explanation for {chapter}-{number}"),
                        options: vec![
                            QuestionOption {
                                id: "a".to_owned(),
                                label: "A".to_owned(),
                                text: "Option A".to_owned(),
                            },
                            QuestionOption {
                                id: "b".to_owned(),
                                label: "B".to_owned(),
                                text: "Option B".to_owned(),
                            },
                        ],
                        media: Vec::new(),
                    }),
                });
            }
        }
        Self { questions }
    }

    fn chapter_questions(&self, chapter_id: &str) -> Result<Vec<Question>> {
        let questions: Vec<Question> = self
            .questions
            .iter()
            .filter(|question| question.chapter_id == chapter_id)
            .cloned()
            .collect();
        if questions.is_empty() {
            return Err(AppError::not_found(format!("chapter {chapter_id}")));
        }
        Ok(questions)
    }

    fn question(&self, question_id: &str) -> Result<Question> {
        self.questions
            .iter()
            .find(|question| question.id == question_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("question {question_id}")))
    }
}

impl Default for InMemoryQuestionBank {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QuestionBank for InMemoryQuestionBank {
    async fn get_question(&self, question_id: &str) -> Result<Question> {
        self.question(question_id)
    }

    async fn batch_get_questions(&self, question_ids: &[String]) -> Result<Vec<Question>> {
        question_ids
            .iter()
            .map(|question_id| self.question(question_id))
            .collect()
    }

    async fn get_chapter_questions(&self, chapter_id: &str, limit: i32) -> Result<Vec<Question>> {
        let mut questions = self.chapter_questions(chapter_id)?;
        if limit > 0 {
            questions.truncate(limit as usize);
        }
        Ok(questions)
    }

    async fn get_question_meta(&self, question_ids: &[String]) -> Result<Vec<QuestionMeta>> {
        question_ids
            .iter()
            .map(|question_id| {
                self.question(question_id).map(|question| QuestionMeta {
                    id: question.id,
                    chapter_id: question.chapter_id,
                    difficulty: question.difficulty,
                    updated_at_epoch_ms: 1_700_000_000_000,
                })
            })
            .collect()
    }

    async fn get_random_questions(&self, chapter_id: &str, count: i32) -> Result<Vec<Question>> {
        let mut questions = self.chapter_questions(chapter_id)?;
        if count <= 0 {
            return Ok(Vec::new());
        }

        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for byte in chapter_id.as_bytes() {
            state = state
                .wrapping_mul(0x100_0000_01b3)
                .wrapping_add(u64::from(*byte));
        }
        for index in (1..questions.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let target = (state % (index as u64 + 1)) as usize;
            questions.swap(index, target);
        }

        questions.truncate(count as usize);
        Ok(questions)
    }

    async fn search_questions(
        &self,
        query: &str,
        chapter_id: &str,
        difficulty: i32,
        limit: i32,
    ) -> Result<Vec<Question>> {
        let needle = query.trim().to_ascii_lowercase();
        let mut questions: Vec<Question> = self
            .questions
            .iter()
            .filter(|question| chapter_id.is_empty() || question.chapter_id == chapter_id)
            .filter(|question| difficulty <= 0 || question.difficulty == difficulty)
            .filter(|question| {
                needle.is_empty()
                    || question
                        .content
                        .as_ref()
                        .map(|content| content.stem.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        if limit > 0 {
            questions.truncate(limit as usize);
        }
        Ok(questions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proto_question() -> qbank::Question {
        qbank::Question {
            id: "mechanics-1".to_owned(),
            chapter_id: "mechanics".to_owned(),
            difficulty: 2,
            content: Some(qbank::QuestionContent {
                stem: "A block slides down a ramp.".to_owned(),
                image_url: "https://example.test/ramp.png".to_owned(),
                explanation: "Use energy conservation.".to_owned(),
                options: vec![qbank::QuestionOption {
                    id: "a".to_owned(),
                    label: "A".to_owned(),
                    text: "5 m/s".to_owned(),
                }],
                media: vec![qbank::QuestionMedia {
                    url: "https://example.test/ramp.svg".to_owned(),
                    alt: "Ramp diagram".to_owned(),
                    variant: "default".to_owned(),
                    width: 640,
                    height: 480,
                    mime: "image/svg+xml".to_owned(),
                }],
            }),
        }
    }

    #[test]
    fn proto_question_roundtrip_preserves_fields() {
        let proto = proto_question();
        let dto = Question::from(proto.clone());
        assert_eq!(dto.id, "mechanics-1");
        assert_eq!(dto.chapter_id, "mechanics");
        assert_eq!(dto.difficulty, 2);
        let content = dto.content.as_ref().expect("content");
        assert_eq!(content.options.len(), 1);
        assert_eq!(content.media.len(), 1);
        assert_eq!(content.media[0].variant, "default");

        let back: qbank::Question = dto.into();
        assert_eq!(back, proto);
    }

    #[test]
    fn proto_meta_roundtrip_preserves_fields() {
        let proto = qbank::QuestionMeta {
            id: "optics-2".to_owned(),
            chapter_id: "optics".to_owned(),
            difficulty: 3,
            updated_at_epoch_ms: 42,
        };
        let dto = QuestionMeta::from(proto.clone());
        let back: qbank::QuestionMeta = dto.into();
        assert_eq!(back, proto);
    }

    #[tokio::test]
    async fn in_memory_lookup_and_not_found() {
        let bank = InMemoryQuestionBank::new();
        let question = bank.get_question("mechanics-1").await.expect("lookup");
        assert_eq!(question.chapter_id, "mechanics");
        assert!(question.content.is_some());

        let error = bank
            .get_question("mechanics-99")
            .await
            .expect_err("missing question");
        assert!(matches!(error, AppError::NotFound(_)));

        let error = bank
            .get_chapter_questions("unknown", 3)
            .await
            .expect_err("missing chapter");
        assert!(matches!(error, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn in_memory_batch_preserves_request_order() {
        let bank = InMemoryQuestionBank::new();
        let ids = vec![
            "mechanics-3".to_owned(),
            "mechanics-1".to_owned(),
            "mechanics-2".to_owned(),
        ];
        let questions = bank.batch_get_questions(&ids).await.expect("batch");
        let returned: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(returned, ["mechanics-3", "mechanics-1", "mechanics-2"]);

        let metas = bank.get_question_meta(&ids).await.expect("metas");
        let returned: Vec<&str> = metas.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(returned, ["mechanics-3", "mechanics-1", "mechanics-2"]);
    }

    #[tokio::test]
    async fn in_memory_chapter_limit_and_coverage() {
        let bank = InMemoryQuestionBank::new();
        assert_eq!(CHAPTERS.len(), 8);
        assert_eq!(bank.questions.len(), 24);

        let all = bank
            .get_chapter_questions("mechanics", 0)
            .await
            .expect("all");
        assert_eq!(all.len(), 3);

        let limited = bank
            .get_chapter_questions("mechanics", 2)
            .await
            .expect("limited");
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn in_memory_random_is_deterministic() {
        let bank = InMemoryQuestionBank::new();
        let first = bank
            .get_random_questions("mechanics", 2)
            .await
            .expect("random");
        let second = bank
            .get_random_questions("mechanics", 2)
            .await
            .expect("random");
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert!(first
            .iter()
            .all(|question| question.chapter_id == "mechanics"));

        let none = bank
            .get_random_questions("mechanics", 0)
            .await
            .expect("empty");
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn in_memory_search_matches_stem_and_filters() {
        let bank = InMemoryQuestionBank::new();
        let all = bank
            .search_questions("question", "", 0, 0)
            .await
            .expect("all");
        assert_eq!(all.len(), 24);

        let filtered = bank
            .search_questions("mechanics", "mechanics", 2, 0)
            .await
            .expect("filtered");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "mechanics-2");

        let limited = bank
            .search_questions("", "optics", 0, 1)
            .await
            .expect("limited");
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].chapter_id, "optics");
    }

    #[test]
    fn grpc_status_maps_to_app_error() {
        let mapped = map_status(tonic::Status::not_found("missing"));
        assert!(matches!(mapped, AppError::NotFound(_)));

        let mapped = map_status(tonic::Status::invalid_argument("bad input"));
        assert!(matches!(mapped, AppError::InvalidRequest(_)));

        let mapped = map_status(tonic::Status::unavailable("down"));
        assert!(matches!(mapped, AppError::Internal(_)));

        let mapped = map_status(tonic::Status::deadline_exceeded("timeout"));
        assert!(matches!(mapped, AppError::Internal(_)));

        let mapped = map_status(tonic::Status::unknown("boom"));
        assert!(matches!(mapped, AppError::Internal(_)));
    }
}
