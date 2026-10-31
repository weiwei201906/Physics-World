use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use common::clock::{Clock, FakeClock};
use common::config::ConfigLoader;
use common::settings::UserApiSettings;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use user_api::store::MemoryStore;
use user_api::{router, App, AppState};
use uuid::Uuid;

fn harness() -> (App, Arc<FakeClock>) {
    let mut settings = UserApiSettings::from_config(&ConfigLoader::new("user_api", HashMap::new()));
    settings.session_max_questions = 5;
    let clock = Arc::new(FakeClock::new(Utc::now()));
    let state = AppState::new(
        Arc::new(MemoryStore::seeded()),
        settings,
        clock.clone() as Arc<dyn Clock>,
    );
    (App::new(state), clock)
}

async fn post_graphql(app: &App, user: Option<(Uuid, &str)>, query: &str) -> Value {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");
    if let Some((user_id, roles)) = user {
        builder = builder
            .header("x-user-id", user_id.to_string())
            .header("x-user-roles", roles);
    }
    let body =
        Body::from(serde_json::to_vec(&json!({ "query": query })).expect("graphql request body"));
    let response = router(app.clone())
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("graphql json")
}

async fn get(app: &App, uri: &str) -> (StatusCode, Value) {
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json")
    };
    (status, value)
}

#[tokio::test]
async fn health_and_config_snapshot_are_available() {
    let (app, _) = harness();
    let (status, body) = get(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["service"], "user-api");

    let (status, body) = get(&app, "/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], 0);
    let entries = body["entries"].as_array().expect("entries");
    assert!(!entries.is_empty());
}

#[tokio::test]
async fn chapters_are_seeded_from_the_question_bank() {
    let (app, _) = harness();
    let body = post_graphql(&app, None, "{ chapters { id name questionCount } }").await;
    let chapters = body["data"]["chapters"].as_array().expect("chapters");
    assert_eq!(chapters.len(), 8);
    assert_eq!(chapters[0]["id"], "mechanics");
    assert_eq!(chapters[0]["questionCount"], 3);
}

#[tokio::test]
async fn full_quiz_flow_scores_and_completes_the_session() {
    let (app, _) = harness();
    let user = Uuid::new_v4();

    let started = post_graphql(
        &app,
        Some((user, "student")),
        r#"mutation { startQuizSession(mode: CHAPTER, chapterId: "mechanics") {
            id mode chapterId status currentIndex totalQuestions
        } }"#,
    )
    .await;
    let session = &started["data"]["startQuizSession"];
    let session_id = session["id"].as_str().expect("session id").to_string();
    assert_eq!(session["mode"], "CHAPTER");
    assert_eq!(session["chapterId"], "mechanics");
    assert_eq!(session["status"], "ACTIVE");
    assert_eq!(session["totalQuestions"], 3);

    let question = post_graphql(
        &app,
        Some((user, "student")),
        &format!(
            r#"query {{ sessionQuestion(sessionId: "{}", index: 0) {{
                id chapterId content {{ stem options {{ id label text }} }}
            }} }}"#,
            session_id
        ),
    )
    .await;
    let first_question = &question["data"]["sessionQuestion"];
    assert_eq!(first_question["id"], "mechanics-1");
    assert_eq!(
        first_question["content"]["options"]
            .as_array()
            .expect("options")
            .len(),
        4
    );

    let mut attempt_ids = Vec::new();
    for (question_id, answer) in [
        ("mechanics-1", "a"),
        ("mechanics-2", "b"),
        ("mechanics-3", "c"),
    ] {
        let submitted = post_graphql(
            &app,
            Some((user, "student")),
            &format!(
                r#"mutation {{ submitAttempt(input: {{
                    sessionId: "{}", questionId: "{}", answer: "{}"
                }}) {{ accepted attemptId correct correctOptionId explanation }} }}"#,
                session_id, question_id, answer
            ),
        )
        .await;
        let payload = &submitted["data"]["submitAttempt"];
        assert_eq!(payload["accepted"], true);
        assert_eq!(payload["correct"], true);
        assert_eq!(payload["correctOptionId"], answer);
        attempt_ids.push(
            payload["attemptId"]
                .as_str()
                .expect("attempt id")
                .to_string(),
        );
    }

    let duplicate = post_graphql(
        &app,
        Some((user, "student")),
        &format!(
            r#"mutation {{ submitAttempt(input: {{
                sessionId: "{}", questionId: "mechanics-1", answer: "a"
            }}) {{ accepted attemptId correct }} }}"#,
            session_id
        ),
    )
    .await;
    assert_eq!(
        duplicate["data"]["submitAttempt"]["attemptId"],
        attempt_ids[0]
    );
    assert_eq!(duplicate["data"]["submitAttempt"]["correct"], true);

    let finished = post_graphql(
        &app,
        Some((user, "student")),
        &format!(
            r#"query {{ quizSession(id: "{}") {{ status currentIndex totalQuestions completedAt }} }}"#,
            session_id
        ),
    )
    .await;
    let session = &finished["data"]["quizSession"];
    assert_eq!(session["status"], "COMPLETED");
    assert_eq!(session["currentIndex"], 3);
    assert!(session["completedAt"].is_string());

    let leaderboard = post_graphql(
        &app,
        None,
        "{ leaderboard(period: DAILY, page: 1, size: 10) { total entries { rank userId score nickname } } }",
    )
    .await;
    let page = &leaderboard["data"]["leaderboard"];
    assert_eq!(page["total"], 1);
    assert_eq!(page["entries"][0]["rank"], 1);
    assert_eq!(page["entries"][0]["score"], 300);
    assert_eq!(page["entries"][0]["userId"], user.to_string());

    let rank = post_graphql(
        &app,
        Some((user, "student")),
        "{ myRank(period: DAILY) { rank score } }",
    )
    .await;
    assert_eq!(rank["data"]["myRank"]["rank"], 1);
    assert_eq!(rank["data"]["myRank"]["score"], 300);
}

