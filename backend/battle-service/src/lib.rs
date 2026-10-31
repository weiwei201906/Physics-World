//! Battle service: WebSocket matchmaking, room state machine and settlement.

pub mod engine;
pub mod protocol;
pub mod settings;
pub mod store;
pub mod validator;
pub mod vector;
pub mod ws;

pub use ws::{router, AppState};
