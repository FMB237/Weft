use pulldown_cmark::{Options, Parser, html};
use std::path::Path;
use toml_edit;

/// A wikilink `[[target|alias]]` extracted from markdown source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wikilink {
    /// The link target (page/file referenced).
    pub target: String,
    /// Optional display alias (the part after `|`).
    pub alias: Option<String>,
}

/// A parsed markdown document with wikilinks and frontmatter.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub path: std::path::PathBuf,
    pub html: String,
    pub wikilinks: Vec<Wikilink>,
    pub frontmatter: crate::document::Frontmatter,
}

/// Extract wikilinks (`[[link]]` and `[[link|alias]]`) from markdown source.
///
/// Links inside fenced code blocks (```` ``` ```` or `~~~`) are ignored so
/// documentation examples do not turn into graph edges.
pub fn extract_wikilinks(content: &str) -> Vec<Wikilink> {
    let mut links = Vec::new();
    let mut fence: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };

        if let Some(marker) = marker {
            match fence.as_deref() {
                None => {
                    fence = Some(marker.to_string());
                    continue;
                }
                Some(open) if open == marker => {
                    fence = None;
                    continue;
                }
                Some(_) => {}
            }
        }

        if fence.is_some() {
            continue;
        }

        let mut rest = line;
        while let Some(start) = rest.find("[[") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("]]") else {
                break;
            };
            let inner = &after[..end];
            let (target, alias) = if let Some(pipe) = inner.find('|') {
                (inner[..pipe].trim(), Some(inner[pipe + 1..].trim()))
            } else {
                (inner.trim(), None)
            };
            if !target.is_empty() {
                links.push(Wikilink {
                    target: target.to_string(),
                    alias: alias.map(ToString::to_string),
                });
            }
            rest = &after[end + 2..];
        }
    }

    links
}

/// Parse markdown into HTML and extract wikilinks.
pub fn parse_markdown(path: &Path, content: &str) -> ParsedDocument {
    let wikilinks = extract_wikilinks(content);

    // Also produce proper HTML via pulldown-cmark for rendering.
    let mut pulldown_opts = Options::empty();
    pulldown_opts.insert(Options::ENABLE_TABLES);
    pulldown_opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(content, pulldown_opts);
    let mut html = String::new();
    html::push_html(&mut html, parser);

    ParsedDocument {
        path: path.to_path_buf(),
        html,
        wikilinks,
        frontmatter: parse_frontmatter(content),
    }
}

/// Split an optional TOML frontmatter block from the markdown body.
///
/// Returns `(frontmatter, body)`. When no frontmatter is present the first
/// element is `None` and `body` is the original input unchanged.
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let leading_ws = content.len() - content.trim_start().len();
    let stripped = &content[leading_ws..];
    if let Some(rest) = stripped.strip_prefix("---\n")
        && let Some(end_idx) = rest.find("\n---\n")
    {
        return (Some(&rest[..end_idx]), &rest[end_idx + 5..]);
    }
    (None, content)
}

pub fn parse_frontmatter(content: &str) -> crate::document::Frontmatter {
    let (frontmatter, _) = split_frontmatter(content);
    frontmatter
        .and_then(|fm| toml_edit::de::from_str(fm).ok())
        .unwrap_or_default()
}

/// Render markdown to HTML (standalone helper).
pub fn markdown_to_html(content: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(content, opts);
    let mut html = String::new();
    html::push_html(&mut html, parser);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_wikilinks_works() {
        let content = "See [[Home]] and [[Other Page]] here.";
        let links = extract_wikilinks(content);
        assert_eq!(
            links,
            vec![
                Wikilink {
                    target: "Home".into(),
                    alias: None
                },
                Wikilink {
                    target: "Other Page".into(),
                    alias: None
                },
            ]
        );
    }

    #[test]
    fn extract_wikilinks_with_alias() {
        let content = "See [[Home|Home Page]] here.";
        let links = extract_wikilinks(content);
        assert_eq!(
            links,
            vec![Wikilink {
                target: "Home".into(),
                alias: Some("Home Page".into())
            }]
        );
    }

    #[test]
    fn parse_markdown_produces_html() {
        let parsed = parse_markdown(Path::new("/test.md"), "# Hello [[World]]");
        assert!(parsed.html.contains("Hello"));
        assert_eq!(
            parsed.wikilinks,
            vec![Wikilink {
                target: "World".into(),
                alias: None
            }]
        );
    }

    #[test]
    fn extract_wikilinks_skips_fenced_code_blocks() {
        let content = "# A\n\n```rust\n[[not-a-link]]\n```\n\n[[real-link]]\n";
        let links = extract_wikilinks(content);
        assert_eq!(
            links,
            vec![Wikilink {
                target: "real-link".into(),
                alias: None
            }]
        );
    }

    #[test]
    fn extract_wikilinks_handles_tilde_fences_and_multiple_per_line() {
        let content = "~~~\n[[hidden]]\n~~~\n[[one]] and [[two|Two]]\n";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "one");
        assert_eq!(links[1].target, "two");
        assert_eq!(links[1].alias.as_deref(), Some("Two"));
    }

    #[test]
    fn extract_wikilinks_ignores_empty_targets() {
        let links = extract_wikilinks("[[ ]] and [[]] and [[ok]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "ok");
    }

    #[test]
    fn split_frontmatter_extracts_body() {
        let content = "---\ntitle = \"Hello\"\n---\n\n# Body";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, Some("title = \"Hello\""));
        assert_eq!(body, "\n# Body");
    }

    #[test]
    fn split_frontmatter_without_block_returns_input() {
        let content = "# Just a note";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_tolerates_leading_blank_lines() {
        let content = "\n\n---\ntitle = \"Hello\"\n---\nBody";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, Some("title = \"Hello\""));
        assert_eq!(body, "Body");
    }
}
