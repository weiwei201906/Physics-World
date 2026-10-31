//! Recommendation ranking over chapter vectors.

use crate::model::{reason_for_score, Chapter, ChapterVector, Recommendation};

#[derive(Debug, Clone)]
pub struct RecommendationEngine {
    dims: Vec<String>,
    chapters: Vec<Chapter>,
    min_score: f64,
}

impl RecommendationEngine {
    pub fn new(dims: Vec<String>, chapters: Vec<Chapter>, min_score: f64) -> Self {
        Self {
            dims,
            chapters,
            min_score,
        }
    }

    pub fn from_dims(dims: Vec<String>, min_score: f64) -> Self {
        let chapters = Chapter::catalog(&dims);
        Self::new(dims, chapters, min_score)
    }

    pub fn dims(&self) -> &[String] {
        &self.dims
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    pub fn chapter(&self, chapter_id: &str) -> Option<&Chapter> {
        self.chapters
            .iter()
            .find(|chapter| chapter.id == chapter_id)
    }

    /// Rank chapters by cosine similarity to the merged profile. Chapters whose
    /// score is below `min_score` are excluded, ties are broken by name and the
    /// result is truncated to `limit`.
    pub fn rank(&self, profile: &ChapterVector, limit: usize) -> Vec<Recommendation> {
        let mut scored: Vec<Recommendation> = self
            .chapters
            .iter()
            .filter_map(|chapter| {
                let index = self.dims.iter().position(|dim| dim == &chapter.id)?;
                let chapter_vector = ChapterVector::one_hot(&self.dims, index);
                let score = profile.cosine(&chapter_vector);
                if score < self.min_score {
                    return None;
                }
                Some(Recommendation {
                    chapter_id: chapter.id.clone(),
                    chapter_name: chapter.name.clone(),
                    score,
                    reason: reason_for_score(score),
                })
            })
            .collect();
        scored.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.chapter_name.cmp(&right.chapter_name))
        });
        scored.truncate(limit);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> Vec<String> {
        vec![
            "mechanics".to_string(),
            "thermodynamics".to_string(),
            "optics".to_string(),
            "electromagnetism".to_string(),
        ]
    }

    fn engine(min_score: f64) -> RecommendationEngine {
        RecommendationEngine::from_dims(dims(), min_score)
    }

    #[test]
    fn ranks_strongest_chapters_first() {
        let profile = ChapterVector::from_weights(dims(), vec![1.0, 0.0, 0.6, 0.2])
            .unwrap()
            .normalized();
        let ranked = engine(0.0).rank(&profile, 4);
        assert_eq!(ranked.len(), 4);
        assert_eq!(ranked[0].chapter_id, "mechanics");
        assert_eq!(ranked[1].chapter_id, "optics");
        assert_eq!(ranked[2].chapter_id, "electromagnetism");
        assert_eq!(ranked[3].chapter_id, "thermodynamics");
        assert!(!ranked[0].reason.is_empty());
    }

    #[test]
    fn thresholds_exclude_weak_chapters() {
        let profile = ChapterVector::from_weights(dims(), vec![1.0, 0.0, 0.1, 0.0])
            .unwrap()
            .normalized();
        let ranked = engine(0.2).rank(&profile, 10);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].chapter_id, "mechanics");
    }

    #[test]
    fn limit_truncates_the_ranking() {
        let profile = ChapterVector::from_weights(dims(), vec![1.0, 0.9, 0.8, 0.7])
            .unwrap()
            .normalized();
        let ranked = engine(0.0).rank(&profile, 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].chapter_id, "mechanics");
        assert_eq!(ranked[1].chapter_id, "thermodynamics");
    }

    #[test]
    fn zero_profile_below_threshold_returns_empty() {
        let ranked = engine(0.05).rank(&ChapterVector::zero(dims()), 10);
        assert!(ranked.is_empty());
    }

    #[test]
    fn chapter_lookup_is_exact() {
        let engine = engine(0.0);
        assert!(engine.chapter("mechanics").is_some());
        assert!(engine.chapter("unknown").is_none());
        assert_eq!(engine.dims().len(), 4);
    }
}
