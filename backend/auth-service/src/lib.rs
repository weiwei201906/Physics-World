//! Auth service: registration, login, RS256 access tokens, rotating refresh
//! tokens and the public JWKS document.

pub mod password;
pub mod routes;
pub mod store;
pub mod tokens;

pub use routes::{router, AppState};
