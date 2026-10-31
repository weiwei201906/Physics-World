//! GraphQL schema mirroring `contracts/graphql/user_api.graphql`.

use async_graphql::{Context, EmptySubscription, InputObject, Object, Schema, ID};
use common::error::Result as AppResult;
use common::identity::Identity;
use uuid::Uuid;

use crate::state::{AppState, UserApiSchema};
use crate::store::{
    ConfigSchemaEntry, LeaderboardPage, Period, Question, QuestionContent, QuestionVersion,
    QuizMode, QuizSession, RankSummary, SubmitAttemptPayload, UserView,
};

fn gql<T>(result: AppResult<T>) -> async_graphql::Result<T> {
    result.map_err(|error| async_graphql::Error::new(error.to_string()))
}

fn state<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a AppState> {
    ctx.data::<AppState>()
}

fn identity(ctx: &Context<'_>) -> Option<Identity> {
    ctx.data::<Option<Identity>>().ok().cloned().flatten()
}

fn require_identity(ctx: &Context<'_>) -> async_graphql::Result<Identity> {
    identity(ctx).ok_or_else(|| async_graphql::Error::new("unauthenticated"))
}

fn require_admin(ctx: &Context<'_>) -> async_graphql::Result<Identity> {
    let identity = require_identity(ctx)?;
    if !identity.is_admin() {
        return Err(async_graphql::Error::new("forbidden"));
    }
    Ok(identity)
}

pub fn build_schema(state: AppState) -> UserApiSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(state)
        .finish()
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn me(&self, ctx: &Context<'_>) -> async_graphql::Result<UserView> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.profile(
            identity.user_id,
            identity.email.as_deref(),
            state.clock.now(),
        ))
    }

    async fn chapters(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<crate::store::Chapter>> {
        let state = state(ctx)?;
        Ok(state.store.chapters())
    }

    async fn quiz_session(
        &self,
        ctx: &Context<'_>,
        id: Uuid,
    ) -> async_graphql::Result<Option<QuizSession>> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.session(id, identity.user_id))
    }

    async fn session_question(
        &self,
        ctx: &Context<'_>,
        session_id: Uuid,
        index: i32,
    ) -> async_graphql::Result<Option<Question>> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        Ok(state
            .store
            .question_in_session(session_id, identity.user_id, index)
            .ok())
    }

    async fn leaderboard(
        &self,
        ctx: &Context<'_>,
        period: Period,
        page: i32,
        size: i32,
    ) -> async_graphql::Result<LeaderboardPage> {
        let state = state(ctx)?;
        Ok(state.store.leaderboard(period, page, size))
    }

    async fn my_rank(
        &self,
        ctx: &Context<'_>,
        period: Period,
    ) -> async_graphql::Result<RankSummary> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.my_rank(period, identity.user_id))
    }

    async fn admin_configs(
        &self,
        ctx: &Context<'_>,
        namespace: String,
    ) -> async_graphql::Result<Vec<crate::store::ConfigEntry>> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.configs(&namespace))
    }

    async fn admin_config_schemas(
        &self,
        ctx: &Context<'_>,
        namespace: Option<String>,
    ) -> async_graphql::Result<Vec<ConfigSchemaEntry>> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.config_schemas(namespace.as_deref()))
    }

    async fn admin_config_history(
        &self,
        ctx: &Context<'_>,
        namespace: String,
        key: String,
        limit: i32,
    ) -> async_graphql::Result<Vec<crate::store::ConfigAuditEntry>> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.config_history(&namespace, &key, limit))
    }

    async fn admin_question_versions(
        &self,
        ctx: &Context<'_>,
        question_id: ID,
    ) -> async_graphql::Result<Vec<QuestionVersion>> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.question_versions(question_id.as_ref()))
    }
}

#[derive(InputObject)]
pub struct SubmitAttemptInput {
    pub session_id: Uuid,
    pub question_id: ID,
    pub answer: String,
}

#[derive(InputObject)]
pub struct UpdateProfileInput {
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(InputObject)]
pub struct ConfigSchemaInput {
    pub namespace: String,
    pub key: String,
    #[graphql(name = "type")]
    pub type_name: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub description: Option<String>,
}

#[derive(InputObject)]
pub struct OptionInput {
    pub id: ID,
    pub label: String,
    pub text: String,
}

#[derive(InputObject)]
pub struct QuestionContentInput {
    pub stem: String,
    pub image_url: Option<String>,
    pub explanation: Option<String>,
    pub options: Vec<OptionInput>,
    pub correct_option_id: ID,
}

