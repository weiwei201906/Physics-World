//! HTTP surface for the user API: GraphQL, config snapshot and health probes.

use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use common::identity::Identity;
use tower_http::trace::TraceLayer;

use crate::state::App;

pub fn router(app: App) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/config", get(config_snapshot))
        .route("/graphiql", get(graphiql))
        .route("/graphql", post(graphql_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "user-api",
    })
}

async fn config_snapshot(State(app): State<App>) -> Json<serde_json::Value> {
    Json(app.state.store.config_snapshot())
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

async fn graphql_handler(
    State(app): State<App>,
    headers: HeaderMap,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let identity = Identity::from_headers(&headers).ok();
    let request = request.into_inner().data(identity);
    app.schema.execute(request).await.into()
}
