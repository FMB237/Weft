use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};

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
        Self { limit: 10, tags: Vec::new() }
    }
}

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
        let body = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema.clone());
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
        let term = tantivy::Term::from_field_text(
            self.schema_fields.path,
            path.to_str().unwrap_or(""),
        );
        self.writer.delete_term(term);

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();

        self.writer.add_document(doc!(
            self.schema_fields.path => path.to_str().unwrap_or(""),
            self.schema_fields.title => title.clone(),
            self.schema_fields.body => content.to_string()
        ));
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, path: &Path) -> tantivy::Result<()> {
        let term = tantivy::Term::from_field_text(
            self.schema_fields.path,
            path.to_str().unwrap_or(""),
        );
        self.writer.delete_term(term);
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search the index using tantivy full-text search.
    pub fn search(&self, query: &str, opts: &SearchOptions) -> tantivy::Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.schema_fields.body, self.schema_fields.title],
        );
        let tantivy_query = query_parser.parse_query(query)?;

        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(opts.limit))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let path_str = doc.get_first(self.schema_fields.path)
                .and_then(|v| v.as_text())
                .unwrap_or("");
            let title = doc.get_first(self.schema_fields.title)
                .and_then(|v| v.as_text())
                .unwrap_or("untitled");
            let path = PathBuf::from(path_str);

            // Re-read body for snippet generation from the original content.
            // Store body separately if snippet context needed; here we use title as fallback.
            results.push(SearchResult {
                path,
                title: title.to_string(),
                snippet: title.to_string(),
                score: _score,
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

fn snippet_for(_content: &str, _query: &str) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_search() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(Path::new("/a.md"), "hello world foo".into());
        let results = idx.search("foo", &SearchOptions::default()).expect("search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, Path::new("/a.md"));
    }

    #[test]
    fn remove_document() {
        let mut idx = SearchIndex::new().expect("failed to create index");
        idx.add_document(Path::new("/a.md"), "foo".into());
        idx.remove_document(Path::new("/a.md")).expect("remove failed");
        assert!(idx.search("foo", &SearchOptions::default()).expect("search failed").is_empty());
    }
}
