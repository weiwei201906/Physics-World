//! JSON WebSocket protocol between battle clients and the service.

use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::{BattleResult, QuestionView, RoomView};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Enqueue,
    Answer {
        #[serde(rename = "roomId", alias = "room_id")]
        room_id: Uuid,
        #[serde(rename = "questionIndex", alias = "question_index")]
        question_index: usize,
        option: String,
    },
    Leave,
    Token {
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Matched {
        #[serde(rename = "roomId")]
        room_id: Uuid,
        room: RoomView,
        question: QuestionView,
    },
    Question {
        #[serde(rename = "roomId")]
        room_id: Uuid,
        question: QuestionView,
    },
    Score {
        #[serde(rename = "roomId")]
        room_id: Uuid,
        #[serde(rename = "playerId")]
        player_id: Uuid,
        #[serde(rename = "questionIndex")]
        question_index: usize,
        correct: bool,
        points: i32,
        #[serde(rename = "speedBonus")]
        speed_bonus: i32,
        #[serde(rename = "totalScore")]
        total_score: i32,
    },
    Finished {
        #[serde(rename = "roomId")]
        room_id: Uuid,
        result: BattleResult,
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
            ClientMessage::parse(r#"{"type":"enqueue"}"#).expect("enqueue"),
            ClientMessage::Enqueue
        );
        assert_eq!(
            ClientMessage::parse(r#"{"type":"leave"}"#).expect("leave"),
            ClientMessage::Leave
        );
        assert_eq!(
            ClientMessage::parse(r#"{"type":"token","token":"abc"}"#).expect("token"),
            ClientMessage::Token {
                token: "abc".to_string()
            }
        );
    }

    #[test]
    fn parses_answer_with_camel_case_and_snake_case_fields() {
        let room_id = Uuid::new_v4();
        let camel = ClientMessage::parse(&format!(
            r#"{{"type":"answer","roomId":"{room_id}","questionIndex":2,"option":"b"}}"#
        ))
        .expect("camel answer");
        let snake = ClientMessage::parse(&format!(
            r#"{{"type":"answer","room_id":"{room_id}","question_index":2,"option":"b"}}"#
        ))
        .expect("snake answer");
        assert_eq!(camel, snake);
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
}
