use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Frontmatter metadata parsed from a markdown document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub other: HashMap<String, toml::Value>,
}

/// A markdown document model with frontmatter, content, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub path: PathBuf,
    pub content: String,
    pub frontmatter: Frontmatter,
    pub metadata: DocumentMetadata,
}

/// Metadata tracked for a document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentMetadata {
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub word_count: usize,
}

impl Document {
    pub fn new(path: PathBuf, content: String) -> Self {
        let (frontmatter, body) = parse_frontmatter(&content);
        let word_count = body.split_whitespace().count();
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            path,
            content: body.clone(),
            frontmatter,
            metadata: DocumentMetadata {
                created_at: Some(now.clone()),
                modified_at: Some(now),
                word_count,
            },
        }
    }

    pub fn title(&self) -> String {
        self.frontmatter
            .title
            .clone()
            .unwrap_or_else(|| {
                self.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string()
            })
    }

    pub fn set_content(&mut self, content: String) {
        let (frontmatter, body) = parse_frontmatter(&content);
        let word_count = body.split_whitespace().count();
        self.frontmatter = frontmatter;
        self.content = body.clone();
        self.metadata.modified_at = Some(chrono::Utc::now().to_rfc3339());
        self.metadata.word_count = word_count;
    }

    pub fn full_content(&self) -> String {
        let mut out = String::new();
        let has_frontmatter = self.frontmatter.title.is_some()
            || !self.frontmatter.tags.is_empty()
            || !self.frontmatter.other.is_empty();
        if has_frontmatter {
            out.push_str("---\n");
            out.push_str(&toml::to_string(&self.frontmatter).unwrap_or_default());
            out.push_str("---\n");
        }
        out.push_str(&self.content);
        out
    }
}

/// Split frontmatter and body from raw markdown content.
fn parse_frontmatter(content: &str) -> (Frontmatter, String) {
    let stripped = content.trim_start();
    if let Some(rest) = stripped.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let fm_str = &rest[..end_idx];
            let body = &rest[end_idx + 5..];
            let fm: Frontmatter = toml_edit::de::from_str(fm_str).unwrap_or_default();
            return (fm, body.to_string());
        }
    }
    (Frontmatter::default(), content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_works() {
        let content = "---\ntitle = \"Hello\"\ntags = [\"a\", \"b\"]\n---\n\n# Body";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.title, Some("Hello".into()));
        assert_eq!(fm.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(body, "\n# Body");
    }

    #[test]
    fn document_title_fallback() {
        let doc = Document::new(PathBuf::from("/vault/note.md"), "# Hello".into());
        assert_eq!(doc.title(), "note");
    }

    #[test]
    fn document_title_frontmatter() {
        let doc = Document::new(
            PathBuf::from("/vault/x.md"),
            "---\ntitle = \"Custom\"\n---\n\n# Body".into(),
        );
        assert_eq!(doc.title(), "Custom");
    }
}
