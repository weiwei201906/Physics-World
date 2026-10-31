//! JSON WebSocket protocol between battle clients and the service.
//!
//! The frame names mirror `frontend/src/api/battle.ts`: the lobby sends
//! `find_match`, the room sends `join_room`/`answer`, and the server replies
//! with `match_found`, `room_state`, `question`, `answer_result` and `game_over`.

use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    FindMatch {
        #[serde(default)]
        mode: Option<String>,
    },
    #[serde(alias = "leave")]
    CancelMatch,
    JoinRoom {
        #[serde(rename = "roomId", alias = "room_id")]
        room_id: Uuid,
    },
    Answer {
        #[serde(rename = "questionIndex", alias = "question_index")]
        question_index: usize,
        #[serde(rename = "optionId", alias = "option", alias = "option_id")]
        option_id: String,
    },
    TokenUpdate {
        token: String,
    },
}

impl ClientMessage {
    pub fn parse(raw: &str) -> Result<Self> {
        serde_json::from_str(raw)
            .map_err(|error| AppError::invalid(format!("invalid client message: {error}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlePlayerView {
    pub user_id: Uuid,
    pub nickname: String,
    pub score: i32,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub correct_count: i32,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleOptionView {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleQuestionBody {
    pub id: String,
    pub stem: String,
    pub options: Vec<BattleOptionView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleQuestionView {
    pub index: usize,
    pub total: usize,
    pub seconds: Option<i64>,
    pub question: BattleQuestionBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    MatchFound {
        #[serde(rename = "roomId")]
        room_id: Uuid,
    },
    RoomState {
        #[serde(rename = "roomId")]
        room_id: Uuid,
        status: String,
        players: Vec<BattlePlayerView>,
        #[serde(rename = "currentIndex")]
        current_index: usize,
        #[serde(rename = "totalQuestions")]
        total_questions: usize,
    },
    Question {
        index: usize,
        total: usize,
        seconds: Option<i64>,
        question: BattleQuestionBody,
    },
    AnswerResult {
        index: usize,
        correct: bool,
        #[serde(rename = "correctOptionId")]
        correct_option_id: Option<String>,
        score: i32,
        #[serde(rename = "totalScore")]
        total_score: i32,
        explanation: Option<String>,
    },
    GameOver {
        #[serde(rename = "winnerId")]
        winner_id: Option<Uuid>,
        players: Vec<BattlePlayerView>,
        #[serde(rename = "finalScore")]
        final_score: i32,
    },
    Error {
        code: String,
        message: String,
    },
}

impl ServerMessage {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|error| AppError::internal(format!("failed to serialize message: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_client_message_variants() {
        assert_eq!(
            ClientMessage::parse(r#"{"type":"find_match","mode":"1v1"}"#).expect("find"),
            ClientMessage::FindMatch {
                mode: Some("1v1".to_string())
            }
        );
        assert_eq!(
            ClientMessage::parse(r#"{"type":"cancel_match"}"#).expect("cancel"),
            ClientMessage::CancelMatch
        );
        assert_eq!(
            ClientMessage::parse(r#"{"type":"token_update","token":"abc"}"#).expect("token"),
            ClientMessage::TokenUpdate {
                token: "abc".to_string()
            }
        );
    }

    #[test]
    fn parses_answer_with_camel_case_and_snake_case_fields() {
        let room_id = Uuid::new_v4();
        let camel = ClientMessage::parse(r#"{"type":"answer","questionIndex":2,"optionId":"b"}"#)
            .expect("camel answer");
        let snake = ClientMessage::parse(r#"{"type":"answer","question_index":2,"option":"b"}"#)
            .expect("snake answer");
        assert_eq!(camel, snake);
        assert_eq!(
            ClientMessage::parse(&format!(r#"{{"type":"join_room","roomId":"{room_id}"}}"#))
                .expect("join"),
            ClientMessage::JoinRoom { room_id }
        );
    }

    #[test]
    fn rejects_unknown_message_types() {
        let error = ClientMessage::parse(r#"{"type":"unknown"}"#).expect_err("unknown");
        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn serializes_error_messages() {
        let message = ServerMessage::error("invalid_request", "bad payload");
        assert_eq!(
            message.to_json().expect("json"),
            r#"{"type":"error","code":"invalid_request","message":"bad payload"}"#
        );
    }

    #[test]
    fn serializes_match_found_frames() {
        let room_id = Uuid::new_v4();
        let message = ServerMessage::MatchFound { room_id };
        let json = message.to_json().expect("json");
        assert_eq!(
            json,
            format!(r#"{{"type":"match_found","roomId":"{room_id}"}}"#)
        );
    }
}
