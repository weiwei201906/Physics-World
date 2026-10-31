//! GraphQL schema for `/rec/graphql`.
//!
//! The contract in `contracts/graphql/recommendation.graphql` defines
//! `recommendQuestions` and `userProfileSummary`. The service also exposes the
//! profile-centric `recommendations`, `profileSummary` and `recordAttempt`
//! operations used by the recommendation flow.

use async_graphql::{Context, EmptySubscription, InputObject, Object, Schema, SimpleObject, ID};
use chrono::{DateTime, Utc};
use common::error::Result as AppResult;
use uuid::Uuid;

use crate::model::ChapterVector;
use crate::AppState;

pub type RecSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

fn gql<T>(result: AppResult<T>) -> async_graphql::Result<T> {
    result.map_err(|error| async_graphql::Error::new(error.to_string()))
}

fn state<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a AppState> {
    ctx.data::<AppState>()
}

fn identity(ctx: &Context<'_>) -> Option<common::identity::Identity> {
    ctx.data::<Option<common::identity::Identity>>()
        .ok()
        .cloned()
        .flatten()
}

fn require_identity(ctx: &Context<'_>) -> async_graphql::Result<common::identity::Identity> {
    identity(ctx).ok_or_else(|| async_graphql::Error::new("unauthenticated"))
}

async fn merged_profile(state: &AppState, user_id: Uuid) -> AppResult<ChapterVector> {
    let baseline = state.source.load_baseline(user_id).await?;
    let increments = state.source.load_increments(user_id).await?;
    Ok(baseline.merge(&increments).normalized())
}

async fn profile_updated_at(state: &AppState, user_id: Uuid) -> AppResult<DateTime<Utc>> {
    Ok(state
        .source
        .updated_at(user_id)
        .await?
        .unwrap_or_else(|| state.clock.now()))
}

pub fn build_schema(state: AppState) -> RecSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(state)
        .finish()
}

#[derive(SimpleObject)]
#[graphql(name = "Recommendation")]
pub struct RecommendationView {
    pub chapter_id: String,
    pub chapter_name: String,
    pub score: f64,
    pub reason: String,
}

#[derive(SimpleObject)]
#[graphql(name = "FeatureSummary")]
pub struct FeatureSummaryView {
    pub feature_vector: Vec<f64>,
    pub chapter_dims: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(SimpleObject)]
#[graphql(name = "ChapterStrength")]
pub struct ChapterStrengthView {
    pub chapter_id: String,
    pub chapter_name: String,
    pub strength: f64,
    pub question_count: i32,
}

#[derive(SimpleObject)]
#[graphql(name = "ProfileSummary")]
pub struct ProfileSummaryView {
    pub user_id: Uuid,
    pub total_attempts: i32,
    pub correct_rate: f64,
    pub chapter_strengths: Vec<ChapterStrengthView>,
}

#[derive(SimpleObject)]
#[graphql(name = "Option")]
pub struct OptionView {
    pub id: ID,
    pub label: String,
    pub text: String,
}

#[derive(SimpleObject)]
#[graphql(name = "QuestionContent")]
pub struct QuestionContentView {
    pub stem: String,
    pub image_url: Option<String>,
    pub explanation: Option<String>,
    pub options: Vec<OptionView>,
}

#[derive(SimpleObject)]
#[graphql(name = "Question")]
pub struct QuestionView {
    pub id: ID,
    pub chapter_id: String,
    pub difficulty: i32,
    pub content: QuestionContentView,
}

#[derive(InputObject)]
#[graphql(name = "RecordAttemptInput")]
pub struct RecordAttemptInput {
    pub chapter_id: String,
    pub correct: bool,
}

#[derive(SimpleObject)]
#[graphql(name = "RecordAttemptPayload")]
pub struct RecordAttemptPayload {
    pub accepted: bool,
    pub chapter_id: String,
    pub weight: f64,
    pub updated_at: DateTime<Utc>,
}

pub struct QueryRoot;

