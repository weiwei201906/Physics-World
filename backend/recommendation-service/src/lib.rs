//! Recommendation service: profile vectors, read-time merge, flusher and the
//! `/rec/graphql` API.

pub mod engine;
pub mod graphql;
pub mod model;
pub mod routes;
pub mod settings;
pub mod store;

use std::sync::Arc;

use common::clock::Clock;

pub use engine::RecommendationEngine;
pub use graphql::{build_schema, RecSchema};
pub use model::{Chapter, ChapterVector, Recommendation};
pub use routes::router;
pub use settings::Settings;
pub use store::{Flusher, InMemoryProfileSource, ProfileSource, VectorFlusher};

/// Shared state for GraphQL resolvers.
#[derive(Clone)]
pub struct AppState {
    pub source: Arc<dyn ProfileSource>,
    pub engine: Arc<RecommendationEngine>,
    pub settings: Arc<Settings>,
    pub clock: Arc<dyn Clock>,
}

impl AppState {
    pub fn new(
        source: Arc<dyn ProfileSource>,
        engine: Arc<RecommendationEngine>,
        settings: Arc<Settings>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            source,
            engine,
            settings,
            clock,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use common::clock::SystemClock;
    use common::config::ConfigLoader;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn test_state() -> AppState {
        let settings = Settings::from_config(&ConfigLoader::new("rec", HashMap::new()));
        let dims = settings.chapter_dims.clone();
        let engine = Arc::new(RecommendationEngine::from_dims(
            dims.clone(),
            settings.min_score,
        ));
        let source = Arc::new(InMemoryProfileSource::new(dims));
        AppState::new(source, engine, Arc::new(settings), Arc::new(SystemClock))
    }

    fn graphql_request(user_id: Option<&str>, query: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/rec/graphql")
            .header("content-type", "application/json");
        if let Some(user_id) = user_id {
            builder = builder.header("x-user-id", user_id);
        }
        builder
            .body(Body::from(
                serde_json::json!({ "query": query }).to_string(),
            ))
            .unwrap()
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    const USER: &str = "0e6f8f3e-5f7f-4f2b-9c1f-1d2e3a4b5c6d";

    #[tokio::test]
    async fn health_endpoint_is_public() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "recommendation-service");
    }

    #[tokio::test]
    async fn graphql_get_accepts_query_string() {
        let app = router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rec/graphql?query=%7B__typename%7D")
                    .header("x-user-id", USER)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert!(body["errors"].is_null(), "{body}");
        assert_eq!(body["data"]["__typename"], "Query");
    }

    #[tokio::test]
    async fn graphql_rejects_missing_identity() {
        let app = router(test_state());
        let response = app
            .oneshot(graphql_request(
                None,
                "{ recommendations(limit: 3) { chapterId score } }",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert!(body["errors"].as_array().unwrap().iter().any(|error| {
            error["message"]
                .as_str()
                .is_some_and(|message| message.contains("unauthenticated"))
        }));
    }

    #[tokio::test]
    async fn record_attempt_then_recommendations_reflect_it() {
        let app = router(test_state());

        let mutation = app
            .clone()
            .oneshot(graphql_request(
                Some(USER),
                "mutation { recordAttempt(input: { chapterId: \"mechanics\", correct: false }) { accepted chapterId weight } }",
            ))
            .await
            .unwrap();
        let body = body_json(mutation).await;
        assert!(body["errors"].is_null(), "{body}");
        assert_eq!(body["data"]["recordAttempt"]["accepted"], true);
        assert_eq!(body["data"]["recordAttempt"]["chapterId"], "mechanics");

        let query = app
            .oneshot(graphql_request(
                Some(USER),
                "{ recommendations(limit: 5) { chapterId chapterName score reason } }",
            ))
            .await
            .unwrap();
        let body = body_json(query).await;
        assert!(body["errors"].is_null(), "{body}");
        let recommendations = body["data"]["recommendations"].as_array().unwrap();
        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0]["chapterId"], "mechanics");
        assert!(recommendations[0]["score"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn profile_summary_exposes_feature_vector_and_updated_at() {
        let app = router(test_state());
        let response = app
            .oneshot(graphql_request(
                Some(USER),
                "{ profileSummary { featureVector chapterDims updatedAt } }",
            ))
            .await
            .unwrap();
        let body = body_json(response).await;
        assert!(body["errors"].is_null(), "{body}");
        let summary = &body["data"]["profileSummary"];
        assert_eq!(summary["featureVector"].as_array().unwrap().len(), 8);
        assert_eq!(summary["chapterDims"].as_array().unwrap().len(), 8);
        assert!(summary["updatedAt"].is_string());
    }

    #[tokio::test]
    async fn contract_operations_are_in_the_schema() {
        let sdl = build_schema(test_state()).sdl();
        for needle in [
            "type Recommendation",
            "recommendations(limit: Int!): [Recommendation!]!",
            "recordAttempt(input: RecordAttemptInput!): RecordAttemptPayload!",
            "profileSummary: FeatureSummary!",
            "type Question",
            "recommendQuestions(chapterId: ID, count: Int): [Question!]!",
            "userProfileSummary: ProfileSummary!",
        ] {
            assert!(sdl.contains(needle), "missing {needle} in SDL");
        }
    }
}