#[tokio::test]
async fn identity_is_required_for_personal_queries() {
    let (app, _) = harness();
    let body = post_graphql(&app, None, "{ me { id nickname } }").await;
    assert!(body["data"]["me"].is_null());
    assert_eq!(body["errors"][0]["message"], "unauthenticated");
}

#[tokio::test]
async fn profile_can_be_updated() {
    let (app, _) = harness();
    let user = Uuid::new_v4();
    let body = post_graphql(
        &app,
        Some((user, "student")),
        r#"mutation { updateProfile(input: { nickname: "Ada" }) { id nickname avatarUrl } }"#,
    )
    .await;
    assert_eq!(body["data"]["updateProfile"]["nickname"], "Ada");
    assert_eq!(body["data"]["updateProfile"]["id"], user.to_string());
}

#[tokio::test]
async fn admin_mutations_require_the_admin_role() {
    let (app, _) = harness();
    let student = Uuid::new_v4();
    let denied = post_graphql(
        &app,
        Some((student, "student")),
        r#"mutation { adminUpsertConfig(namespace: "quiz", key: "session.max_questions", value: "60") {
            applied
        } }"#,
    )
    .await;
    assert!(denied["errors"][0]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("forbidden"));

    let admin = Uuid::new_v4();
    let applied = post_graphql(
        &app,
        Some((admin, "admin")),
        r#"mutation { adminUpsertConfig(namespace: "quiz", key: "session.max_questions", value: "60") {
            applied degraded entry { namespace key value }
        } }"#,
    )
    .await;
    let payload = &applied["data"]["adminUpsertConfig"];
    assert_eq!(payload["applied"], true);
    assert_eq!(payload["degraded"], false);
    assert_eq!(payload["entry"]["value"], "60");

    let listing = post_graphql(
        &app,
        Some((admin, "admin")),
        r#"query { adminConfigs(namespace: "quiz") { key value } }"#,
    )
    .await;
    let configs = listing["data"]["adminConfigs"].as_array().expect("configs");
    let max_questions = configs
        .iter()
        .find(|entry| entry["key"] == "session.max_questions")
        .expect("updated key");
    assert_eq!(max_questions["value"], "60");

    let (_, snapshot) = get(&app, "/config").await;
    assert_eq!(snapshot["revision"], 1);
}

#[tokio::test]
async fn admin_can_upload_and_soft_delete_questions() {
    let (app, _) = harness();
    let admin = Uuid::new_v4();
    let uploaded = post_graphql(
        &app,
        Some((admin, "admin")),
        r#"mutation { adminUploadQuestion(input: {
            chapterId: "optics", difficulty: 2,
            content: {
                stem: "Which lens converges parallel rays?",
                explanation: "A convex lens converges rays.",
                correctOptionId: "a",
                options: [
                    { id: "a", label: "A", text: "Convex lens" },
                    { id: "b", label: "B", text: "Concave lens" },
                    { id: "c", label: "C", text: "Plane mirror" },
                    { id: "d", label: "D", text: "Prism" }
                ]
            }
        }) { id questionId version } }"#,
    )
    .await;
    let version = &uploaded["data"]["adminUploadQuestion"];
    assert_eq!(version["version"], 1);
    let question_id = version["questionId"]
        .as_str()
        .expect("question id")
        .to_string();

    let deleted = post_graphql(
        &app,
        Some((admin, "admin")),
        &format!(
            r#"mutation {{ adminSoftDeleteQuestion(questionId: "{}") }}"#,
            question_id
        ),
    )
    .await;
    assert_eq!(deleted["data"]["adminSoftDeleteQuestion"], true);

    let chapters = post_graphql(&app, None, "{ chapters { id questionCount } }").await;
    let optics = chapters["data"]["chapters"]
        .as_array()
        .expect("chapters")
        .iter()
        .find(|chapter| chapter["id"] == "optics")
        .expect("optics");
    assert_eq!(optics["questionCount"], 3);
}
