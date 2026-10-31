//! gRPC implementation of the `qbank.v1.QuestionBank` contract.

use std::sync::Arc;

use qbank_client::qbank::question_bank_server::QuestionBank;
use qbank_client::qbank::{
    BatchGetQuestionsRequest, BatchGetQuestionsResponse, GetChapterQuestionsRequest,
    GetQuestionMetaRequest, GetQuestionMetaResponse, GetQuestionRequest, GetQuestionResponse,
    GetRandomQuestionsRequest, GetRandomQuestionsResponse, Question,
};
use tonic::{Request, Response, Status};

use crate::store::QuestionStore;

pub struct QuestionBankService {
    store: Arc<QuestionStore>,
}

impl QuestionBankService {
    pub fn new(store: Arc<QuestionStore>) -> Self {
        Self { store }
    }
}

#[allow(clippy::result_large_err)]
#[tonic::async_trait]
impl QuestionBank for QuestionBankService {
    async fn get_question(
        &self,
        request: Request<GetQuestionRequest>,
    ) -> Result<Response<GetQuestionResponse>, Status> {
        let question_id = request.into_inner().question_id;
        if question_id.is_empty() {
            return Err(Status::invalid_argument("question_id is required"));
        }
        let question = self
            .store
            .get(&question_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| Status::not_found(format!("question {question_id} not found")))?;
        Ok(Response::new(GetQuestionResponse {
            question: Some(question.into()),
        }))
    }

    async fn batch_get_questions(
        &self,
        request: Request<BatchGetQuestionsRequest>,
    ) -> Result<Response<BatchGetQuestionsResponse>, Status> {
        let ids = request.into_inner().question_ids;
        if ids.is_empty() {
            return Ok(Response::new(BatchGetQuestionsResponse {
                questions: Vec::new(),
            }));
        }
        let questions = self.store.batch(&ids).await.map_err(internal)?;
        Ok(Response::new(BatchGetQuestionsResponse {
            questions: questions.into_iter().map(Into::into).collect(),
        }))
    }

    type GetChapterQuestionsStream =
        tokio_stream::Iter<std::vec::IntoIter<Result<Question, Status>>>;

    async fn get_chapter_questions(
        &self,
        request: Request<GetChapterQuestionsRequest>,
    ) -> Result<Response<Self::GetChapterQuestionsStream>, Status> {
        let request = request.into_inner();
        if request.chapter_id.is_empty() {
            return Err(Status::invalid_argument("chapter_id is required"));
        }
        let questions = self
            .store
            .chapter_questions(
                &request.chapter_id,
                request.limit,
                &request.exclude_question_ids,
            )
            .await
            .map_err(internal)?;
        let stream = tokio_stream::iter(
            questions
                .into_iter()
                .map(|question| Ok(question.into()))
                .collect::<Vec<Result<Question, Status>>>(),
        );
        Ok(Response::new(stream))
    }

    async fn get_question_meta(
        &self,
        request: Request<GetQuestionMetaRequest>,
    ) -> Result<Response<GetQuestionMetaResponse>, Status> {
        let ids = request.into_inner().question_ids;
        let metas = if ids.is_empty() {
            Vec::new()
        } else {
            self.store.meta(&ids).await.map_err(internal)?
        };
        Ok(Response::new(GetQuestionMetaResponse {
            metas: metas.into_iter().map(Into::into).collect(),
        }))
    }

    async fn get_random_questions(
        &self,
        request: Request<GetRandomQuestionsRequest>,
    ) -> Result<Response<GetRandomQuestionsResponse>, Status> {
        let request = request.into_inner();
        let questions = self
            .store
            .random(&request.chapter_id, request.count)
            .await
            .map_err(internal)?;
        Ok(Response::new(GetRandomQuestionsResponse {
            questions: questions.into_iter().map(Into::into).collect(),
        }))
    }
}

fn internal(error: common::error::AppError) -> Status {
    Status::internal(error.to_string())
}
