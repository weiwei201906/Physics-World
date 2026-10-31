//! HTTP surface: `/rec/graphql` (GET and POST) and `/health`.

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use common::identity::Identity;
use serde::Serialize;

use crate::graphql::{build_schema, RecSchema};
use crate::AppState;

pub fn router(state: AppState) -> Router {
    let schema = build_schema(state);
    Router::new()
        .route("/health", get(health))
        .route("/rec/graphql", get(graphql_handler).post(graphql_handler))
        .with_state(schema)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "recommendation-service",
    })
}

async fn graphql_handler(
    State(schema): State<RecSchema>,
    headers: HeaderMap,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let identity = Identity::from_headers(&headers).ok();
    let request = request.into_inner().data(identity);
    schema.execute(request).await.into()
}
