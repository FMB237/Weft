use std::path::{Path, PathBuf};

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, STORED, STRING, Schema, TEXT};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, doc};

/// A search result from the full-text index.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

/// Options for a search query.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub tags: Vec<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            tags: Vec::new(),
        }
    }
}

/// Characters with special meaning to tantivy's query parser.
const QUERY_SPECIAL_CHARS: &str = r#"+ - && || ! ( ) { } [ ] ^ " ~ * ? : \ /"#;

/// Number of characters of context to include on each side of a snippet hit.
const SNIPPET_CONTEXT: usize = 80;
/// Maximum snippet length when there is no query match.
const SNIPPET_MAX: usize = 180;

/// Field handles in the tantivy schema.
struct SchemaFields {
    path: Field,
    title: Field,
    body: Field,
}

/// Wrapper around the tantivy search index.
pub struct SearchIndex {
    index: Index,
    schema_fields: SchemaFields,
    reader: IndexReader,
    writer: IndexWriter,
}

impl SearchIndex {
    /// Create a new in-memory tantivy index.
    pub fn new() -> tantivy::Result<Self> {
        let mut schema_builder = Schema::builder();
        let path = schema_builder.add_text_field("path", STRING | STORED);
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let body = schema_builder.add_text_field("body", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()?;
        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            schema_fields: SchemaFields { path, title, body },
            reader,
            writer,
        })
    }

    /// Add or update a document in the index.
    pub fn add_document(&mut self, path: &Path, content: &str) -> tantivy::Result<()> {
        // Remove any existing document with the same path before re-adding.
        let term =
            tantivy::Term::from_field_text(self.schema_fields.path, path.to_str().unwrap_or(""));
        self.writer.delete_term(term);

        let frontmatter = crate::parser::parse_frontmatter(content);
        let title = frontmatter
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string()
            });

        // Index the body only: frontmatter keys should not pollute matches.
        let (_, body) = crate::parser::split_frontmatter(content);

        self.writer.add_document(doc!(
            self.schema_fields.path => path.to_str().unwrap_or(""),
            self.schema_fields.title => title,
            self.schema_fields.body => body.to_string()
        ))?;
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, path: &Path) -> tantivy::Result<()> {
        let term =
            tantivy::Term::from_field_text(self.schema_fields.path, path.to_str().unwrap_or(""));
        self.writer.delete_term(term);
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search the index using tantivy full-text search.
    pub fn search(&self, query: &str, opts: &SearchOptions) -> tantivy::Result<Vec<SearchResult>> {
        let query = sanitize_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.schema_fields.body, self.schema_fields.title],
        );
        let tantivy_query = query_parser.parse_query(&query)?;

        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(opts.limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let path_str = doc
                .get_first(self.schema_fields.path)
                .and_then(|v| v.as_text())
                .unwrap_or("");
            let title = doc
                .get_first(self.schema_fields.title)
                .and_then(|v| v.as_text())
                .unwrap_or("untitled");
            let body = doc
                .get_first(self.schema_fields.body)
                .and_then(|v| v.as_text())
                .unwrap_or("");
            let path = PathBuf::from(path_str);

            results.push(SearchResult {
                path,
                title: title.to_string(),
                snippet: make_snippet(body, &query),
                score,
            });
        }

        Ok(results)
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new().expect("failed to create tantivy index")
    }
}

impl std::fmt::Debug for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchIndex").finish_non_exhaustive()
    }
}

/// Remove characters that would otherwise be parsed as query syntax errors.
fn sanitize_query(query: &str) -> String {
    query
        .chars()
        .map(|ch| {
            if QUERY_SPECIAL_CHARS.contains(ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Build a short plain-text snippet around the first matching query term.
pub fn make_snippet(body: &str, query: &str) -> String {
    let clean = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        return String::new();
    }

    let lower = clean.to_lowercase();
    let match_pos = query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !term.is_empty())
        .filter_map(|term| lower.find(&term))
        .min();

    let Some(pos) = match_pos else {
        return truncate_chars(&clean, SNIPPET_MAX);
    };

    let mut start = pos.saturating_sub(SNIPPET_CONTEXT);
    while !clean.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (pos + SNIPPET_CONTEXT).min(clean.len());
    while end < clean.len() && !clean.is_char_boundary(end) {
        end += 1;
    }

    let mut snippet = clean[start..end].trim().to_string();
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < clean.len() {
        snippet.push('…');
    }
    snippet
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_search() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(Path::new("/a.md"), "hello world foo")
            .expect("add document");
        let results = idx
            .search("foo", &SearchOptions::default())
            .expect("search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, Path::new("/a.md"));
    }

    #[test]
    fn remove_document() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(Path::new("/a.md"), "foo")
            .expect("add document");
        idx.remove_document(Path::new("/a.md"))
            .expect("remove failed");
        assert!(
            idx.search("foo", &SearchOptions::default())
                .expect("search failed")
                .is_empty()
        );
    }

    #[test]
    fn search_uses_frontmatter_title() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(
            Path::new("/notes/a.md"),
            "---\ntitle = \"Real Title\"\n---\nBody with needle",
        )
        .expect("add document");
        let results = idx
            .search("needle", &SearchOptions::default())
            .expect("search failed");
        assert_eq!(results[0].title, "Real Title");
    }

    #[test]
    fn snippet_is_short_and_contains_term() {
        let body = format!(
            "{} needle in the middle {}",
            "start ".repeat(40),
            "end ".repeat(40)
        );
        let snippet = make_snippet(&body, "needle");
        assert!(snippet.contains("needle"));
        assert!(snippet.contains('…'));
        assert!(snippet.chars().count() < body.chars().count());
    }

    #[test]
    fn snippet_without_match_is_truncated() {
        let body = "word ".repeat(100);
        let snippet = make_snippet(&body, "absent");
        assert!(!snippet.is_empty());
        assert!(snippet.chars().count() <= SNIPPET_MAX + 1);
    }

    #[test]
    fn empty_query_returns_no_results() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(Path::new("/a.md"), "foo")
            .expect("add document");
        assert!(
            idx.search("   ", &SearchOptions::default())
                .expect("search failed")
                .is_empty()
        );
    }

    #[test]
    fn query_special_characters_are_ignored() {
        assert_eq!(
            sanitize_query("foo: (bar)")
                .split_whitespace()
                .collect::<Vec<_>>(),
            vec!["foo", "bar"]
        );
        assert!(sanitize_query("+-!").is_empty());
    }
}
