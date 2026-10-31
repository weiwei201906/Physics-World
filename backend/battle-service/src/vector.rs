//! Per-player error vectors used to pair opponents.

use async_trait::async_trait;
use common::error::{AppError, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Sparse-free vector over `chapter_id:difficulty` dimensions. Weights are the
/// normalized wrong-answer rate produced by `app.refresh_user_vector`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerVector {
    dims: Vec<String>,
    weights: Vec<f64>,
}

impl PlayerVector {
    pub fn new(dims: Vec<String>, weights: Vec<f64>) -> Option<Self> {
        if dims.len() != weights.len() {
            return None;
        }
        Some(Self { dims, weights })
    }

    pub fn get(&self, dim: &str) -> f64 {
        self.dims
            .iter()
            .position(|candidate| candidate == dim)
            .map(|index| self.weights[index])
            .unwrap_or(0.0)
    }

    /// Cosine similarity; returns 0 when either vector is all zero.
    pub fn cosine(&self, other: &PlayerVector) -> f64 {
        let dot: f64 = self
            .dims
            .iter()
            .map(|dim| self.get(dim) * other.get(dim))
            .sum();
        let left = self.l2_norm();
        let right = other.l2_norm();
        if left == 0.0 || right == 0.0 {
            0.0
        } else {
            dot / (left * right)
        }
    }

    fn l2_norm(&self) -> f64 {
        self.weights
            .iter()
            .map(|weight| weight * weight)
            .sum::<f64>()
            .sqrt()
    }
}

/// Matchmaking inputs loaded for one player.
#[derive(Debug, Clone, Default)]
pub struct PlayerProfile {
    pub vector: PlayerVector,
    pub nickname: Option<String>,
}

/// Looks up the vector and display name for a player. Implementations degrade
/// to the zero vector and no nickname instead of failing, so matchmaking and
/// room rendering keep working.
#[async_trait]
pub trait VectorSource: Send + Sync {
    async fn profile(&self, user: Uuid) -> PlayerProfile;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ZeroVectorSource;

#[async_trait]
impl VectorSource for ZeroVectorSource {
    async fn profile(&self, _user: Uuid) -> PlayerProfile {
        PlayerProfile::default()
    }
}

pub struct PgVectorSource {
    pool: PgPool,
}

impl PgVectorSource {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|error| AppError::internal(format!("connect vector store: {error}")))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl VectorSource for PgVectorSource {
    async fn profile(&self, user: Uuid) -> PlayerProfile {
        let vector = match sqlx::query_as::<_, (Vec<String>, Vec<f64>)>(
            "SELECT dims, weights FROM app.user_vectors WHERE user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(Some((dims, weights))) => PlayerVector::new(dims, weights).unwrap_or_default(),
            Ok(None) => PlayerVector::default(),
            Err(error) => {
                tracing::warn!(%error, user_id = %user, "load user vector failed");
                PlayerVector::default()
            }
        };
        let nickname = sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(NULLIF(p.nickname, ''), u.nickname) \
             FROM auth.users u \
             LEFT JOIN app.user_profiles p ON p.user_id = u.id \
             WHERE u.id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            tracing::warn!(%error, user_id = %user, "load nickname failed");
            error
        })
        .ok()
        .flatten();
        PlayerProfile { vector, nickname }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(weights: &[(&str, f64)]) -> PlayerVector {
        PlayerVector::new(
            weights.iter().map(|(dim, _)| (*dim).to_owned()).collect(),
            weights.iter().map(|(_, weight)| *weight).collect(),
        )
        .expect("matching lengths")
    }

    #[test]
    fn cosine_prefers_shared_error_dimensions() {
        let incoming = vector(&[("mechanics:1", 1.0), ("optics:2", 0.0)]);
        let close = vector(&[("mechanics:1", 1.0), ("optics:2", 0.0)]);
        let distant = vector(&[("mechanics:1", 0.0), ("optics:2", 1.0)]);
        assert!((close.cosine(&incoming) - 1.0).abs() < 1e-9);
        assert_eq!(distant.cosine(&incoming), 0.0);
        assert_eq!(PlayerVector::default().cosine(&incoming), 0.0);
    }

    #[test]
    fn missing_dimensions_count_as_zero() {
        let left = vector(&[("mechanics:1", 1.0)]);
        let right = vector(&[("optics:2", 1.0)]);
        assert_eq!(left.cosine(&right), 0.0);
        assert_eq!(left.get("optics:2"), 0.0);
    }
}