#[derive(InputObject)]
pub struct QuestionUploadInput {
    pub chapter_id: String,
    pub difficulty: i32,
    pub content: QuestionContentInput,
}

fn split_content(input: QuestionContentInput) -> (QuestionContent, String) {
    let correct = input.correct_option_id.to_string();
    let options = input
        .options
        .into_iter()
        .map(|option| crate::store::OptionItem {
            id: option.id,
            label: option.label,
            text: option.text,
        })
        .collect();
    (
        QuestionContent {
            stem: input.stem,
            image_url: input.image_url,
            explanation: input.explanation,
            options,
        },
        correct,
    )
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn start_quiz_session(
        &self,
        ctx: &Context<'_>,
        mode: QuizMode,
        chapter_id: Option<ID>,
    ) -> async_graphql::Result<QuizSession> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        let session = gql(state.store.start_session(
            identity.user_id,
            mode,
            chapter_id.map(|id| id.to_string()),
            state.settings.session_max_questions,
            state.clock.now(),
        ))?;
        Ok(session)
    }

    async fn submit_attempt(
        &self,
        ctx: &Context<'_>,
        input: SubmitAttemptInput,
    ) -> async_graphql::Result<SubmitAttemptPayload> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        gql(state.store.submit_attempt(
            input.session_id,
            identity.user_id,
            input.question_id.as_ref(),
            &input.answer,
            state.clock.now(),
        ))
    }

    async fn update_profile(
        &self,
        ctx: &Context<'_>,
        input: UpdateProfileInput,
    ) -> async_graphql::Result<UserView> {
        let identity = require_identity(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.update_profile(
            identity.user_id,
            identity.email.as_deref(),
            input.nickname,
            input.avatar_url,
            state.clock.now(),
        ))
    }

    async fn admin_upsert_config(
        &self,
        ctx: &Context<'_>,
        namespace: String,
        key: String,
        value: String,
    ) -> async_graphql::Result<crate::store::ConfigMutationPayload> {
        let identity = require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state.store.upsert_config(
            &namespace,
            &key,
            &value,
            identity.user_id,
            state.clock.now(),
        ))
    }

    async fn admin_delete_config(
        &self,
        ctx: &Context<'_>,
        namespace: String,
        key: String,
    ) -> async_graphql::Result<crate::store::ConfigMutationPayload> {
        let identity = require_admin(ctx)?;
        let state = state(ctx)?;
        Ok(state
            .store
            .delete_config(&namespace, &key, identity.user_id, state.clock.now()))
    }

    async fn admin_register_config_schema(
        &self,
        ctx: &Context<'_>,
        input: ConfigSchemaInput,
    ) -> async_graphql::Result<ConfigSchemaEntry> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        let schema = ConfigSchemaEntry {
            namespace: input.namespace,
            key: input.key,
            type_name: input.type_name,
            required: input.required,
            default_value: input.default_value,
            description: input.description,
        };
        gql(state.store.register_schema(schema))
    }

    async fn admin_rollback_config(
        &self,
        ctx: &Context<'_>,
        namespace: String,
        key: String,
        version: i32,
    ) -> async_graphql::Result<crate::store::ConfigMutationPayload> {
        let identity = require_admin(ctx)?;
        let state = state(ctx)?;
        gql(state.store.rollback_config(
            &namespace,
            &key,
            version,
            identity.user_id,
            state.clock.now(),
        ))
    }

    async fn admin_upload_question(
        &self,
        ctx: &Context<'_>,
        input: QuestionUploadInput,
    ) -> async_graphql::Result<QuestionVersion> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        let (content, correct_option_id) = split_content(input.content);
        gql(state.store.upload_question(
            &input.chapter_id,
            input.difficulty,
            content,
            correct_option_id,
            state.clock.now(),
        ))
    }

    async fn admin_update_question(
        &self,
        ctx: &Context<'_>,
        question_id: ID,
        input: QuestionUploadInput,
    ) -> async_graphql::Result<QuestionVersion> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        let (content, correct_option_id) = split_content(input.content);
        gql(state.store.update_question(
            question_id.as_ref(),
            &input.chapter_id,
            input.difficulty,
            content,
            correct_option_id,
            state.clock.now(),
        ))
    }

    async fn admin_soft_delete_question(
        &self,
        ctx: &Context<'_>,
        question_id: ID,
    ) -> async_graphql::Result<bool> {
        require_admin(ctx)?;
        let state = state(ctx)?;
        gql(state
            .store
            .soft_delete_question(question_id.as_ref(), state.clock.now()))
    }
}
