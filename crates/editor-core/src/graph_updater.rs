use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::file_ops::FileOps;
use crate::graph::LinkGraph;
use crate::parser::parse_markdown;
use crate::watcher::WatchEvent;

/// Incrementally maintains a `LinkGraph` from markdown file watch events.
///
/// On every watched change the updater re-parses the affected file, rebuilds
/// its node, removes its outgoing edges (wikilinks from that file), and re-adds
/// edges to the files it links to. Deletions remove the node entirely.
///
/// Link targets go through [`resolve_link`] so `[[Home]]`, `[[sub/alpha]]`, and
/// case-mismatched names all point at real files. Targets that cannot be
/// resolved become unresolved placeholder nodes until the file appears.
pub struct GraphUpdater<'a> {
    graph: &'a mut LinkGraph,
    vault: &'a dyn FileOps,
    files: Vec<PathBuf>,
}

impl std::fmt::Debug for GraphUpdater<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphUpdater")
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl<'a> GraphUpdater<'a> {
    pub fn new(graph: &'a mut LinkGraph, vault: &'a dyn FileOps) -> Self {
        let files = vault.list_files().unwrap_or_default();
        Self {
            graph,
            vault,
            files,
        }
    }

    /// Process a single watch event against the graph.
    pub fn apply_event(&mut self, event: &WatchEvent) -> anyhow::Result<()> {
        match event {
            WatchEvent::Created(path) | WatchEvent::Modified(path) => self.upsert_document(path),
            WatchEvent::Deleted(path) => {
                self.files.retain(|file| file != path);
                self.remove_document(path);
                Ok(())
            }
            WatchEvent::Renamed(from, to) => {
                self.files.retain(|file| file != from);
                self.remove_document(from);
                self.upsert_document(to)
            }
        }
    }

    /// Rebuild the graph for every file currently in the vault.
    pub fn rebuild(&mut self) -> anyhow::Result<()> {
        self.graph.clear();
        self.files = self.vault.list_files()?;
        let files = self.files.clone();
        for path in &files {
            // A single unreadable file should not abort the whole rebuild.
            let _ = self.upsert_document(path);
        }
        Ok(())
    }

    /// Parse and index a single file, rebuilding its outgoing edges.
    pub fn upsert(&mut self, path: &Path) -> anyhow::Result<()> {
        self.upsert_document(path)
    }

    fn upsert_document(&mut self, path: &Path) -> anyhow::Result<()> {
        let content = self.vault.read(path)?;
        let parsed = parse_markdown(path, &content);

        let title = parsed
            .frontmatter
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| {
                path.file_stem()
                    .and_then(OsStr::to_str)
                    .map(ToString::to_string)
            })
            .unwrap_or_default();

        let id = self.graph.add_node(path.to_path_buf(), title);
        self.graph.set_resolved(id, true);

        if !self.files.iter().any(|file| file == path) {
            self.files.push(path.to_path_buf());
            self.files.sort();
        }

        // Count occurrences per target so duplicate links become one weighted
        // edge instead of parallel edges.
        let mut counts: HashMap<PathBuf, usize> = HashMap::new();

        for link in &parsed.wikilinks {
            let target = link.target.trim();
            if target.is_empty() {
                continue;
            }
            let resolved_path = resolve_link(path, target, &self.files);
            let exists = resolved_path.is_some();
            let target_path = resolved_path.unwrap_or_else(|| unresolved_path(target));

            *counts.entry(target_path.clone()).or_insert(0) += 1;

            if self.graph.node_by_path(&target_path).is_none() {
                let display = if exists {
                    target_path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .unwrap_or(target)
                        .to_string()
                } else {
                    target.to_string()
                };
                let target_id = self.graph.add_node(target_path.clone(), display);
                self.graph.set_resolved(target_id, exists);
            } else if exists && let Some(target_id) = self.graph.node_by_path(&target_path) {
                self.graph.set_resolved(target_id, true);
            }
        }

        self.graph.remove_outgoing_edges(id);
        for (target_path, count) in counts {
            if let Some(target_id) = self.graph.node_by_path(&target_path) {
                self.graph.add_edge(id, target_id, count as f32);
            }
        }

        Ok(())
    }

    /// Remove a document node and all its edges.
    fn remove_document(&mut self, path: &Path) {
        if let Some(id) = self.graph.node_by_path(path) {
            self.graph.remove_node(id);
        }
    }
}

