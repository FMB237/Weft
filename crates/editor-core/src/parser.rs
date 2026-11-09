use comrak::ComrakOptions;
use pulldown_cmark::{html, Parser, Options};
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
pub fn extract_wikilinks(content: &str) -> Vec<Wikilink> {
    let mut links = Vec::new();
    let mut chars = content.char_indices().peekable();
    while let Some((i, _)) = chars.next() {
        if content.get(i..i + 2) == Some("[[") {
            let end = content[i + 2..].find("]]");
            if let Some(end) = end {
                let link = &content[i + 2..i + 2 + end];
                let (target, alias) = if let Some(pipe) = link.find('|') {
                    (link[..pipe].to_string(), Some(link[pipe + 1..].to_string()))
                } else {
                    (link.to_string(), None)
                };
                links.push(Wikilink { target, alias });
            }
        }
    }
    links
}

/// Parse markdown into HTML and extract wikilinks.
pub fn parse_markdown(path: &Path, content: &str) -> ParsedDocument {
    let wikilinks = extract_wikilinks(content);

    let mut comrak_opts = ComrakOptions::default();
    comrak_opts.extension.strikethrough = true;
    comrak_opts.extension.superscript = true;
    comrak_opts.extension.autolink = true;
    comrak_opts.extension.table = true;

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

pub fn parse_frontmatter(content: &str) -> crate::document::Frontmatter {
    let stripped = content.trim_start();
    if let Some(rest) = stripped.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let fm_str = &rest[..end_idx];
            if let Ok(fm) = toml_edit::de::from_str(fm_str) {
                return fm;
            }
        }
    }
    crate::document::Frontmatter::default()
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
                Wikilink { target: "Home".into(), alias: None },
                Wikilink { target: "Other Page".into(), alias: None },
            ]
        );
    }

    #[test]
    fn extract_wikilinks_with_alias() {
        let content = "See [[Home|Home Page]] here.";
        let links = extract_wikilinks(content);
        assert_eq!(
            links,
            vec![Wikilink { target: "Home".into(), alias: Some("Home Page".into()) }]
        );
    }

    #[test]
    fn parse_markdown_produces_html() {
        let parsed = parse_markdown(Path::new("/test.md"), "# Hello [[World]]");
        assert!(parsed.html.contains("Hello"));
        assert_eq!(
            parsed.wikilinks,
            vec![Wikilink { target: "World".into(), alias: None }]
        );
    }
}
