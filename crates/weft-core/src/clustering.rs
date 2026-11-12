use std::collections::HashMap;

use crate::graph::{LinkGraph, NodeId};
use slotmap::Key;

/// A cluster (community) of nodes detected by community detection.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Stable cluster id (the label assigned during propagation).
    pub id: usize,
    /// Node ids belonging to this cluster, in index order.
    pub nodes: Vec<NodeId>,
}

impl Cluster {
    /// Number of nodes in this cluster.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if the cluster contains no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Detect communities in the link graph using the Label Propagation Algorithm.
///
/// Each node starts in its own community. Repeatedly, each node moves to the
/// community shared by the most of its neighbors (ties broken by lowest label
/// for determinism). The process stops when no node changes community or the
/// iteration cap is reached.
///
/// The graph is treated as undirected for clustering purposes.
///
/// Returns clusters sorted by cluster id, with nodes in each cluster in
/// deterministic index order. Repeated calls on the same graph produce the
/// same result.
pub fn cluster_graph(graph: &LinkGraph) -> Vec<Cluster> {
    let node_ids: Vec<NodeId> = graph.node_ids();
    let n = node_ids.len();
    if n == 0 {
        return Vec::new();
    }

    // Index lookup: node id -> array index.
    let mut index: Vec<(NodeId, usize)> = node_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    index.sort_by_key(|a| a.0.data());
    let pos: HashMap<NodeId, usize> = index.iter().map(|(id, i)| (*id, *i)).collect();

    // Undirected adjacency by index.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in graph.edges() {
        let i = match pos.get(&edge.source) {
            Some(i) => *i,
            None => continue,
        };
        let j = match pos.get(&edge.target) {
            Some(j) => *j,
            None => continue,
        };
        adj[i].push(j);
        adj[j].push(i);
    }

    // Each node starts in its own community.
    let mut labels: Vec<usize> = (0..n).collect();

    let max_iterations = 1000;
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < max_iterations {
        changed = false;
        for i in 0..n {
            let current = labels[i];
            let mut counts: HashMap<usize, usize> = HashMap::new();
            for &j in &adj[i] {
                *counts.entry(labels[j]).or_insert(0) += 1;
            }

            let best = counts
                .into_iter()
                .max_by_key(|pair| (pair.1, usize::MAX - pair.0))
                .map_or(current, |(label, _)| label);

            if best != current {
                labels[i] = best;
                changed = true;
            }
        }
        iterations += 1;
    }

    // Group nodes by community label.
    let mut groups: HashMap<usize, Vec<NodeId>> = HashMap::new();
    for (i, &id) in node_ids.iter().enumerate() {
        groups.entry(labels[i]).or_default().push(id);
    }

    let mut clusters: Vec<Cluster> = groups
        .into_iter()
        .map(|(id, mut nodes)| {
            nodes.sort_by(|a, b| {
                pos.get(a)
                    .unwrap_or(&usize::MAX)
                    .cmp(pos.get(b).unwrap_or(&usize::MAX))
            });
            Cluster { id, nodes }
        })
        .collect();

    clusters.sort_by_key(|c| c.id);
    clusters
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
        let e = g.add_node("/e.md".into(), "E".into());
        // Two tight communities: {a,b,c} and {d,e}, plus a weak bridge c-d.
        g.add_edge(a, b, 1.0);
        g.add_edge(b, c, 1.0);
        g.add_edge(c, a, 1.0);
        g.add_edge(d, e, 1.0);
        g.add_edge(c, d, 1.0);
        g
    }

    #[test]
    fn cluster_graph_returns_all_nodes() {
        let graph = make_graph();
        let clusters = cluster_graph(&graph);
        let total: usize = clusters.iter().map(super::Cluster::len).sum();
        assert_eq!(total, graph.node_count());
    }

    #[test]
    fn cluster_graph_no_duplicates() {
        let graph = make_graph();
        let clusters = cluster_graph(&graph);
        let mut all: Vec<NodeId> = clusters.iter().flat_map(|c| c.nodes.clone()).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), graph.node_count());
    }

    #[test]
    fn cluster_graph_is_deterministic() {
        let graph = make_graph();
        let c1 = cluster_graph(&graph);
        let c2 = cluster_graph(&graph);
        assert_eq!(c1, c2);
    }

    #[test]
    fn cluster_graph_empty_graph() {
        let graph = LinkGraph::new();
        let clusters = cluster_graph(&graph);
        assert!(clusters.is_empty());
    }

    #[test]
    fn cluster_graph_single_node() {
        let mut graph = LinkGraph::new();
        let id = graph.add_node("/x.md".into(), "X".into());
        let clusters = cluster_graph(&graph);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].nodes, vec![id]);
    }

    #[test]
    fn cluster_graph_no_edges_each_node_own_cluster() {
        let mut graph = LinkGraph::new();
        let a = graph.add_node("/a.md".into(), "A".into());
        let b = graph.add_node("/b.md".into(), "B".into());
        let clusters = cluster_graph(&graph);
        assert_eq!(clusters.len(), 2);
        assert!(clusters.iter().any(|c| c.nodes == vec![a]));
        assert!(clusters.iter().any(|c| c.nodes == vec![b]));
    }

    #[test]
    fn cluster_graph_len_and_is_empty() {
        let mut graph = LinkGraph::new();
        let _id = graph.add_node("/x.md".into(), "X".into());
        let clusters = cluster_graph(&graph);
        assert!(!clusters[0].is_empty());
        assert_eq!(clusters[0].len(), 1);
    }
}