#[Object(name = "Query")]
impl QueryRoot {
    /// Ranked chapter recommendations for the authenticated user.
    async fn recommendations(
        &self,
        ctx: &Context<'_>,
        limit: i32,
    ) -> async_graphql::Result<Vec<RecommendationView>> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let profile = gql(merged_profile(state, identity.user_id).await)?;
        let limit = state.settings.resolve_limit(Some(limit));
        Ok(state
            .engine
            .rank(&profile, limit)
            .into_iter()
            .map(|item| RecommendationView {
                chapter_id: item.chapter_id,
                chapter_name: item.chapter_name,
                score: item.score,
                reason: item.reason,
            })
            .collect())
    }

    /// Merged, normalized feature vector and the last baseline update.
    async fn profile_summary(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<FeatureSummaryView> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let profile = gql(merged_profile(state, identity.user_id).await)?;
        let updated_at = gql(profile_updated_at(state, identity.user_id).await)?;
        Ok(FeatureSummaryView {
            feature_vector: profile.weights().to_vec(),
            chapter_dims: profile.dims().to_vec(),
            updated_at,
        })
    }

    /// Contract operation: question recommendations per chapter.
    async fn recommend_questions(
        &self,
        ctx: &Context<'_>,
        chapter_id: Option<ID>,
        count: Option<i32>,
    ) -> async_graphql::Result<Vec<QuestionView>> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let profile = gql(merged_profile(state, identity.user_id).await)?;
        let count = state.settings.resolve_limit(count);

        let targets: Vec<(String, String)> = match chapter_id.as_ref() {
            Some(id) => {
                let chapter = state.engine.chapter(id.as_ref()).ok_or_else(|| {
                    async_graphql::Error::new(format!("unknown chapter {}", id.as_ref()))
                })?;
                vec![(chapter.id.clone(), chapter.name.clone())]
            }
            None => {
                let ranked = state.engine.rank(&profile, count);
                if ranked.is_empty() {
                    state
                        .engine
                        .chapters()
                        .iter()
                        .take(count)
                        .map(|chapter| (chapter.id.clone(), chapter.name.clone()))
                        .collect()
                } else {
                    ranked
                        .into_iter()
                        .map(|item| (item.chapter_id, item.chapter_name))
                        .collect()
                }
            }
        };

        let per_chapter = if chapter_id.is_some() { count } else { 1 };
        let mut questions = Vec::new();
        for (chapter_id, chapter_name) in targets {
            for index in 1..=per_chapter {
                if questions.len() >= count {
                    break;
                }
                questions.push(question_stub(&chapter_id, &chapter_name, index));
            }
        }
        Ok(questions)
    }

    /// Contract operation: attempts, correct rate and per-chapter strengths.
    async fn user_profile_summary(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ProfileSummaryView> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let profile = gql(merged_profile(state, identity.user_id).await)?;
        let stats = gql(state.source.attempt_stats(identity.user_id).await)?;
        let chapter_strengths = state
            .engine
            .dims()
            .iter()
            .map(|dim| {
                let chapter = state.engine.chapter(dim);
                ChapterStrengthView {
                    chapter_id: dim.clone(),
                    chapter_name: chapter
                        .map(|chapter| chapter.name.clone())
                        .unwrap_or_else(|| dim.clone()),
                    strength: profile.get(dim),
                    question_count: i32::try_from(
                        stats.chapter_attempts.get(dim).copied().unwrap_or(0),
                    )
                    .unwrap_or(i32::MAX),
                }
            })
            .collect();
        Ok(ProfileSummaryView {
            user_id: identity.user_id,
            total_attempts: i32::try_from(stats.total_attempts).unwrap_or(i32::MAX),
            correct_rate: stats.correct_rate(),
            chapter_strengths,
        })
    }
}

pub struct MutationRoot;

#[Object(name = "Mutation")]
impl MutationRoot {
    /// Record a practice attempt: a wrong answer adds `delta_wrong`, a correct
    /// answer decays the chapter weight by `delta_correct`.
    async fn record_attempt(
        &self,
        ctx: &Context<'_>,
        input: RecordAttemptInput,
    ) -> async_graphql::Result<RecordAttemptPayload> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let chapter = state.engine.chapter(&input.chapter_id).ok_or_else(|| {
            async_graphql::Error::new(format!("unknown chapter {}", input.chapter_id))
        })?;
        let chapter_id = chapter.id.clone();
        gql(state
            .source
            .record_attempt(
                identity.user_id,
                &chapter_id,
                input.correct,
                state.settings.delta_wrong,
                state.settings.delta_correct,
            )
            .await)?;
        let profile = gql(merged_profile(state, identity.user_id).await)?;
        let updated_at = gql(profile_updated_at(state, identity.user_id).await)?;
        Ok(RecordAttemptPayload {
            accepted: true,
            weight: profile.get(&chapter_id),
            chapter_id,
            updated_at,
        })
    }
}

// The question payload is a deterministic placeholder until the QBank client
// is wired in; it keeps the contract shape stable for the frontend.
fn question_stub(chapter_id: &str, chapter_name: &str, index: usize) -> QuestionView {
    let options = ["A", "B", "C", "D"]
        .into_iter()
        .map(|label| OptionView {
            id: ID::from(format!("{chapter_id}-{index}-{label}")),
            label: label.to_string(),
            text: format!("Choice {label}"),
        })
        .collect();
    QuestionView {
        id: ID::from(format!("{chapter_id}-{index}")),
        chapter_id: chapter_id.to_string(),
        difficulty: 2,
        content: QuestionContentView {
            stem: format!("Review question {index} for {chapter_name}"),
            image_url: None,
            explanation: Some("Unused placeholder content.".to_string()),
            options,
        },
    }
}