/// Resolve a wikilink target to a file in `files`, following the order used by
/// Obsidian as closely as a small vault needs:
///
/// 1. exact path relative to the source file
/// 2. the same path with `.md` appended
/// 3. case-insensitive full-path or file-stem match (same directory first)
/// 4. exact or case-insensitive match from the vault root
///
/// Returns `None` when nothing matches; the caller creates a placeholder.
pub fn resolve_link(source: &Path, target: &str, files: &[PathBuf]) -> Option<PathBuf> {
    let target = target.trim().trim_start_matches("./");
    if target.is_empty() {
        return None;
    }

    let source_dir = source.parent().unwrap_or_else(|| Path::new(""));
    let target_path = PathBuf::from(target);

    let find_exact = |candidate: &Path| {
        files
            .iter()
            .find(|file| file.as_path() == candidate)
            .cloned()
    };
    let with_md = |path: &Path| -> PathBuf {
        if path.extension().is_some() {
            path.to_path_buf()
        } else {
            path.with_extension("md")
        }
    };

    // 1-2: relative to the source file's directory.
    let relative = source_dir.join(&target_path);
    if let Some(found) = find_exact(&relative) {
        return Some(found);
    }
    if let Some(found) = find_exact(&with_md(&relative)) {
        return Some(found);
    }

    // 3a: case-insensitive relative path.
    let relative_key = relative.with_extension("").to_string_lossy().to_lowercase();
    if let Some(found) = files
        .iter()
        .find(|file| file.with_extension("").to_string_lossy().to_lowercase() == relative_key)
    {
        return Some(found.clone());
    }

    // 3b: case-insensitive file stem, preferring the source directory.
    let target_stem = target_path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(target);
    let mut stem_matches: Vec<&PathBuf> = files
        .iter()
        .filter(|file| {
            file.file_stem()
                .and_then(OsStr::to_str)
                .is_some_and(|stem| stem.eq_ignore_ascii_case(target_stem))
        })
        .collect();
    stem_matches.sort_by_key(|file| file.components().count());

    if let Some(found) = stem_matches
        .iter()
        .find(|file| file.parent().is_some_and(|parent| parent == source_dir))
    {
        return Some((*found).clone());
    }
    if let Some(found) = stem_matches.first() {
        return Some((*found).clone());
    }

    // 4: from the vault root.
    if let Some(found) = find_exact(&target_path) {
        return Some(found);
    }
    if let Some(found) = find_exact(&with_md(&target_path)) {
        return Some(found);
    }
    let root_key = target_path
        .with_extension("")
        .to_string_lossy()
        .to_lowercase();
    files
        .iter()
        .find(|file| file.with_extension("").to_string_lossy().to_lowercase() == root_key)
        .cloned()
}

