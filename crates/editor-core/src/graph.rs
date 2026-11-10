use egui::Pos2;
use slotmap::Key;
use std::path::{Path, PathBuf};

slotmap::new_key_type! { pub struct NodeId; }
slotmap::new_key_type! { pub struct EdgeId; }

/// A node in the link graph representing a markdown file.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: NodeId,
    pub path: PathBuf,
    pub title: String,
    pub position: Option<Pos2>,
    /// `false` for placeholder nodes created by links to files that do not
    /// exist yet. They are drawn differently and disappear once the file is
    /// created.
    pub resolved: bool,
}

/// An edge in the link graph representing a wikilink from one file to another.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub weight: f32,
}

/// The link graph: nodes are files, edges are wikilinks between them.
#[derive(Debug, Clone, Default)]
pub struct LinkGraph {
    nodes: slotmap::SlotMap<NodeId, GraphNode>,
    edges: slotmap::SlotMap<EdgeId, GraphEdge>,
    path_to_node: std::collections::HashMap<PathBuf, NodeId>,
}

impl LinkGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a node for the given file path.
    pub fn add_node(&mut self, path: PathBuf, title: String) -> NodeId {
        if let Some(&id) = self.path_to_node.get(&path) {
            let node = self
                .nodes
                .get_mut(id)
                .expect("path map points at a live node");
            node.title = title;
            return id;
        }
        let id = self.nodes.insert(GraphNode {
            id: NodeId::null(),
            path: path.clone(),
            title,
            position: None,
            resolved: true,
        });
        let node = self.nodes.get_mut(id).expect("node was just inserted");
        node.id = id;
        self.path_to_node.insert(path, id);
        id
    }

    /// Mark a node as resolved (backed by a real file) or unresolved (a
    /// placeholder for a missing link target).
    pub fn set_resolved(&mut self, id: NodeId, resolved: bool) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            node.resolved = resolved;
            true
        } else {
            false
        }
    }

    /// Get a node by ID.
    pub fn node(&self, id: NodeId) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Look up a node ID by file path.
    pub fn node_by_path(&self, path: &Path) -> Option<NodeId> {
        self.path_to_node.get(path).copied()
    }

    /// All node IDs.
    pub fn node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().collect()
    }

    /// All edges.
    pub fn edges(&self) -> Vec<GraphEdge> {
        self.edges.values().cloned().collect()
    }

    /// Add a directed edge from source to target.
    pub fn add_edge(&mut self, source: NodeId, target: NodeId, weight: f32) -> EdgeId {
        let id = self.edges.insert(GraphEdge {
            id: EdgeId::null(),
            source,
            target,
            weight,
        });
        let edge = self.edges.get_mut(id).expect("edge was just inserted");
        edge.id = id;
        id
    }

    /// Incoming edges to a node (backlinks).
    pub fn incoming_edges(&self, target: NodeId) -> Vec<&GraphEdge> {
        self.edges.values().filter(|e| e.target == target).collect()
    }

    /// Outgoing edges from a node (forward links).
    pub fn outgoing_edges(&self, source: NodeId) -> Vec<&GraphEdge> {
        self.edges.values().filter(|e| e.source == source).collect()
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Compute backlinks (incoming links) for a node, returning path + title pairs.
    pub fn compute_backlinks(&self, target: NodeId) -> Vec<(PathBuf, String)> {
        self.incoming_edges(target)
            .iter()
            .filter_map(|e| {
                self.nodes
                    .get(e.source)
                    .map(|n| (n.path.clone(), n.title.clone()))
            })
            .collect()
    }

    /// Remove all outgoing edges from `source`.
    pub fn remove_outgoing_edges(&mut self, source: NodeId) {
        let edge_ids: Vec<EdgeId> = self
            .edges
            .iter()
            .filter(|(_, e)| e.source == source)
            .map(|(id, _)| id)
            .collect();
        for id in edge_ids {
            self.edges.remove(id);
        }
    }

    /// Remove a node and all edges to and from it. Returns `false` if the node
    /// did not exist.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let node = match self.nodes.get(id) {
            Some(n) => n.clone(),
            None => return false,
        };
        self.nodes.remove(id);
        self.path_to_node.remove(&node.path);

        let edge_ids: Vec<EdgeId> = self
            .edges
            .iter()
            .filter(|(_, e)| e.source == id || e.target == id)
            .map(|(id, _)| id)
            .collect();
        for id in edge_ids {
            self.edges.remove(id);
        }
        true
    }

    /// Remove all nodes and edges.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.path_to_node.clear();
    }

    /// Set the layout position of a node. Returns `false` if the node did not exist.
    pub fn set_node_position(&mut self, id: NodeId, position: Pos2) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            node.position = Some(position);
            true
        } else {
            false
        }
    }

    /// Update the path stored on an existing node, keeping `path_to_node`
    /// consistent. Returns `false` if the node did not exist.
    pub fn update_node_path(&mut self, id: NodeId, path: PathBuf) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            self.path_to_node.remove(&node.path);
            node.path.clone_from(&path);
            self.path_to_node.insert(path, id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_find_node() {
        let mut g = LinkGraph::new();
        let id = g.add_node(PathBuf::from("/a.md"), "A".into());
        assert_eq!(g.node(id).unwrap().title, "A");
        assert_eq!(g.node_by_path(Path::new("/a.md")), Some(id));
    }

    #[test]
    fn edges_and_backlinks() {
        let mut g = LinkGraph::new();
        let a = g.add_node(PathBuf::from("/a.md"), "A".into());
        let b = g.add_node(PathBuf::from("/b.md"), "B".into());
        g.add_edge(a, b, 1.0);

        assert_eq!(g.outgoing_edges(a).len(), 1);
        assert_eq!(g.incoming_edges(b).len(), 1);
        assert_eq!(g.incoming_edges(a).len(), 0);
    }
}
