//! Battle persistence boundary and the in-memory implementation used by tests
//! and by the local service. Every room owns a tokio broadcast channel so the
//! WebSocket layer can fan out state changes without holding the engine lock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use common::error::{AppError, Result};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::engine::{
    AnswerOutcome, BattleEngine, BattleResult, EngineEvent, EnqueueOutcome, RoomView,
};
use crate::protocol::ServerMessage;

const ROOM_CHANNEL_CAPACITY: usize = 256;

pub enum EnqueueResult {
    Waiting {
        room_id: Uuid,
        receiver: broadcast::Receiver<ServerMessage>,
    },
    Matched {
        room_id: Uuid,
        room: RoomView,
        receiver: broadcast::Receiver<ServerMessage>,
    },
}

#[async_trait]
pub trait BattleStore: Send + Sync {
    async fn enqueue(&self, player: Uuid) -> Result<EnqueueResult>;
    async fn answer(
        &self,
        player: Uuid,
        room_id: Uuid,
        question_index: usize,
        option: String,
    ) -> Result<AnswerOutcome>;
    async fn leave(&self, player: Uuid) -> Result<()>;
    async fn tick(&self) -> Result<()>;
    async fn subscribe(&self, room_id: Uuid) -> Result<broadcast::Receiver<ServerMessage>>;
    async fn room(&self, room_id: Uuid) -> Result<Option<RoomView>>;
}

/// Settlement side effect boundary.
///
/// The in-memory implementation records results locally. A Redis-backed
/// implementation would ZADD `battle:finished`, acquire the reconcile lock and
/// diff the ZSET against PostgreSQL; see the TODO below.
#[async_trait]
pub trait Reconciler: Send + Sync {
    async fn record_finished(&self, result: &BattleResult) -> Result<()>;
    async fn reconcile(&self) -> Result<usize>;
}

#[derive(Debug, Default)]
pub struct InMemoryReconciler {
    pending: Mutex<Vec<BattleResult>>,
}

impl InMemoryReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> usize {
        self.lock().len()
    }

    pub fn finished(&self) -> Vec<BattleResult> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<BattleResult>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl Reconciler for InMemoryReconciler {
    async fn record_finished(&self, result: &BattleResult) -> Result<()> {
        self.lock().push(result.clone());
        Ok(())
    }

    async fn reconcile(&self) -> Result<usize> {
        let mut pending = self.lock();
        let count = pending.len();
        pending.clear();
        Ok(count)
    }
}

// TODO: implement Reconciler for Redis: SET battle:reconcile:lock {pod}
// NX EX battle.reconcile.lock_ttl_sec, ZRANGEBYSCORE battle:finished by time
// window with LIMIT battle.reconcile.batch_size, batch query PostgreSQL for
// missing battle ids, republish, then ZREM on success. Stale entries older than
// battle.reconcile.stale_threshold_sec only raise an alert.

pub struct InMemoryBattleStore {
    engine: Mutex<BattleEngine>,
    channels: Mutex<HashMap<Uuid, broadcast::Sender<ServerMessage>>>,
    reconciler: Arc<dyn Reconciler>,
}

impl InMemoryBattleStore {
    pub fn new(engine: BattleEngine, reconciler: Arc<dyn Reconciler>) -> Self {
        Self {
            engine: Mutex::new(engine),
            channels: Mutex::new(HashMap::new()),
            reconciler,
        }
    }

    fn engine(&self) -> MutexGuard<'_, BattleEngine> {
        self.engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn channels(&self) -> MutexGuard<'_, HashMap<Uuid, broadcast::Sender<ServerMessage>>> {
        self.channels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn channel_for(&self, room_id: Uuid) -> broadcast::Sender<ServerMessage> {
        let mut channels = self.channels();
        channels
            .entry(room_id)
            .or_insert_with(|| broadcast::channel(ROOM_CHANNEL_CAPACITY).0)
            .clone()
    }

    fn send(&self, room_id: Uuid, message: ServerMessage) {
        let sender = self.channels().get(&room_id).cloned();
        if let Some(sender) = sender {
            let _ = sender.send(message);
        }
    }