/// Path used for a placeholder node for a missing link target.
fn unresolved_path(target: &str) -> PathBuf {
    let mut path = PathBuf::from(target);
    if path.extension().is_none() {
        path.set_extension("md");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    struct TestVault {
        dir: PathBuf,
    }

    impl FileOps for TestVault {
        fn list_files(&self) -> anyhow::Result<Vec<PathBuf>> {
            fn walk(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_dir() {
                        walk(root, &path, files)?;
                    } else if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                    {
                        files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                    }
                }
                Ok(())
            }

            let mut files = Vec::new();
            walk(&self.dir, &self.dir, &mut files)?;
            files.sort();
            Ok(files)
        }

        fn read(&self, path: &Path) -> anyhow::Result<String> {
            Ok(std::fs::read_to_string(self.dir.join(path)).unwrap())
        }

        fn write(&self, _path: &Path, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn delete(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }

        fn resolve(&self, path: &Path) -> PathBuf {
            self.dir.join(path)
        }
    }

    #[test]
    fn updater_adds_node_and_edges_from_wikilinks() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A\nSee [[b.md]] and [[c.md]].").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B\nnothing.").unwrap();
        std::fs::write(dir.path().join("c.md"), "# C\nnothing.").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Created(PathBuf::from("a.md")))
                .unwrap();
        }
        let a_id = graph.node_by_path(Path::new("a.md")).unwrap();
        let b_id = graph.node_by_path(Path::new("b.md")).unwrap();
        let c_id = graph.node_by_path(Path::new("c.md")).unwrap();

        assert_eq!(graph.outgoing_edges(a_id).len(), 2);
        assert!(graph.outgoing_edges(a_id).iter().any(|e| e.target == b_id));
        assert!(graph.outgoing_edges(a_id).iter().any(|e| e.target == c_id));
    }

    #[test]
    fn updater_removes_node_on_delete() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Created(PathBuf::from("a.md")))
                .unwrap();
        }
        assert!(graph.node_by_path(Path::new("a.md")).is_some());

        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Deleted(PathBuf::from("a.md")))
                .unwrap();
        }
        assert!(graph.node_by_path(Path::new("a.md")).is_none());
    }

    #[test]
    fn updater_rebuilds_edges_on_modify() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A\n[[b.md]]").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B").unwrap();
        std::fs::write(dir.path().join("c.md"), "# C").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Created(PathBuf::from("a.md")))
                .unwrap();
        }

        // Change a.md to link to c instead of b.
        std::fs::write(dir.path().join("a.md"), "# A\n[[c.md]]").unwrap();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Modified(PathBuf::from("a.md")))
                .unwrap();
        }

        let a_id = graph.node_by_path(Path::new("a.md")).unwrap();
        let b_id = graph.node_by_path(Path::new("b.md")).unwrap();
        let c_id = graph.node_by_path(Path::new("c.md")).unwrap();

        assert_eq!(graph.outgoing_edges(a_id).len(), 1);
        assert!(graph.outgoing_edges(a_id).iter().any(|e| e.target == c_id));
        assert!(!graph.outgoing_edges(a_id).iter().any(|e| e.target == b_id));
    }

    #[test]
    fn updater_resolves_extensionless_and_case_insensitive_links() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(
            dir.path().join("README.md"),
            "# Welcome\nSee [[home]] and [[Projects/Alpha]].",
        )
        .unwrap();
        std::fs::write(dir.path().join("Home.md"), "# Home").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/alpha.md"), "# Alpha").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater.rebuild().unwrap();
        }

        let readme = graph.node_by_path(Path::new("README.md")).unwrap();
        let home = graph.node_by_path(Path::new("Home.md")).unwrap();
        let alpha = graph.node_by_path(Path::new("sub/alpha.md")).unwrap();

        assert_eq!(graph.outgoing_edges(readme).len(), 2);
        assert!(
            graph
                .outgoing_edges(readme)
                .iter()
                .any(|e| e.target == home)
        );
        assert!(
            graph
                .outgoing_edges(readme)
                .iter()
                .any(|e| e.target == alpha)
        );
    }

    #[test]
    fn updater_creates_unresolved_placeholder_for_missing_target() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A\n[[Missing Note]]").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater.rebuild().unwrap();
        }

        let missing = graph
            .node_by_path(Path::new("Missing Note.md"))
            .expect("placeholder node exists");
        assert!(!graph.node(missing).unwrap().resolved);
    }

    #[test]
    fn updater_resolves_placeholder_when_file_appears() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A\n[[Missing Note]]").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater.rebuild().unwrap();
        }

        let missing = graph
            .node_by_path(Path::new("Missing Note.md"))
            .expect("placeholder node exists");
        assert!(!graph.node(missing).unwrap().resolved);

        std::fs::write(dir.path().join("Missing Note.md"), "# Missing").unwrap();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater
                .apply_event(&WatchEvent::Created(PathBuf::from("Missing Note.md")))
                .unwrap();
        }

        let missing = graph
            .node_by_path(Path::new("Missing Note.md"))
            .expect("placeholder became a real node");
        assert!(graph.node(missing).unwrap().resolved);

        let a = graph.node_by_path(Path::new("a.md")).unwrap();
        assert!(
            graph
                .outgoing_edges(a)
                .iter()
                .any(|edge| edge.target == missing)
        );
    }

    #[test]
    fn updater_deduplicates_duplicate_links_with_weight() {
        let dir = tempdir().unwrap();
        let vault = TestVault {
            dir: dir.path().to_path_buf(),
        };

        std::fs::write(dir.path().join("a.md"), "# A\n[[b]]\n[[b]]\n").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B").unwrap();

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, &vault);
            updater.rebuild().unwrap();
        }

        let a = graph.node_by_path(Path::new("a.md")).unwrap();
        let edges = graph.outgoing_edges(a);
        assert_eq!(edges.len(), 1);
        assert!((edges[0].weight - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_link_handles_aliases_and_missing_targets() {
        let files = vec![
            PathBuf::from("Home.md"),
            PathBuf::from("sub/alpha.md"),
            PathBuf::from("Meeting Notes.md"),
        ];

        assert_eq!(
            resolve_link(Path::new("README.md"), "Home", &files),
            Some(PathBuf::from("Home.md"))
        );
        assert_eq!(
            resolve_link(Path::new("README.md"), "Projects/Alpha", &files),
            Some(PathBuf::from("sub/alpha.md"))
        );
        assert_eq!(
            resolve_link(Path::new("sub/alpha.md"), "Meeting Notes", &files),
            Some(PathBuf::from("Meeting Notes.md"))
        );
        assert_eq!(
            resolve_link(Path::new("README.md"), "home", &files),
            Some(PathBuf::from("Home.md"))
        );
        assert_eq!(resolve_link(Path::new("README.md"), "Nope", &files), None);
    }
}
