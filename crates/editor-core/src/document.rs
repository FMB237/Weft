use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Frontmatter metadata parsed from a markdown document.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Frontmatter {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    /// Any additional keys not covered by the typed fields above.
    #[serde(default, flatten)]
    pub other: HashMap<String, toml::Value>,
}

impl Frontmatter {
    /// Returns `true` when no frontmatter fields are populated.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.tags.is_empty()
            && self.created.is_none()
            && self.modified.is_none()
            && self.other.is_empty()
    }

    /// Serialize the populated fields into a TOML table, omitting empties.
    ///
    /// Used by [`Document::full_content`]; the GUI saves the raw editor buffer
    /// verbatim so this only affects non-GUI callers and round-trip tests.
    pub fn to_toml_table(&self) -> toml::value::Table {
        let mut table = toml::value::Table::new();
        if let Some(title) = &self.title {
            table.insert("title".into(), toml::Value::String(title.clone()));
        }
        if !self.tags.is_empty() {
            table.insert(
                "tags".into(),
                toml::Value::Array(self.tags.iter().cloned().map(toml::Value::String).collect()),
            );
        }
        if let Some(created) = &self.created {
            table.insert("created".into(), toml::Value::String(created.clone()));
        }
        if let Some(modified) = &self.modified {
            table.insert("modified".into(), toml::Value::String(modified.clone()));
        }
        for (key, value) in &self.other {
            table.entry(key.clone()).or_insert_with(|| value.clone());
        }
        table
    }
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
    pub fn new(path: PathBuf, content: &str) -> Self {
        let (frontmatter, body) = parse_document(content);
        let word_count = body.split_whitespace().count();
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            path,
            content: body,
            frontmatter,
            metadata: DocumentMetadata {
                created_at: Some(now.clone()),
                modified_at: Some(now),
                word_count,
            },
        }
    }

    pub fn title(&self) -> String {
        self.frontmatter.title.clone().unwrap_or_else(|| {
            self.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string()
        })
    }

    pub fn set_content(&mut self, content: &str) {
        let (frontmatter, body) = parse_document(content);
        let word_count = body.split_whitespace().count();
        self.frontmatter = frontmatter;
        self.content = body;
        self.metadata.modified_at = Some(chrono::Utc::now().to_rfc3339());
        self.metadata.word_count = word_count;
    }

    /// Serialize the document back to markdown, including frontmatter.
    ///
    /// The frontmatter is written as TOML regardless of the format it was
    /// originally read in. Callers that need byte-preserving round-trips (the
    /// GUI) save their raw buffer instead.
    pub fn full_content(&self) -> String {
        let table = self.frontmatter.to_toml_table();
        if table.is_empty() {
            return self.content.clone();
        }

        let Ok(frontmatter) = toml::to_string(&table) else {
            return self.content.clone();
        };

        let mut out = String::with_capacity(frontmatter.len() + self.content.len() + 8);
        out.push_str("---\n");
        out.push_str(&frontmatter);
        out.push_str("---\n");
        out.push_str(&self.content);
        out
    }
}

/// Split raw markdown into parsed frontmatter and body.
fn parse_document(content: &str) -> (Frontmatter, String) {
    let (block, body) = crate::parser::split_frontmatter(content);
    let frontmatter = block
        .map(crate::parser::parse_frontmatter_block)
        .unwrap_or_default();
    (frontmatter, body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_works() {
        let (fm, body) =
            parse_document("---\ntitle = \"Hello\"\ntags = [\"a\", \"b\"]\n---\n\n# Body");
        assert_eq!(fm.title, Some("Hello".into()));
        assert_eq!(fm.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(body, "\n# Body");
    }

    #[test]
    fn parse_yaml_frontmatter_works() {
        let (fm, body) = parse_document(
            "---\ntitle: YAML Note\ntags:\n  - rust\n  - notes\ncustom: 42\n---\n# Body",
        );
        assert_eq!(fm.title.as_deref(), Some("YAML Note"));
        assert_eq!(fm.tags, vec!["rust", "notes"]);
        assert_eq!(fm.other.get("custom"), Some(&toml::Value::Integer(42)));
        assert_eq!(body, "# Body");
    }

    #[test]
    fn parse_yaml_inline_tags_and_unknown_keys() {
        let (fm, _) =
            parse_document("---\ntitle: Inline\ntags: [alpha, beta]\nstatus: draft\n---\n");
        assert_eq!(fm.title.as_deref(), Some("Inline"));
        assert_eq!(fm.tags, vec!["alpha", "beta"]);
        assert_eq!(
            fm.other.get("status"),
            Some(&toml::Value::String("draft".into()))
        );
    }

    #[test]
    fn malformed_frontmatter_is_ignored() {
        let (fm, body) = parse_document("---\ntitle: [unclosed\n---\nBody");
        assert!(fm.is_empty());
        assert_eq!(body, "Body");
    }

    #[test]
    fn malformed_frontmatter_without_closing_delimiter_is_body() {
        let content = "---\ntitle: Draft\nBody";
        let (fm, body) = parse_document(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn document_title_fallback() {
        let doc = Document::new(PathBuf::from("/vault/note.md"), "# Hello");
        assert_eq!(doc.title(), "note");
    }

    #[test]
    fn document_title_frontmatter() {
        let doc = Document::new(
            PathBuf::from("/vault/x.md"),
            "---\ntitle = \"Custom\"\n---\n\n# Body",
        );
        assert_eq!(doc.title(), "Custom");
    }

    #[test]
    fn full_content_round_trips_populated_fields() {
        let doc = Document::new(
            PathBuf::from("/vault/x.md"),
            "---\ntitle = \"Custom\"\ntags = [\"a\"]\n---\n\n# Body",
        );
        let full = doc.full_content();
        assert!(full.starts_with("---\n"));
        assert!(full.contains("title = \"Custom\""));
        assert!(full.contains("tags = [\"a\"]"));
        assert!(!full.contains("[other]"));
        assert!(full.ends_with("\n# Body"));

        let round_tripped = Document::new(PathBuf::from("/vault/x.md"), &full);
        assert_eq!(round_tripped.frontmatter, doc.frontmatter);
        assert_eq!(round_tripped.content, doc.content);
    }

    #[test]
    fn full_content_without_frontmatter_is_body() {
        let doc = Document::new(PathBuf::from("/vault/x.md"), "# Just body");
        assert_eq!(doc.full_content(), "# Just body");
    }
}