    async fn dispatch(&self, events: Vec<EngineEvent>) -> Result<()> {
        let (messages, expired, finished) = convert_events(events);
        for (room_id, message) in messages {
            self.send(room_id, message);
        }
        for room_id in expired {
            self.channels().remove(&room_id);
        }
        for result in finished {
            self.reconciler.record_finished(&result).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl BattleStore for InMemoryBattleStore {
    async fn enqueue(&self, player: Uuid) -> Result<EnqueueResult> {
        let (outcome, events) = {
            let mut engine = self.engine();
            engine.enqueue(player)?
        };
        let room_id = match &outcome {
            EnqueueOutcome::Waiting { room_id } => *room_id,
            EnqueueOutcome::Matched { room_id, .. } => *room_id,
        };
        let receiver = self.channel_for(room_id).subscribe();
        self.dispatch(events).await?;
        Ok(match outcome {
            EnqueueOutcome::Waiting { room_id } => EnqueueResult::Waiting { room_id, receiver },
            EnqueueOutcome::Matched { room_id, room } => EnqueueResult::Matched {
                room_id,
                room,
                receiver,
            },
        })
    }

    async fn answer(
        &self,
        player: Uuid,
        room_id: Uuid,
        question_index: usize,
        option: String,
    ) -> Result<AnswerOutcome> {
        let (outcome, events) = {
            let mut engine = self.engine();
            engine.answer(player, room_id, question_index, option)?
        };
        self.dispatch(events).await?;
        Ok(outcome)
    }

    async fn leave(&self, player: Uuid) -> Result<()> {
        let events = {
            let mut engine = self.engine();
            engine.leave(player)?
        };
        self.dispatch(events).await
    }

    async fn tick(&self) -> Result<()> {
        let events = {
            let mut engine = self.engine();
            engine.tick()
        };
        self.dispatch(events).await
    }

    async fn subscribe(&self, room_id: Uuid) -> Result<broadcast::Receiver<ServerMessage>> {
        self.channels()
            .get(&room_id)
            .map(broadcast::Sender::subscribe)
            .ok_or_else(|| AppError::not_found("room not found"))
    }

    async fn room(&self, room_id: Uuid) -> Result<Option<RoomView>> {
        Ok(self.engine().view(room_id))
    }
}

fn convert_events(
    events: Vec<EngineEvent>,
) -> (Vec<(Uuid, ServerMessage)>, Vec<Uuid>, Vec<BattleResult>) {
    let mut messages = Vec::new();
    let mut expired = Vec::new();
    let mut finished = Vec::new();
    for event in events {
        match event {
            EngineEvent::Matched {
                room_id,
                room,
                question,
            } => messages.push((
                room_id,
                ServerMessage::Matched {
                    room_id,
                    room,
                    question,
                },
            )),
            EngineEvent::Question { room_id, question } => {
                messages.push((room_id, ServerMessage::Question { room_id, question }));
            }
            EngineEvent::Score {
                room_id,
                player_id,
                question_index,
                correct,
                points,
                speed_bonus,
                total_score,
            } => messages.push((
                room_id,
                ServerMessage::Score {
                    room_id,
                    player_id,
                    question_index,
                    correct,
                    points,
                    speed_bonus,
                    total_score,
                },
            )),
            EngineEvent::Finished { room_id, result } => {
                finished.push(result.clone());
                messages.push((room_id, ServerMessage::Finished { room_id, result }));
            }
            EngineEvent::QueueTimeout { room_id, .. } => messages.push((
                room_id,
                ServerMessage::error("queue_timeout", "matchmaking timed out"),
            )),
            EngineEvent::RoomExpired { room_id } => expired.push(room_id),
        }
    }
    (messages, expired, finished)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use common::clock::FakeClock;

    use crate::engine::{QuestionSpec, RoomPhase};
    use crate::settings::Settings;

    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 31, 8, 0, 0)
            .single()
            .unwrap_or_default()
    }

    fn catalog() -> Vec<QuestionSpec> {
        ["a", "b", "c", "d", "a"]
            .iter()
            .enumerate()
            .map(|(index, option)| QuestionSpec {
                id: format!("q-{}", index + 1),
                correct_option: (*option).to_string(),
            })
            .collect()
    }

    #[tokio::test]
    async fn broadcasts_match_questions_scores_and_settlement() {
        let clock = Arc::new(FakeClock::new(fixed_now()));
        let engine = BattleEngine::new(Settings::default(), clock, catalog());
        let reconciler = Arc::new(InMemoryReconciler::default());
        let store = InMemoryBattleStore::new(engine, reconciler.clone());
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();

        let waiting = store.enqueue(first).await.expect("first enqueue");
        let (room_id, mut first_rx) = match waiting {
            EnqueueResult::Waiting { room_id, receiver } => (room_id, receiver),
            EnqueueResult::Matched { .. } => panic!("first player should wait"),
        };

        let matched = store.enqueue(second).await.expect("second enqueue");
        let mut second_rx = match matched {
            EnqueueResult::Matched {
                room_id: matched_room,
                receiver,
                ..
            } => {
                assert_eq!(matched_room, room_id);
                receiver
            }
            EnqueueResult::Waiting { .. } => panic!("second player should match"),
        };

        let message = first_rx.recv().await.expect("first matched broadcast");
        assert!(matches!(message, ServerMessage::Matched { .. }));
        let message = second_rx.recv().await.expect("second matched broadcast");
        assert!(matches!(message, ServerMessage::Matched { .. }));

        for index in 0..Settings::default().questions_per_room as usize {
            let option = catalog()[index].correct_option.clone();
            store
                .answer(first, room_id, index, option.clone())
                .await
                .expect("first answer");
            let outcome = store
                .answer(second, room_id, index, option)
                .await
                .expect("second answer");
            if index + 1 == Settings::default().questions_per_room as usize {
                assert_eq!(outcome.phase, RoomPhase::Finished);
            }
        }

        assert_eq!(reconciler.pending(), 1);
        let reconciled = reconciler.reconcile().await.expect("reconcile");
        assert_eq!(reconciled, 1);
        let view = store.room(room_id).await.expect("room").expect("view");
        assert_eq!(view.phase, RoomPhase::Finished);
    }
}
