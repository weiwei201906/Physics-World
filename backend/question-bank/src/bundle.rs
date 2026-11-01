//! Declarative question bundle.
//!
//! The JSON bundle lives in RustFS and is synced into PostgreSQL on startup,
//! upserting `question.chapters` and `question.questions`. PostgreSQL is a
//! projection of the object, never an independent source.

use common::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize)]
pub struct Bundle {
    pub chapters: Vec<ChapterSeed>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChapterSeed {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub questions: Vec<QuestionSeed>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionSeed {
    pub id: String,
    pub difficulty: i32,
    pub stem: String,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub answer: Option<String>,
    pub options: Vec<OptionSeed>,
    /// Images attached to this question; the first entry becomes the primary
    /// image exposed as `image_url`.
    #[serde(default)]
    pub media: Vec<MediaSeed>,
    #[serde(default)]
    pub scope: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OptionSeed {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaSeed {
    /// File name relative to the media base, or an absolute URL.
    pub file: String,
    #[serde(default)]
    pub alt: String,
    #[serde(default = "default_variant")]
    pub variant: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default = "default_mime")]
    pub mime: String,
}

fn default_variant() -> String {
    "default".to_owned()
}

fn default_mime() -> String {
    "image/svg+xml".to_owned()
}

impl QuestionSeed {
    pub fn options_value(&self) -> Value {
        json!(self.options)
    }

    /// Media entries with absolute URLs, ready to store as JSONB.
    pub fn media_value(&self, media_base: &str) -> Value {
        Value::Array(
            self.media
                .iter()
                .map(|media| {
                    json!({
                        "url": resolve_url(media_base, &media.file),
                        "alt": media.alt,
                        "variant": media.variant,
                        "width": media.width,
                        "height": media.height,
                        "mime": media.mime,
                    })
                })
                .collect(),
        )
    }

    pub fn primary_image(&self, media_base: &str) -> Option<String> {
        self.media
            .first()
            .map(|media| resolve_url(media_base, &media.file))
    }
}

fn resolve_url(base: &str, file: &str) -> String {
    if file.starts_with("http://") || file.starts_with("https://") || file.starts_with('/') {
        return file.to_owned();
    }
    format!("{}/{}", base.trim_end_matches('/'), file)
}

pub fn parse(raw: &str) -> Result<Bundle> {
    let bundle: Bundle = serde_json::from_str(raw)
        .map_err(|error| AppError::invalid(format!("parse question bundle: {error}")))?;
    if bundle.chapters.is_empty() {
        return Err(AppError::invalid("question bundle contains no chapters"));
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_absolute_media() {
        assert_eq!(resolve_url("/media", "a.svg"), "/media/a.svg");
        assert_eq!(resolve_url("/media/", "a.svg"), "/media/a.svg");
        assert_eq!(
            resolve_url("/media", "https://cdn/a.svg"),
            "https://cdn/a.svg"
        );
        assert_eq!(resolve_url("/media", "/static/a.svg"), "/static/a.svg");
    }

    #[test]
    fn primary_image_mirrors_first_media_entry() {
        let seed = QuestionSeed {
            id: "q".to_owned(),
            difficulty: 1,
            stem: "stem".to_owned(),
            explanation: None,
            answer: Some("a".to_owned()),
            options: Vec::new(),
            media: vec![MediaSeed {
                file: "first.svg".to_owned(),
                alt: "first".to_owned(),
                variant: "default".to_owned(),
                width: 640,
                height: 400,
                mime: "image/svg+xml".to_owned(),
            }],
            scope: None,
        };
        assert_eq!(
            seed.primary_image("/media").as_deref(),
            Some("/media/first.svg")
        );
    }
}
