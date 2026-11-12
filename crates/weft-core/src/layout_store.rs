use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::graph::LinkGraph;
use egui::Pos2;

/// Serializes and deserializes node layout positions to/from disk.
///
/// Positions are keyed by file path rather than `NodeId` because slotmap keys
/// are not stable across sessions (they can be reused after node removal).
/// Paths give a stable, human-readable identifier for each node.
///
/// Internally positions are stored as `[f32; 2]` arrays so they serialize
/// with plain `serde_json` without requiring egui's `serde` feature.
#[derive(Debug, Clone, Default)]
pub struct LayoutStore {
    positions: HashMap<PathBuf, [f32; 2]>,
}

impl LayoutStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    /// Number of stored positions.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Returns `true` if the store contains no positions.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Insert or update the position for the given path.
    pub fn set_position(&mut self, path: PathBuf, position: Pos2) {
        self.positions.insert(path, [position.x, position.y]);
    }

    /// Get the stored position for a path, if any.
    pub fn position(&self, path: &Path) -> Option<Pos2> {
        self.positions.get(path).map(|p| Pos2::new(p[0], p[1]))
    }

    /// Remove the stored position for a path.
    pub fn remove_position(&mut self, path: &Path) -> Option<Pos2> {
        self.positions.remove(path).map(|p| Pos2::new(p[0], p[1]))
    }

    /// All stored (path, position) pairs.
    pub fn positions(&self) -> Vec<(PathBuf, Pos2)> {
        let mut out: Vec<(PathBuf, Pos2)> = self
            .positions
            .iter()
            .map(|(k, v)| (k.clone(), Pos2::new(v[0], v[1])))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Serialize the store to a JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.positions)
    }

    /// Deserialize a store from a JSON string.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let positions: HashMap<PathBuf, [f32; 2]> = serde_json::from_str(json)?;
        Ok(Self { positions })
    }

    /// Write the store to a JSON file at `path`.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, json)
    }

    /// Load a store from a JSON file at `path`.
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Apply all stored positions to the graph, setting `node.position` for
    /// each node whose path matches a stored entry. Returns the number of
    /// positions applied.
    pub fn apply_to_graph(&self, graph: &mut LinkGraph) -> usize {
        let mut applied = 0;
        for id in graph.node_ids() {
            if let Some(node) = graph.node(id)
                && let Some(pos) = self.positions.get(&node.path)
            {
                graph.set_node_position(id, Pos2::new(pos[0], pos[1]));
                applied += 1;
            }
        }
        applied
    }

    /// Capture all node positions that have been set on the graph into this
    /// store, keyed by path. Overwrites any existing entries.
    pub fn capture_from_graph(&mut self, graph: &LinkGraph) {
        self.positions.clear();
        for id in graph.node_ids() {
            if let Some(node) = graph.node(id)
                && let Some(pos) = node.position
            {
                self.positions.insert(node.path.clone(), [pos.x, pos.y]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip_json() {
        let mut store = LayoutStore::new();
        store.set_position("/a.md".into(), Pos2::new(1.0, 2.0));
        store.set_position("/b.md".into(), Pos2::new(3.0, 4.0));

        let json = store.to_json().unwrap();
        let loaded = LayoutStore::from_json(&json).unwrap();
        assert_eq!(
            loaded.position(Path::new("/a.md")),
            Some(Pos2::new(1.0, 2.0))
        );
        assert_eq!(
            loaded.position(Path::new("/b.md")),
            Some(Pos2::new(3.0, 4.0))
        );
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn store_save_and_load_file() {
        let mut store = LayoutStore::new();
        store.set_position("/a.md".into(), Pos2::new(5.0, 6.0));
        let file = std::env::temp_dir().join("layout_store_test.json");
        store.save_to_file(&file).unwrap();
        let loaded = LayoutStore::load_from_file(&file).unwrap();
        assert_eq!(
            loaded.position(Path::new("/a.md")),
            Some(Pos2::new(5.0, 6.0))
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn store_apply_to_graph() {
        let mut graph = LinkGraph::new();
        let a = graph.add_node("/a.md".into(), "A".into());
        let b = graph.add_node("/b.md".into(), "B".into());

        let mut store = LayoutStore::new();
        store.set_position("/a.md".into(), Pos2::new(10.0, 20.0));
        // /b.md has no stored position, should be left alone.
        let applied = store.apply_to_graph(&mut graph);
        assert_eq!(applied, 1);
        assert_eq!(graph.node(a).unwrap().position, Some(Pos2::new(10.0, 20.0)));
        assert_eq!(graph.node(b).unwrap().position, None);
    }

    #[test]
    fn store_capture_from_graph() {
        let mut graph = LinkGraph::new();
        let a = graph.add_node("/a.md".into(), "A".into());
        let _b = graph.add_node("/b.md".into(), "B".into());
        assert!(graph.set_node_position(a, Pos2::new(1.0, 2.0)));
        // b has no position set.

        let mut store = LayoutStore::new();
        store.capture_from_graph(&graph);
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.position(Path::new("/a.md")),
            Some(Pos2::new(1.0, 2.0))
        );
        assert_eq!(store.position(Path::new("/b.md")), None);
    }

    #[test]
    fn store_is_empty_and_len() {
        let mut store = LayoutStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        store.set_position("/a.md".into(), Pos2::ZERO);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn store_remove_position() {
        let mut store = LayoutStore::new();
        store.set_position("/a.md".into(), Pos2::new(1.0, 2.0));
        assert_eq!(
            store.remove_position(Path::new("/a.md")),
            Some(Pos2::new(1.0, 2.0))
        );
        assert_eq!(store.remove_position(Path::new("/a.md")), None);
        assert!(store.is_empty());
    }

    #[test]
    fn store_positions_sorted() {
        let mut store = LayoutStore::new();
        store.set_position("/c.md".into(), Pos2::ZERO);
        store.set_position("/a.md".into(), Pos2::ZERO);
        store.set_position("/b.md".into(), Pos2::ZERO);
        let stored = store.positions();
        let paths: Vec<&Path> = stored.iter().map(|(p, _)| p.as_path()).collect();
        assert_eq!(
            paths,
            vec![Path::new("/a.md"), Path::new("/b.md"), Path::new("/c.md")]
        );
    }
}
