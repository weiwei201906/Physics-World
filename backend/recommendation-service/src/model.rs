//! Pure domain types: chapter vectors, scoring and recommendation ranking.

use std::collections::HashSet;

/// Sparse-free vector over chapter dimensions. Each chapter owns exactly one
/// weight in the profile, so the vector length equals the dimension count.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterVector {
    dims: Vec<String>,
    weights: Vec<f64>,
}

impl ChapterVector {
    pub fn zero(dims: Vec<String>) -> Self {
        let weights = vec![0.0; dims.len()];
        Self { dims, weights }
    }

    pub fn from_weights(dims: Vec<String>, weights: Vec<f64>) -> Option<Self> {
        if dims.len() != weights.len() {
            return None;
        }
        Some(Self { dims, weights })
    }

    pub fn one_hot(dims: &[String], index: usize) -> Self {
        let mut vector = Self::zero(dims.to_vec());
        if index < vector.weights.len() {
            vector.weights[index] = 1.0;
        }
        vector
    }

    pub fn dims(&self) -> &[String] {
        &self.dims
    }

    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    pub fn get(&self, dim: &str) -> f64 {
        self.index_of(dim)
            .map(|index| self.weights[index])
            .unwrap_or(0.0)
    }

    pub fn add(&mut self, dim: &str, delta: f64) -> bool {
        match self.index_of(dim) {
            Some(index) => {
                self.weights[index] += delta;
                true
            }
            None => false,
        }
    }

    /// Read-time merge of the PG baseline and the Redis increments. Dimensions
    /// present in only one side are kept, which keeps the merge tolerant of a
    /// chapter being added after the stored vector was written.
    pub fn merge(&self, increments: &ChapterVector) -> Self {
        let mut dims = self.dims.clone();
        let mut seen: HashSet<&str> = self.dims.iter().map(String::as_str).collect();
        for dim in &increments.dims {
            if seen.insert(dim.as_str()) {
                dims.push(dim.clone());
            }
        }
        let weights = dims
            .iter()
            .map(|dim| self.get(dim) + increments.get(dim))
            .collect();
        Self { dims, weights }
    }

    pub fn l2_norm(&self) -> f64 {
        self.weights
            .iter()
            .map(|weight| weight * weight)
            .sum::<f64>()
            .sqrt()
    }

    /// In-place L2 normalization. An all-zero vector stays all-zero.
    pub fn l2_normalize(&mut self) {
        let norm = self.l2_norm();
        if norm > 0.0 {
            for weight in &mut self.weights {
                *weight /= norm;
            }
        }
    }

    pub fn normalized(&self) -> Self {
        let mut vector = self.clone();
        vector.l2_normalize();
        vector
    }

    /// Dot product over the shared dimensions. Missing dimensions count as 0.
    pub fn dot(&self, other: &ChapterVector) -> f64 {
        self.dims
            .iter()
            .map(|dim| self.get(dim) * other.get(dim))
            .sum()
    }

    /// Cosine similarity; returns 0 when either vector has zero norm.
    pub fn cosine(&self, other: &ChapterVector) -> f64 {
        let denominator = self.l2_norm() * other.l2_norm();
        if denominator == 0.0 {
            0.0
        } else {
            self.dot(other) / denominator
        }
    }

    fn index_of(&self, dim: &str) -> Option<usize> {
        self.dims.iter().position(|candidate| candidate == dim)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub id: String,
    pub name: String,
}

impl Chapter {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    /// Build the catalog implied by the configured chapter dimensions.
    pub fn catalog(dims: &[String]) -> Vec<Self> {
        dims.iter()
            .map(|dim| Self::new(dim.clone(), display_name(dim)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub chapter_id: String,
    pub chapter_name: String,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttemptStats {
    pub total_attempts: u64,
    pub correct_attempts: u64,
    pub chapter_attempts: std::collections::HashMap<String, u64>,
}

impl AttemptStats {
    pub fn correct_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            0.0
        } else {
            self.correct_attempts as f64 / self.total_attempts as f64
        }
    }
}

pub fn reason_for_score(score: f64) -> String {
    if score >= 0.5 {
        "High error weight in this chapter".to_string()
    } else if score >= 0.25 {
        "Moderate error weight in this chapter".to_string()
    } else {
        "Light review recommended".to_string()
    }
}

fn display_name(dim: &str) -> String {
    let mut chars = dim.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => dim.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> Vec<String> {
        vec![
            "mechanics".to_string(),
            "optics".to_string(),
            "modern".to_string(),
        ]
    }

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "{left} != {right}");
    }

    #[test]
    fn merge_adds_increments_and_keeps_union_of_dimensions() {
        let baseline = ChapterVector::from_weights(dims(), vec![1.0, 0.5, 0.0]).unwrap();
        let increments = ChapterVector::from_weights(dims(), vec![0.5, -0.2, 1.0]).unwrap();
        let merged = baseline.merge(&increments);
        assert_eq!(merged.dims(), dims());
        close(merged.get("mechanics"), 1.5);
        close(merged.get("optics"), 0.3);
        close(merged.get("modern"), 1.0);
    }

    #[test]
    fn merge_keeps_unknown_increment_dimensions() {
        let baseline =
            ChapterVector::from_weights(vec!["mechanics".to_string()], vec![1.0]).unwrap();
        let increments = ChapterVector::from_weights(
            vec!["mechanics".to_string(), "optics".to_string()],
            vec![0.5, 2.0],
        )
        .unwrap();
        let merged = baseline.merge(&increments);
        assert_eq!(merged.dims().len(), 2);
        close(merged.get("optics"), 2.0);
    }

    #[test]
    fn l2_normalization_produces_unit_length_and_handles_zero() {
        let mut vector = ChapterVector::from_weights(dims(), vec![3.0, 4.0, 0.0]).unwrap();
        vector.l2_normalize();
        close(vector.l2_norm(), 1.0);
        close(vector.get("mechanics"), 0.6);
        close(vector.get("optics"), 0.8);

        let zero = ChapterVector::zero(dims());
        assert_eq!(zero.normalized().l2_norm(), 0.0);
    }

    #[test]
    fn dot_and_cosine_score_vectors() {
        let left = ChapterVector::from_weights(dims(), vec![1.0, 2.0, 0.0]).unwrap();
        let right = ChapterVector::from_weights(dims(), vec![2.0, 1.0, 0.0]).unwrap();
        close(left.dot(&right), 4.0);
        close(left.cosine(&right), 4.0 / (5.0_f64.sqrt() * 5.0_f64.sqrt()));
        close(left.cosine(&ChapterVector::zero(dims())), 0.0);
    }

    #[test]
    fn one_hot_vector_matches_its_dimension() {
        let vector = ChapterVector::one_hot(&dims(), 1);
        close(vector.get("optics"), 1.0);
        close(vector.get("mechanics"), 0.0);
        close(vector.l2_norm(), 1.0);
    }

    #[test]
    fn catalog_derives_display_names() {
        let chapters = Chapter::catalog(&dims());
        assert_eq!(chapters[0], Chapter::new("mechanics", "Mechanics"));
        assert_eq!(chapters[1], Chapter::new("optics", "Optics"));
    }
}
