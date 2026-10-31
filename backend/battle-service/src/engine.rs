//! Pure battle state machine: matchmaking, room phases, answer scoring and
//! settlement. The engine holds no I/O handles except an injectable clock, so
//! unit tests run it deterministically with `common::clock::FakeClock`.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::clock::Clock;
use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::settings::Settings;
use crate::vector::PlayerVector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomPhase {
    Waiting,
    Matching,
    Playing,
    Settling,
    Finished,
}

impl RoomPhase {
    pub fn as_status(self) -> &'static str {
        match self {
            RoomPhase::Waiting => "waiting",
            RoomPhase::Matching => "matching",
            RoomPhase::Playing => "playing",
            RoomPhase::Settling => "settling",
            RoomPhase::Finished => "finished",
        }
    }

    pub fn can_transition_to(self, next: RoomPhase) -> bool {
        matches!(
            (self, next),
            (RoomPhase::Waiting, RoomPhase::Matching)
                | (RoomPhase::Matching, RoomPhase::Playing)
                | (RoomPhase::Playing, RoomPhase::Settling)
                | (RoomPhase::Settling, RoomPhase::Finished)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleOption {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSpec {
    pub id: String,
    pub correct_option: String,
    pub stem: String,
    pub options: Vec<BattleOption>,
    pub explanation: String,
}

impl QuestionSpec {
    pub fn new(
        id: impl Into<String>,
        correct_option: impl Into<String>,
        stem: impl Into<String>,
        options: Vec<BattleOption>,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            correct_option: correct_option.into(),
            stem: stem.into(),
            options,
            explanation: explanation.into(),
        }
    }

    pub fn demo(id: impl Into<String>, correct_option: &str) -> Self {
        let id = id.into();
        let options = ["a", "b", "c", "d"]
            .iter()
            .map(|label| BattleOption {
                id: (*label).to_string(),
                text: format!("Option {label}"),
            })
            .collect();
        Self::new(
            &id,
            correct_option,
            format!("Demo question {id}"),
            options,
            String::new(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub player_id: Uuid,
    pub nickname: String,
    pub score: i32,
    pub correct_answers: u32,
    pub answered_questions: u32,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    pub room_id: Uuid,
    pub phase: RoomPhase,
    pub players: Vec<PlayerSnapshot>,
    pub current_index: usize,
    pub total_questions: usize,
    pub question_id: Option<String>,
    pub question_deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionView {
    pub index: usize,
    pub total: usize,
    pub seconds: i64,
    pub id: String,
    pub stem: String,
    pub options: Vec<BattleOption>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerResult {
    pub rank: usize,
    pub player_id: Uuid,
    pub nickname: String,
    pub score: i32,
    pub correct_answers: u32,
    pub answered_questions: u32,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleResult {
    pub room_id: Uuid,
    pub finished_at: DateTime<Utc>,
    pub winner: Option<Uuid>,
    pub players: Vec<PlayerResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerOutcome {
    pub room_id: Uuid,
    pub question_index: usize,
    pub applied: bool,
    pub duplicate: bool,
    pub correct: bool,
    pub points: i32,
    pub speed_bonus: i32,
    pub total_score: i32,
    pub phase: RoomPhase,
}

#[derive(Debug, Clone)]
pub enum EnqueueOutcome {
    Waiting { room_id: Uuid },
    Matched { room_id: Uuid, room: RoomView },
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Matched {
        room_id: Uuid,
        room: RoomView,
    },
    Question {
        room_id: Uuid,
        question: QuestionView,
    },
    Score {
        room_id: Uuid,
        player_id: Uuid,
        question_index: usize,
        correct: bool,
        correct_option_id: String,
        explanation: String,
        points: i32,
        speed_bonus: i32,
        total_score: i32,
        room: RoomView,
    },
    Finished {
        room_id: Uuid,
        result: BattleResult,
    },
    QueueTimeout {
        room_id: Uuid,
        player_id: Uuid,
    },
    RoomExpired {
        room_id: Uuid,
    },
}

#[derive(Debug, Clone)]
struct PlayerState {
    nickname: String,
    score: i32,
    correct_answers: u32,
    answered_questions: u32,
    connected: bool,
}

impl PlayerState {
    fn new(nickname: String) -> Self {
        Self {
            nickname,
            score: 0,
            correct_answers: 0,
            answered_questions: 0,
            connected: true,
        }
    }
}

#[derive(Debug, Clone)]
struct AnswerRecord {
    correct: bool,
    points: i32,
    speed_bonus: i32,
}

#[derive(Debug)]
struct Room {
    id: Uuid,
    phase: RoomPhase,
    players: BTreeMap<Uuid, PlayerState>,
    questions: Vec<QuestionSpec>,
    current_index: usize,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    question_started_at: Option<DateTime<Utc>>,
    question_deadline: Option<DateTime<Utc>>,
    total_deadline: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    answers: HashMap<(usize, Uuid), AnswerRecord>,
}

impl Room {
    fn new(id: Uuid, player: Uuid, nickname: String, created_at: DateTime<Utc>) -> Self {
        let mut players = BTreeMap::new();
        players.insert(player, PlayerState::new(nickname));
        Self {
            id,
            phase: RoomPhase::Waiting,
            players,
            questions: Vec::new(),
            current_index: 0,
            created_at,
            started_at: None,
            question_started_at: None,
            question_deadline: None,
            total_deadline: None,
            finished_at: None,
            answers: HashMap::new(),
        }
    }

    fn transition_to(&mut self, next: RoomPhase) -> Result<()> {
        if !self.phase.can_transition_to(next) {
            return Err(AppError::internal(format!(
                "invalid room transition {:?} -> {:?}",
                self.phase, next
            )));
        }
        self.phase = next;
        Ok(())
    }

    fn view(&self) -> RoomView {
        RoomView {
            room_id: self.id,
            phase: self.phase,
            players: self
                .players
                .iter()
                .map(|(player_id, state)| PlayerSnapshot {
                    player_id: *player_id,
                    nickname: state.nickname.clone(),
                    score: state.score,
                    correct_answers: state.correct_answers,
                    answered_questions: state.answered_questions,
                    connected: state.connected,
                })
                .collect(),
            current_index: self.current_index,
            total_questions: self.questions.len(),
            question_id: self
                .questions
                .get(self.current_index)
                .map(|question| question.id.clone()),
            question_deadline: self.question_deadline,
        }
    }

    fn current_question(&self) -> Option<&QuestionSpec> {
        self.questions.get(self.current_index)
    }

    fn current_question_view(&self, now: DateTime<Utc>) -> Option<QuestionView> {
        let question = self.current_question()?;
        let deadline = self.question_deadline?;
        Some(QuestionView {
            index: self.current_index,
            total: self.questions.len(),
            seconds: (deadline - now).num_seconds().max(0),
            id: question.id.clone(),
            stem: question.stem.clone(),
            options: question.options.clone(),
            explanation: question.explanation.clone(),
        })
    }

    fn all_connected_answered(&self) -> bool {
        let connected: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, state)| state.connected)
            .map(|(player_id, _)| *player_id)
            .collect();
        !connected.is_empty()
            && connected
                .iter()
                .all(|player_id| self.answers.contains_key(&(self.current_index, *player_id)))
    }

    fn result(&self) -> BattleResult {
        let mut ranked: Vec<PlayerResult> = self
            .players
            .iter()
            .map(|(player_id, state)| PlayerResult {
                rank: 0,
                player_id: *player_id,
                nickname: state.nickname.clone(),
                score: state.score,
                correct_answers: state.correct_answers,
                answered_questions: state.answered_questions,
                connected: state.connected,
            })
            .collect();
        ranked.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.player_id.cmp(&right.player_id))
        });
        for (index, player) in ranked.iter_mut().enumerate() {
            player.rank = index + 1;
        }
        let winner = match (ranked.first(), ranked.get(1)) {
            (Some(first), Some(second)) if first.score == second.score => None,
            (Some(first), _) => Some(first.player_id),
            (None, _) => None,
        };
        BattleResult {
            room_id: self.id,
            finished_at: self.finished_at.unwrap_or(self.created_at),
            winner,
            players: ranked,
        }
    }
}

pub struct BattleEngine {
    settings: Settings,
    clock: Arc<dyn Clock>,
    catalog: Vec<QuestionSpec>,
    rooms: HashMap<Uuid, Room>,
    queue: VecDeque<Uuid>,
    player_room: HashMap<Uuid, Uuid>,
    player_vectors: HashMap<Uuid, PlayerVector>,
}

impl BattleEngine {
    pub fn new(settings: Settings, clock: Arc<dyn Clock>, catalog: Vec<QuestionSpec>) -> Self {
        Self {
            settings,
            clock,
            catalog,
            rooms: HashMap::new(),
            queue: VecDeque::new(),
            player_room: HashMap::new(),
            player_vectors: HashMap::new(),
        }
    }

    pub fn enqueue(&mut self, player: Uuid) -> Result<(EnqueueOutcome, Vec<EngineEvent>)> {
        self.enqueue_with_profile(player, None, PlayerVector::default())
    }

    pub fn enqueue_with_vector(
        &mut self,
        player: Uuid,
        vector: PlayerVector,
    ) -> Result<(EnqueueOutcome, Vec<EngineEvent>)> {
        self.enqueue_with_profile(player, None, vector)
    }

    /// Queues `player` and pairs it with the waiting opponent whose error
    /// vector is most cosine-similar. Zero vectors fall back to FIFO order.
    pub fn enqueue_with_profile(
        &mut self,
        player: Uuid,
        nickname: Option<String>,
        vector: PlayerVector,
    ) -> Result<(EnqueueOutcome, Vec<EngineEvent>)> {
        let nickname = nickname
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Player".to_string());
        let now = self.clock.now();
        let mut events = self.expire_waiting(now);

        if let Some(room_id) = self.player_room.get(&player).copied() {
            let active = self
                .rooms
                .get(&room_id)
                .map(|room| room.phase != RoomPhase::Finished)
                .unwrap_or(false);
            if active {
                return Err(AppError::conflict("player is already in a room"));
            }
            self.player_room.remove(&player);
        }

        self.player_vectors.insert(player, vector.clone());

        if let Some(room_id) = self.select_waiting_room(&vector) {
            let questions = self.build_questions()?;
            let room = self
                .rooms
                .get_mut(&room_id)
                .ok_or_else(|| AppError::internal("waiting room disappeared"))?;
            room.players.insert(player, PlayerState::new(nickname));
            room.transition_to(RoomPhase::Matching)?;
            room.questions = questions;
            room.current_index = 0;
            room.started_at = Some(now);
            room.question_started_at = Some(now);
            room.question_deadline = Some(now + self.settings.per_question_duration());
            room.total_deadline = Some(now + self.settings.total_duration());
            room.transition_to(RoomPhase::Playing)?;
            self.player_room.insert(player, room_id);

            let view = room.view();
            let question = room
                .current_question_view(now)
                .ok_or_else(|| AppError::internal("room has no first question"))?;
            events.push(EngineEvent::Matched {
                room_id,
                room: view.clone(),
            });
            events.push(EngineEvent::Question { room_id, question });
            return Ok((
                EnqueueOutcome::Matched {
                    room_id,
                    room: view,
                },
                events,
            ));
        }

        let room_id = Uuid::new_v4();
        let room = Room::new(room_id, player, nickname, now);
        self.rooms.insert(room_id, room);
        self.queue.push_back(room_id);
        self.player_room.insert(player, room_id);
        Ok((EnqueueOutcome::Waiting { room_id }, events))
    }

    /// Re-attaches a player that already owns a seat in `room_id`.
    pub fn join(
        &mut self,
        player: Uuid,
        room_id: Uuid,
    ) -> Result<(RoomView, Option<QuestionView>)> {
        let now = self.clock.now();
        let room = self
            .rooms
            .get_mut(&room_id)
            .ok_or_else(|| AppError::not_found("room not found"))?;
        let state = room
            .players
            .get_mut(&player)
            .ok_or_else(|| AppError::forbidden("player is not a room member"))?;
        state.connected = true;
        self.player_room.insert(player, room_id);
        let view = room.view();
        let question = if room.phase == RoomPhase::Playing {
            room.current_question_view(now)
        } else {
            None
        };
        Ok((view, question))
    }

    pub fn answer(
        &mut self,
        player: Uuid,
        room_id: Uuid,
        question_index: usize,
        option: String,
    ) -> Result<(AnswerOutcome, Vec<EngineEvent>)> {
        if let Some(record) = self
            .rooms
            .get(&room_id)
            .and_then(|room| room.answers.get(&(question_index, player)))
            .cloned()
        {
            let total_score = self
                .rooms
                .get(&room_id)
                .and_then(|room| room.players.get(&player))
                .map(|state| state.score)
                .unwrap_or(0);
            let phase = self
                .rooms
                .get(&room_id)
                .map(|room| room.phase)
                .unwrap_or(RoomPhase::Finished);
            return Ok((
                AnswerOutcome {
                    room_id,
                    question_index,
                    applied: false,
                    duplicate: true,
                    correct: record.correct,
                    points: record.points,
                    speed_bonus: record.speed_bonus,
                    total_score,
                    phase,
                },
                Vec::new(),
            ));
        }

        let normalized = option.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err(AppError::invalid("answer option is empty"));
        }

        let now = self.clock.now();
        let speed_bonus_max = self.settings.speed_bonus_max;
        let (correct, correct_option_id, explanation, points, speed_bonus, total_score) = {
            let room = self
                .rooms
                .get_mut(&room_id)
                .ok_or_else(|| AppError::not_found("room not found"))?;
            if !room.players.contains_key(&player) {
                return Err(AppError::forbidden("player is not a room member"));
            }
            if room.phase != RoomPhase::Playing {
                return Err(AppError::conflict("room is not playing"));
            }
            if room.current_index != question_index {
                return Err(AppError::invalid("question is not active"));
            }
            let deadline = room
                .question_deadline
                .ok_or_else(|| AppError::internal("question deadline missing"))?;
            if now >= deadline {
                return Err(AppError::conflict("question deadline passed"));
            }
            let started_at = room
                .question_started_at
                .ok_or_else(|| AppError::internal("question start missing"))?;
            let question = room
                .current_question()
                .ok_or_else(|| AppError::internal("question missing"))?
                .clone();
            let correct = normalized == question.correct_option.to_ascii_lowercase();
            let speed_bonus = if correct {
                speed_bonus(now, started_at, deadline, speed_bonus_max)
            } else {
                0
            };
            let points = if correct {
                self.settings.score_correct + speed_bonus
            } else {
                self.settings.score_wrong
            };
            let state = room
                .players
                .get_mut(&player)
                .ok_or_else(|| AppError::internal("player state missing"))?;
            state.score += points;
            state.answered_questions += 1;
            if correct {
                state.correct_answers += 1;
            }
            let total_score = state.score;
            room.answers.insert(
                (question_index, player),
                AnswerRecord {
                    correct,
                    points,
                    speed_bonus,
                },
            );
            (
                correct,
                question.correct_option.clone(),
                question.explanation.clone(),
                points,
                speed_bonus,
                total_score,
            )
        };

        let room_view = self
            .rooms
            .get(&room_id)
            .map(Room::view)
            .ok_or_else(|| AppError::internal("room disappeared"))?;
        let mut events = vec![EngineEvent::Score {
            room_id,
            player_id: player,
            question_index,
            correct,
            correct_option_id,
            explanation,
            points,
            speed_bonus,
            total_score,
            room: room_view,
        }];
        let all_answered = self
            .rooms
            .get(&room_id)
            .map(|room| room.all_connected_answered())
            .unwrap_or(false);
        if all_answered {
            events.extend(self.advance_room(room_id, now));
        }
        let phase = self
            .rooms
            .get(&room_id)
            .map(|room| room.phase)
            .unwrap_or(RoomPhase::Finished);
        Ok((
            AnswerOutcome {
                room_id,
                question_index,
                applied: true,
                duplicate: false,
                correct,
                points,
                speed_bonus,
                total_score,
                phase,
            },
            events,
        ))
    }

    pub fn leave(&mut self, player: Uuid) -> Result<Vec<EngineEvent>> {
        let Some(room_id) = self.player_room.get(&player).copied() else {
            return Ok(Vec::new());
        };
        let now = self.clock.now();
        let phase = self
            .rooms
            .get(&room_id)
            .map(|room| room.phase)
            .unwrap_or(RoomPhase::Finished);

        if matches!(phase, RoomPhase::Waiting | RoomPhase::Matching) {
            self.rooms.remove(&room_id);
            self.queue.retain(|queued| *queued != room_id);
            self.player_room.remove(&player);
            self.player_vectors.remove(&player);
            return Ok(vec![EngineEvent::RoomExpired { room_id }]);
        }

        if phase == RoomPhase::Playing {
            // A browser tab navigating between the lobby and the room tears
            // down the old socket right before the new one re-joins, so a
            // disconnect only marks presence; settle once everyone is gone.
            let all_disconnected = {
                let room = self
                    .rooms
                    .get_mut(&room_id)
                    .ok_or_else(|| AppError::not_found("room not found"))?;
                if let Some(state) = room.players.get_mut(&player) {
                    state.connected = false;
                }
                room.players.values().all(|state| !state.connected)
            };
            if all_disconnected {
                if let Some(result) = self.settle_room(room_id, now) {
                    return Ok(vec![EngineEvent::Finished { room_id, result }]);
                }
            }
            return Ok(Vec::new());
        }

        self.player_room.remove(&player);
        Ok(Vec::new())
    }

    pub fn tick(&mut self) -> Vec<EngineEvent> {
        let now = self.clock.now();
        let mut events = self.expire_waiting(now);

        let playing: Vec<Uuid> = self
            .rooms
            .iter()
            .filter(|(_, room)| room.phase == RoomPhase::Playing)
            .map(|(room_id, _)| *room_id)
            .collect();
        for room_id in playing {
            events.extend(self.advance_room(room_id, now));
        }

        let ttl = self.settings.room_ttl();
        let expired: Vec<Uuid> = self
            .rooms
            .iter()
            .filter(|(_, room)| {
                room.phase == RoomPhase::Finished
                    && room
                        .finished_at
                        .map(|finished_at| finished_at + ttl <= now)
                        .unwrap_or(false)
            })
            .map(|(room_id, _)| *room_id)
            .collect();
        for room_id in expired {
            self.rooms.remove(&room_id);
            events.push(EngineEvent::RoomExpired { room_id });
        }
        events
    }

    pub fn view(&self, room_id: Uuid) -> Option<RoomView> {
        self.rooms.get(&room_id).map(Room::view)
    }

    /// Picks the waiting room whose occupant is most cosine-similar to the
    /// incoming player. Ties and zero-similarity fall back to queue order.
    fn select_waiting_room(&mut self, incoming: &PlayerVector) -> Option<Uuid> {
        let mut best: Option<(Uuid, f64)> = None;
        let mut fallback: Option<Uuid> = None;
        let mut stale: Vec<Uuid> = Vec::new();

        for room_id in self.queue.iter().copied() {
            let valid = self
                .rooms
                .get(&room_id)
                .map(|room| room.phase == RoomPhase::Waiting && room.players.len() == 1)
                .unwrap_or(false);
            if !valid {
                stale.push(room_id);
                continue;
            }
            if fallback.is_none() {
                fallback = Some(room_id);
            }
            let similarity = self
                .rooms
                .get(&room_id)
                .and_then(|room| room.players.keys().next().copied())
                .and_then(|player| self.player_vectors.get(&player))
                .map(|vector| incoming.cosine(vector))
                .unwrap_or(0.0);
            match best {
                Some((_, score)) if score >= similarity => {}
                _ => best = Some((room_id, similarity)),
            }
        }

        let chosen = match best {
            Some((room_id, score)) if score > 0.0 => room_id,
            _ => fallback?,
        };
        stale.push(chosen);
        self.queue.retain(|queued| !stale.contains(queued));
        Some(chosen)
    }

    fn build_questions(&self) -> Result<Vec<QuestionSpec>> {
        if self.catalog.is_empty() {
            return Err(AppError::unavailable("question catalog is empty"));
        }
        let per_room = self.settings.questions_per_room.max(1) as usize;
        let catalog_len = self.catalog.len();
        Ok((0..per_room)
            .map(|index| self.catalog[index % catalog_len].clone())
            .collect())
    }

    fn advance_room(&mut self, room_id: Uuid, now: DateTime<Utc>) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let settle = {
            let Some(room) = self.rooms.get_mut(&room_id) else {
                return events;
            };
            if room.phase != RoomPhase::Playing {
                return events;
            }
            let total_expired = room
                .total_deadline
                .map(|deadline| now >= deadline)
                .unwrap_or(false);
            let question_expired = room
                .question_deadline
                .map(|deadline| now >= deadline)
                .unwrap_or(false);
            let all_answered = room.all_connected_answered();
            if !(total_expired || question_expired || all_answered) {
                return events;
            }
            let last_question = room.current_index + 1 >= room.questions.len();
            if total_expired || last_question {
                true
            } else {
                room.current_index += 1;
                room.question_started_at = Some(now);
                let deadline = now + self.settings.per_question_duration();
                room.question_deadline = Some(deadline);
                if let Some(question) = room.current_question_view(now) {
                    events.push(EngineEvent::Question { room_id, question });
                }
                false
            }
        };

        if settle {
            if let Some(result) = self.settle_room(room_id, now) {
                events.push(EngineEvent::Finished { room_id, result });
            }
        }
        events
    }

    fn settle_room(&mut self, room_id: Uuid, now: DateTime<Utc>) -> Option<BattleResult> {
        let room = self.rooms.get_mut(&room_id)?;
        if room.phase == RoomPhase::Finished {
            return None;
        }
        if room.phase == RoomPhase::Playing {
            room.transition_to(RoomPhase::Settling).ok()?;
        }
        if room.phase == RoomPhase::Settling {
            room.transition_to(RoomPhase::Finished).ok()?;
        }
        room.finished_at = Some(now);
        let result = room.result();
        let players: Vec<Uuid> = room.players.keys().copied().collect();
        for player in players {
            if self.player_room.get(&player) == Some(&room_id) {
                self.player_room.remove(&player);
            }
            self.player_vectors.remove(&player);
        }
        Some(result)
    }

    fn expire_waiting(&mut self, now: DateTime<Utc>) -> Vec<EngineEvent> {
        let timeout = self.settings.queue_timeout();
        let expired: Vec<(Uuid, Uuid)> = self
            .rooms
            .iter()
            .filter(|(_, room)| {
                room.phase == RoomPhase::Waiting && room.created_at + timeout <= now
            })
            .filter_map(|(room_id, room)| {
                room.players
                    .keys()
                    .next()
                    .copied()
                    .map(|player_id| (*room_id, player_id))
            })
            .collect();

        let mut events = Vec::new();
        for (room_id, player_id) in expired {
            self.rooms.remove(&room_id);
            self.queue.retain(|queued| *queued != room_id);
            self.player_room.remove(&player_id);
            self.player_vectors.remove(&player_id);
            events.push(EngineEvent::QueueTimeout { room_id, player_id });
            events.push(EngineEvent::RoomExpired { room_id });
        }
        events
    }
}

fn speed_bonus(
    now: DateTime<Utc>,
    started_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    max_bonus: i32,
) -> i32 {
    let total_ms = (deadline - started_at).num_milliseconds().max(1);
    let remaining_ms = (deadline - now).num_milliseconds().max(0);
    let bonus = i64::from(max_bonus.max(0)) * remaining_ms / total_ms;
    (bonus as i32).clamp(0, max_bonus.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use common::clock::FakeClock;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 31, 8, 0, 0)
            .single()
            .unwrap_or_default()
    }

    fn catalog() -> Vec<QuestionSpec> {
        ["a", "b", "c", "d", "a", "b"]
            .iter()
            .enumerate()
            .map(|(index, option)| QuestionSpec::demo(format!("q-{}", index + 1), option))
            .collect()
    }

    fn engine() -> (Arc<FakeClock>, BattleEngine) {
        let clock = Arc::new(FakeClock::new(now()));
        let engine = BattleEngine::new(Settings::default(), clock.clone(), catalog());
        (clock, engine)
    }

    fn matched_room(engine: &mut BattleEngine, first: Uuid, second: Uuid) -> Uuid {
        let (outcome, _) = engine.enqueue(first).expect("first enqueue");
        assert!(matches!(outcome, EnqueueOutcome::Waiting { .. }));
        let (outcome, events) = engine.enqueue(second).expect("second enqueue");
        match outcome {
            EnqueueOutcome::Matched { room_id, room } => {
                assert_eq!(room.phase, RoomPhase::Playing);
                assert_eq!(room.players.len(), 2);
                assert_eq!(room.current_index, 0);
                assert!(matches!(events[0], EngineEvent::Matched { .. }));
                assert!(matches!(events[1], EngineEvent::Question { .. }));
                room_id
            }
            EnqueueOutcome::Waiting { .. } => panic!("expected a match"),
        }
    }

    fn correct_for(index: usize) -> String {
        catalog()[index].correct_option.clone()
    }

    #[test]
    fn phase_transitions_are_linear() {
        assert!(RoomPhase::Waiting.can_transition_to(RoomPhase::Matching));
        assert!(RoomPhase::Matching.can_transition_to(RoomPhase::Playing));
        assert!(RoomPhase::Playing.can_transition_to(RoomPhase::Settling));
        assert!(RoomPhase::Settling.can_transition_to(RoomPhase::Finished));
        assert!(!RoomPhase::Waiting.can_transition_to(RoomPhase::Playing));
        assert!(!RoomPhase::Finished.can_transition_to(RoomPhase::Waiting));
        assert!(!RoomPhase::Matching.can_transition_to(RoomPhase::Finished));
    }

    #[test]
    fn matchmaking_pairs_two_waiting_players() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let view = engine.view(room_id).expect("room view");
        assert_eq!(view.phase, RoomPhase::Playing);
        assert_eq!(view.current_index, 0);
        assert_eq!(view.players.len(), 2);
        assert_eq!(view.question_id.as_deref(), Some("q-1"));
        assert!(view.question_deadline.is_some());
    }

    #[test]
    fn matchmaking_prefers_the_closest_error_vector() {
        let (clock, mut engine) = engine();
        let mechanics =
            PlayerVector::new(vec!["mechanics:1".to_owned()], vec![1.0]).expect("mechanics vector");
        let optics =
            PlayerVector::new(vec!["optics:1".to_owned()], vec![1.0]).expect("optics vector");
        let now = clock.now();

        // Seed two waiting rooms directly: in normal operation every enqueue
        // matches the first waiter, so similarity only matters when several
        // players wait at once.
        let waiting_optics = Uuid::new_v4();
        let waiting_mechanics = Uuid::new_v4();
        let optics_player = Uuid::new_v4();
        let mechanics_player = Uuid::new_v4();
        engine.rooms.insert(
            waiting_optics,
            Room::new(waiting_optics, optics_player, "Optics".to_string(), now),
        );
        engine.rooms.insert(
            waiting_mechanics,
            Room::new(
                waiting_mechanics,
                mechanics_player,
                "Mechanics".to_string(),
                now,
            ),
        );
        engine.queue.push_back(waiting_optics);
        engine.queue.push_back(waiting_mechanics);
        engine.player_room.insert(optics_player, waiting_optics);
        engine
            .player_room
            .insert(mechanics_player, waiting_mechanics);
        engine.player_vectors.insert(optics_player, optics);
        engine
            .player_vectors
            .insert(mechanics_player, mechanics.clone());

        let incoming = Uuid::new_v4();
        let (outcome, _) = engine
            .enqueue_with_vector(incoming, mechanics)
            .expect("enqueue");
        match outcome {
            EnqueueOutcome::Matched { room_id, room } => {
                assert_eq!(room_id, waiting_mechanics);
                assert_eq!(room.players.len(), 2);
            }
            EnqueueOutcome::Waiting { .. } => panic!("expected a match"),
        }
        assert_eq!(
            engine.view(waiting_optics).map(|room| room.phase),
            Some(RoomPhase::Waiting)
        );
    }

    #[test]
    fn correct_answer_adds_full_speed_bonus() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let (outcome, events) = engine
            .answer(first, room_id, 0, correct_for(0))
            .expect("answer");
        assert!(outcome.correct);
        assert_eq!(outcome.speed_bonus, 50);
        assert_eq!(outcome.points, 150);
        assert_eq!(outcome.total_score, 150);
        assert!(matches!(events[0], EngineEvent::Score { .. }));
    }

    #[test]
    fn wrong_answer_uses_configured_penalty() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let (outcome, _) = engine
            .answer(first, room_id, 0, "wrong".to_string())
            .expect("answer");
        assert!(!outcome.correct);
        assert_eq!(outcome.speed_bonus, 0);
        assert_eq!(outcome.points, crate::settings::DEFAULT_SCORE_WRONG);
        assert_eq!(outcome.total_score, crate::settings::DEFAULT_SCORE_WRONG);
    }

    #[test]
    fn speed_bonus_decays_and_is_capped() {
        let (clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        clock.advance(Duration::seconds(5));
        let (outcome, _) = engine
            .answer(first, room_id, 0, correct_for(0))
            .expect("answer");
        assert!(outcome.correct);
        assert_eq!(outcome.speed_bonus, 25);
        assert_eq!(outcome.points, 125);
        assert!(outcome.speed_bonus <= Settings::default().speed_bonus_max);
    }

    #[test]
    fn duplicate_answer_is_idempotent_even_after_advance() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let (first_outcome, _) = engine
            .answer(first, room_id, 0, correct_for(0))
            .expect("first answer");
        assert_eq!(first_outcome.total_score, 150);
        let (_second_outcome, _) = engine
            .answer(second, room_id, 0, correct_for(0))
            .expect("second answer");
        let (duplicate, events) = engine
            .answer(first, room_id, 0, correct_for(0))
            .expect("duplicate");
        assert!(duplicate.duplicate);
        assert!(!duplicate.applied);
        assert_eq!(duplicate.total_score, first_outcome.total_score);
        assert!(events.is_empty());
        let view = engine.view(room_id).expect("room view");
        assert_eq!(view.current_index, 1);
    }

    #[test]
    fn question_deadline_advances_through_tick() {
        let (clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        clock.advance(Duration::seconds(11));
        let events = engine.tick();
        assert!(events.iter().any(
            |event| matches!(event, EngineEvent::Question { question, .. } if question.index == 1)
        ));
        let view = engine.view(room_id).expect("room view");
        assert_eq!(view.current_index, 1);
        assert_eq!(view.phase, RoomPhase::Playing);
    }

    #[test]
    fn all_questions_answered_settles_room() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let mut finished = None;
        for index in 0..Settings::default().questions_per_room as usize {
            for player in [first, second] {
                let (_outcome, events) = engine
                    .answer(player, room_id, index, correct_for(index))
                    .expect("answer");
                for event in events {
                    if let EngineEvent::Finished { result, .. } = event {
                        finished = Some(result);
                    }
                }
            }
        }
        let result = finished.expect("settlement");
        assert_eq!(result.room_id, room_id);
        assert_eq!(result.players.len(), 2);
        assert_eq!(result.winner, None);
        let view = engine.view(room_id).expect("room view");
        assert_eq!(view.phase, RoomPhase::Finished);
    }

    #[test]
    fn queue_timeout_disposes_the_waiting_room() {
        let (clock, mut engine) = engine();
        let player = Uuid::new_v4();
        let (outcome, _) = engine.enqueue(player).expect("enqueue");
        let room_id = match outcome {
            EnqueueOutcome::Waiting { room_id } => room_id,
            EnqueueOutcome::Matched { .. } => panic!("unexpected match"),
        };
        clock.advance(Duration::seconds(16));
        let events = engine.tick();
        assert!(events.iter().any(|event| matches!(
            event,
            EngineEvent::QueueTimeout { player_id, .. } if *player_id == player
        )));
        assert!(engine.view(room_id).is_none());
        let (outcome, _) = engine.enqueue(player).expect("requeue");
        assert!(matches!(outcome, EnqueueOutcome::Waiting { .. }));
    }

    #[test]
    fn disconnecting_both_players_settles_the_room() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        let events = engine.leave(first).expect("leave");
        assert!(events.is_empty());
        assert_eq!(
            engine.view(room_id).map(|room| room.phase),
            Some(RoomPhase::Playing)
        );
        let events = engine.leave(second).expect("leave");
        assert!(matches!(events[0], EngineEvent::Finished { .. }));
        let view = engine.view(room_id).expect("room view");
        assert_eq!(view.phase, RoomPhase::Finished);
    }

    #[test]
    fn join_restores_connection_state() {
        let (_clock, mut engine) = engine();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let room_id = matched_room(&mut engine, first, second);
        engine.leave(first).expect("leave");
        let (view, question) = engine.join(first, room_id).expect("rejoin");
        assert_eq!(view.phase, RoomPhase::Playing);
        assert!(view
            .players
            .iter()
            .find(|player| player.player_id == first)
            .map(|player| player.connected)
            .unwrap_or(false));
        assert!(question.is_some());
    }
}
