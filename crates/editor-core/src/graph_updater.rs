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
/// Nodes referenced as link targets are added lazily so the graph always
/// contains every page mentioned somewhere, even if it has no outgoing links.
pub struct GraphUpdater<'a> {
    graph: &'a mut LinkGraph,
    vault: &'a dyn FileOps,
}

impl std::fmt::Debug for GraphUpdater<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphUpdater").finish_non_exhaustive()
    }
}

impl<'a> GraphUpdater<'a> {
    pub fn new(graph: &'a mut LinkGraph, vault: &'a dyn FileOps) -> Self {
        Self { graph, vault }
    }

    /// Process a single watch event against the graph.
    pub fn apply_event(&mut self, event: &WatchEvent) -> anyhow::Result<()> {
        match event {
            WatchEvent::Created(path) | WatchEvent::Modified(path) => self.upsert_document(path),
            WatchEvent::Deleted(path) => {
                self.remove_document(path);
                Ok(())
            }
            WatchEvent::Renamed(from, to) => {
                self.remove_document(from);
                self.upsert_document(to)
            }
        }
    }

    /// Parse and index a single file, rebuilding its outgoing edges.
    fn upsert_document(&mut self, path: &Path) -> anyhow::Result<()> {
        let content = self.vault.read(path)?;

        let parsed = parse_markdown(path, &content);

        // Find or create the node for this file.
        let id = match self.graph.node_by_path(path) {
            Some(id) => {
                self.graph.update_node_path(id, path.to_path_buf());
                id
            }
            None => self.graph.add_node(
                path.to_path_buf(),
                parsed.frontmatter.title.clone().unwrap_or_default(),
            ),
        };

        // Rebuild outgoing edges: remove existing ones, then re-add from wikilinks.
        self.graph.remove_outgoing_edges(id);

        for link in &parsed.wikilinks {
            let target_path = Self::resolve_link(path, &link.target);
            let target_id = match self.graph.node_by_path(&target_path) {
                Some(tid) => tid,
                None => self.graph.add_node(target_path.clone(), String::new()),
            };
            self.graph.add_edge(id, target_id, 1.0);
        }

        Ok(())
    }

    /// Remove a document node and all its edges.
    fn remove_document(&mut self, path: &Path) {
        if let Some(id) = self.graph.node_by_path(path) {
            self.graph.remove_node(id);
        }
    }

    /// Resolve a wikilink target relative to the source file's directory.
    fn resolve_link(source: &Path, target: &str) -> PathBuf {
        let target_path = PathBuf::from(target);
        if target_path.is_absolute() {
            return target_path;
        }
        let source_dir = source.parent().unwrap_or(Path::new(""));
        source_dir.join(target_path)
    }
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
            Ok(Vec::new())
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
}
