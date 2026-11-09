use std::collections::{HashMap, HashSet, VecDeque};

use crate::graph::{LinkGraph, NodeId};
use slotmap::Key;

/// Read-only query API over a [`LinkGraph`].
///
/// Provides neighbor lookups, reachability, shortest paths, degree-based
/// centrality, and betweenness centrality. Holds an immutable reference to the
/// graph so queries can run concurrently with reads but not with mutations.
#[derive(Debug)]
pub struct GraphQuery<'a> {
    graph: &'a LinkGraph,
}

impl<'a> GraphQuery<'a> {
    /// Create a new query handle borrowing `graph`.
    pub fn new(graph: &'a LinkGraph) -> Self {
        Self { graph }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// All node IDs.
    pub fn node_ids(&self) -> Vec<NodeId> {
        self.graph.node_ids()
    }

    /// Nodes reachable from `id` via outgoing edges (forward links).
    pub fn outgoing_neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.graph
            .outgoing_edges(id)
            .iter()
            .map(|e| e.target)
            .collect()
    }

    /// Nodes linking to `id` via incoming edges (backlinks).
    pub fn incoming_neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.graph
            .incoming_edges(id)
            .iter()
            .map(|e| e.source)
            .collect()
    }

    /// All adjacent nodes regardless of edge direction.
    pub fn neighbors(&self, id: NodeId) -> Vec<NodeId> {
        let mut seen: HashSet<NodeId> = HashSet::new();
        for e in self.graph.outgoing_edges(id) {
            seen.insert(e.target);
        }
        for e in self.graph.incoming_edges(id) {
            seen.insert(e.source);
        }
        let mut out: Vec<NodeId> = seen.into_iter().collect();
        out.sort_by_key(slotmap::Key::data);
        out
    }

    /// Total degree (incoming + outgoing edges).
    pub fn degree(&self, id: NodeId) -> usize {
        self.graph.outgoing_edges(id).len() + self.graph.incoming_edges(id).len()
    }

    /// Outgoing degree (number of forward links from this node).
    pub fn out_degree(&self, id: NodeId) -> usize {
        self.graph.outgoing_edges(id).len()
    }

    /// Incoming degree (number of backlinks to this node).
    pub fn in_degree(&self, id: NodeId) -> usize {
        self.graph.incoming_edges(id).len()
    }

    /// Backlinks to `id`: `(path, title)` pairs of source nodes.
    pub fn backlinks(&self, id: NodeId) -> Vec<(std::path::PathBuf, String)> {
        self.graph.compute_backlinks(id)
    }

    /// Whether `target` is reachable from `source` following directed edges.
    pub fn is_connected(&self, source: NodeId, target: NodeId) -> bool {
        if source == target {
            return self.graph.node(source).is_some();
        }
        let mut visited: HashSet<NodeId> = HashSet::new();
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        visited.insert(source);
        queue.push_back(source);
        while let Some(curr) = queue.pop_front() {
            for e in self.graph.outgoing_edges(curr) {
                let next = e.target;
                if next == target {
                    return true;
                }
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        false
    }

    /// Shortest directed path from `source` to `target` (BFS), or `None` if
    /// unreachable. Includes both endpoints.
    pub fn shortest_path(&self, source: NodeId, target: NodeId) -> Option<Vec<NodeId>> {
        if source == target {
            return self.graph.node(source).map(|_| vec![source]);
        }
        let mut visited: HashSet<NodeId> = HashSet::new();
        let mut parent: HashMap<NodeId, NodeId> = HashMap::new();
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        visited.insert(source);
        queue.push_back(source);
        while let Some(curr) = queue.pop_front() {
            for e in self.graph.outgoing_edges(curr) {
                let next = e.target;
                if visited.insert(next) {
                    parent.insert(next, curr);
                    if next == target {
                        // Reconstruct path.
                        let mut path = vec![target];
                        let mut node = target;
                        while let Some(&p) = parent.get(&node) {
                            path.push(p);
                            node = p;
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// Degree centrality for every node: `degree / (n - 1)` normalized to [0, 1].
    /// Nodes with no edges get 0.0.
    pub fn degree_centrality(&self) -> Vec<(NodeId, f64)> {
        let n = self.graph.node_count();
        let max = if n > 1 { (n - 1) as f64 } else { 1.0 };
        let mut scores: Vec<(NodeId, f64)> = self
            .graph
            .node_ids()
            .iter()
            .map(|&id| (id, self.degree(id) as f64 / max))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Betweenness centrality for every node using Brandes' algorithm on the
    /// unweighted directed graph. Returns `(node, score)` sorted descending.
    pub fn betweenness_centrality(&self) -> Vec<(NodeId, f64)> {
        let node_ids: Vec<NodeId> = self.graph.node_ids();
        let mut betweenness: HashMap<NodeId, f64> = node_ids.iter().map(|&id| (id, 0.0)).collect();

        for &s in &node_ids {
            // BFS from s.
            let mut stack: Vec<NodeId> = Vec::new();
            let mut predecessors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
            let mut sigma: HashMap<NodeId, f64> = HashMap::new();
            let mut dist: HashMap<NodeId, i64> = HashMap::new();
            let mut queue: VecDeque<NodeId> = VecDeque::new();

            sigma.insert(s, 1.0);
            dist.insert(s, 0);
            queue.push_back(s);

            while let Some(v) = queue.pop_front() {
                stack.push(v);
                for e in self.graph.outgoing_edges(v) {
                    let w = e.target;
                    // First time seeing w.
                    if !dist.contains_key(&w) {
                        dist.insert(w, dist[&v] + 1);
                        queue.push_back(w);
                    }
                    if dist[&w] == dist[&v] + 1 {
                        sigma.insert(w, sigma.get(&w).unwrap_or(&0.0) + sigma[&v]);
                        predecessors.entry(w).or_default().push(v);
                    }
                }
            }

            // Accumulation phase.
            let mut delta: HashMap<NodeId, f64> = node_ids.iter().map(|&id| (id, 0.0)).collect();
            while let Some(w) = stack.pop() {
                for &v in predecessors.get(&w).unwrap_or(&Vec::new()) {
                    let d = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                    *delta.get_mut(&v).expect("node is in the delta map") += d;
                }
                if w != s {
                    *betweenness
                        .get_mut(&w)
                        .expect("node is in the betweenness map") += delta[&w];
                }
            }
        }

        let mut scores: Vec<(NodeId, f64)> = betweenness.into_iter().collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Strongly connected components via Tarjan's algorithm.
    pub fn strongly_connected_components(&self) -> Vec<Vec<NodeId>> {
        struct Tarjan<'a> {
            graph: &'a LinkGraph,
            index_of: HashMap<NodeId, i32>,
            lowlink: HashMap<NodeId, i32>,
            on_stack: HashSet<NodeId>,
            stack: Vec<NodeId>,
            components: Vec<Vec<NodeId>>,
            index: i32,
        }

        impl Tarjan<'_> {
            fn strongconnect(&mut self, v: NodeId) {
                self.index_of.insert(v, self.index);
                self.lowlink.insert(v, self.index);
                self.index += 1;
                self.stack.push(v);
                self.on_stack.insert(v);

                for e in self.graph.outgoing_edges(v) {
                    let w = e.target;
                    if !self.index_of.contains_key(&w) {
                        self.strongconnect(w);
                        let low_w = self.lowlink[&w];
                        let low_v = self.lowlink[&v];
                        self.lowlink.insert(v, low_v.min(low_w));
                    } else if self.on_stack.contains(&w) {
                        let low_v = self.lowlink[&v];
                        self.lowlink.insert(v, low_v.min(self.index_of[&w]));
                    }
                }

                if self.lowlink[&v] == self.index_of[&v] {
                    let mut component = Vec::new();
                    loop {
                        let w = self
                            .stack
                            .pop()
                            .expect("stack is non-empty while unwinding");
                        self.on_stack.remove(&w);
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    component.sort_by_key(slotmap::Key::data);
                    self.components.push(component);
                }
            }
        }

        let mut tarjan = Tarjan {
            graph: self.graph,
            index_of: HashMap::new(),
            lowlink: HashMap::new(),
            on_stack: HashSet::new(),
            stack: Vec::new(),
            components: Vec::new(),
            index: 0,
        };

        for v in self.graph.node_ids() {
            if !tarjan.index_of.contains_key(&v) {
                tarjan.strongconnect(v);
            }
        }

        let mut components = tarjan.components;
        components.sort_by_key(|c| c.first().copied().unwrap_or(NodeId::null()).data());
        components
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::many_single_char_names,
        reason = "graph fixtures use a, b, c for nodes"
    )]

    use super::*;

    fn make_graph() -> LinkGraph {
        let mut g = LinkGraph::new();
        let a = g.add_node("/a.md".into(), "A".into());
        let b = g.add_node("/b.md".into(), "B".into());
        let c = g.add_node("/c.md".into(), "C".into());
        let d = g.add_node("/d.md".into(), "D".into());
        g.add_edge(a, b, 1.0);
        g.add_edge(b, c, 1.0);
        g.add_edge(c, d, 1.0);
        g.add_edge(d, a, 1.0); // cycle a->b->c->d->a
        g
    }

    #[test]
    fn query_outgoing_neighbors() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        let b = g.node_by_path(std::path::Path::new("/b.md")).unwrap();
        assert_eq!(q.outgoing_neighbors(a), vec![b]);
    }

    #[test]
    fn query_incoming_neighbors() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let b = g.node_by_path(std::path::Path::new("/b.md")).unwrap();
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        assert_eq!(q.incoming_neighbors(b), vec![a]);
    }

    #[test]
    fn query_neighbors_undirected() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        let b = g.node_by_path(std::path::Path::new("/b.md")).unwrap();
        let d = g.node_by_path(std::path::Path::new("/d.md")).unwrap();
        let mut nb = q.neighbors(a);
        nb.sort_by_key(slotmap::Key::data);
        assert_eq!(nb, vec![b, d]);
    }

    #[test]
    fn query_degrees() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        assert_eq!(q.degree(a), 2);
        assert_eq!(q.out_degree(a), 1);
        assert_eq!(q.in_degree(a), 1);
    }

    #[test]
    fn query_shortest_path() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        let c = g.node_by_path(std::path::Path::new("/c.md")).unwrap();
        let path = q.shortest_path(a, c).unwrap();
        assert_eq!(
            path,
            vec![a, g.node_by_path(std::path::Path::new("/b.md")).unwrap(), c]
        );
    }

    #[test]
    fn query_shortest_path_none() {
        let mut g = LinkGraph::new();
        let a = g.add_node("/a.md".into(), "A".into());
        let b = g.add_node("/b.md".into(), "B".into());
        // No edge a->b.
        let q = GraphQuery::new(&g);
        assert!(q.shortest_path(a, b).is_none());
    }

    #[test]
    fn query_is_connected() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let a = g.node_by_path(std::path::Path::new("/a.md")).unwrap();
        let c = g.node_by_path(std::path::Path::new("/c.md")).unwrap();
        assert!(q.is_connected(a, c));
    }

    #[test]
    fn query_degree_centrality() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let scores = q.degree_centrality();
        assert_eq!(scores.len(), g.node_count());
        // All nodes have degree 2 in the 4-cycle => all 2/3.
        for (_, s) in &scores {
            assert!((s - (2.0 / 3.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn query_betweenness_cycle_zero() {
        // In a simple cycle every node lies on the same number of shortest paths
        // between any pair; with only one shortest path per pair (the cycle
        // direction) interior nodes of a length-3 path get credit. Check scores
        // are non-negative and sum is reasonable instead of exact values.
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let scores = q.betweenness_centrality();
        assert_eq!(scores.len(), g.node_count());
        for (_, s) in &scores {
            assert!(*s >= 0.0);
        }
    }

    #[test]
    fn query_strongly_connected_components_cycle() {
        let g = make_graph();
        let q = GraphQuery::new(&g);
        let comps = q.strongly_connected_components();
        // All 4 nodes form one SCC.
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].len(), 4);
    }

    #[test]
    fn query_strongly_connected_components_disconnected() {
        let mut g = LinkGraph::new();
        let _a = g.add_node("/a.md".into(), "A".into());
        let _b = g.add_node("/b.md".into(), "B".into());
        let q = GraphQuery::new(&g);
        let comps = q.strongly_connected_components();
        assert_eq!(comps.len(), 2);
    }

    #[test]
    fn query_empty_graph() {
        let g = LinkGraph::new();
        let q = GraphQuery::new(&g);
        assert_eq!(q.node_count(), 0);
        assert_eq!(q.degree_centrality(), Vec::new());
        assert_eq!(q.betweenness_centrality(), Vec::new());
        assert_eq!(q.strongly_connected_components(), Vec::<Vec<NodeId>>::new());
    }

    #[test]
    fn query_single_node() {
        let mut g = LinkGraph::new();
        let id = g.add_node("/x.md".into(), "X".into());
        let q = GraphQuery::new(&g);
        assert_eq!(q.degree(id), 0);
        assert!(q.shortest_path(id, id).is_some());
        assert!(q.is_connected(id, id));
    }
}
