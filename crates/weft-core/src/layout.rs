//! Force-directed layout (Fruchterman-Reingold) for the link graph.
//!
//! Positions are computed on an egui `Pos2` grid. The layout is deterministic:
//! starting positions are jittered deterministically per iteration so repeated
//! layouts converge to the same result.

use crate::graph::{LinkGraph, NodeId};
use egui::Pos2;

/// Parameters controlling the force-directed layout.
#[derive(Debug, Clone, Copy)]
pub struct LayoutParams {
    /// Ideal edge length between connected nodes.
    pub ideal_length: f32,
    /// Strength of repulsive force between all node pairs.
    pub repulsion: f32,
    /// Strength of attractive force along edges.
    pub attraction: f32,
    /// Maximum displacement per iteration (cooling).
    pub max_displacement: f32,
    /// Area to confine nodes within (square side length).
    pub area: f32,
    /// Damping factor applied to displacement each iteration.
    pub damping: f32,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            ideal_length: 120.0,
            repulsion: 40000.0,
            attraction: 0.08,
            max_displacement: 50.0,
            area: 1000.0,
            damping: 0.95,
        }
    }
}

/// Layout result: positions keyed by node id, plus the overall bounding box.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Position per node id.
    pub positions: Vec<(NodeId, Pos2)>,
    /// Bounding box of all laid-out nodes.
    pub bounds: Rect,
}

#[derive(Debug, Clone)]
pub struct Rect {
    pub min: Pos2,
    pub max: Pos2,
}

impl Rect {
    pub fn empty() -> Self {
        Self {
            min: Pos2::new(f32::INFINITY, f32::INFINITY),
            max: Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    pub fn grow(&mut self, p: Pos2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn center(&self) -> Pos2 {
        Pos2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }
}

/// Run the Fruchterman-Reingold force-directed layout on the graph.
///
/// Returns positions for every node, plus a bounding box. Positions are in
/// arbitrary units scaled by `params.area`.
///
/// # Panics
///
/// Panics if an edge references a node that does not exist in the graph.
pub fn layout_graph(graph: &LinkGraph, params: &LayoutParams) -> Layout {
    let node_ids: Vec<NodeId> = graph.node_ids();
    let n = node_ids.len();
    if n == 0 {
        return Layout {
            positions: Vec::new(),
            bounds: Rect::empty(),
        };
    }

    // Index lookup: node id -> array index.
    let mut index: Vec<(NodeId, usize)> = node_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    index.sort_by_key(|&(id, _)| id);

    // Start from deterministic jittered positions in a circle.
    let mut positions: Vec<Pos2> = vec![Pos2::ZERO; n];
    let angle_step = 2.0 * std::f32::consts::PI / (n as f32);
    let radius = (n as f32).sqrt() * params.ideal_length;
    for (i, _id) in node_ids.iter().enumerate() {
        let angle = i as f32 * angle_step;
        positions[i] = Pos2::new(
            angle.cos() * radius + i as f32 * 0.7,
            angle.sin() * radius + i as f32 * 0.7,
        );
    }

    // Displacement accumulator per node.
    let mut disp: Vec<Pos2> = vec![Pos2::ZERO; n];

    let mut temperature = params.max_displacement;

    // Deterministic iteration count: more nodes need more iterations.
    let iterations = 50 + (n * 5).min(200);

    for _ in 0..iterations {
        // Reset displacements.
        disp.fill(Pos2::ZERO);

        // Repulsive forces between all pairs.
        for i in 0..n {
            for j in (i + 1)..n {
                let delta = positions[i] - positions[j];
                let dist_len = delta.length().max(0.01);
                let force = params.repulsion / dist_len;
                let direction = delta / dist_len;
                let displacement = direction * force;
                disp[i] += displacement;
                disp[j] -= displacement;
            }
        }

        // Attractive forces along edges.
        for edge in graph.edges() {
            let Some(i) = index_position(&index, edge.source) else {
                continue;
            };
            let Some(j) = index_position(&index, edge.target) else {
                continue;
            };
            let delta = positions[j] - positions[i];
            let dist_len = delta.length().max(0.01);
            let force = (dist_len * dist_len) / params.ideal_length * params.attraction;
            let direction = delta / dist_len;
            let displacement = direction * force;
            disp[i] += displacement;
            disp[j] -= displacement;
        }

        // Apply displacements, limited by temperature.
        for i in 0..n {
            let d = disp[i];
            let move_dist = (d.x * d.x + d.y * d.y).sqrt();
            if move_dist > 0.0 {
                let limited = d * (move_dist.min(temperature) / move_dist);
                positions[i] = Pos2::new(positions[i].x + limited.x, positions[i].y + limited.y);
            }
        }

        temperature *= params.damping;
    }

    // Confine nodes within the area, centered around origin.
    let mut bounds = Rect::empty();
    for p in &positions {
        bounds.grow(*p);
    }
    let center = bounds.center();
    let mut positions_out: Vec<(NodeId, Pos2)> = Vec::with_capacity(n);
    for (i, id) in node_ids.iter().enumerate() {
        let mut p = positions[i] - center;
        // Clamp into [-area/2, area/2].
        p.x = p.x.clamp(-params.area / 2.0, params.area / 2.0);
        p.y = p.y.clamp(-params.area / 2.0, params.area / 2.0);
        positions_out.push((*id, Pos2::new(p.x, p.y)));
    }

    let mut final_bounds = Rect::empty();
    for (_, p) in &positions_out {
        final_bounds.grow(*p);
    }

    Layout {
        positions: positions_out,
        bounds: final_bounds,
    }
}

fn index_position(index: &[(NodeId, usize)], id: NodeId) -> Option<usize> {
    index
        .binary_search_by(|&(key, _)| key.cmp(&id))
        .ok()
        .map(|idx| index[idx].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> LinkGraph {
        let mut g = LinkGraph::new();
        let a = g.add_node("/a.md".into(), "A".into());
        let b = g.add_node("/b.md".into(), "B".into());
        let c = g.add_node("/c.md".into(), "C".into());
        g.add_edge(a, b, 1.0);
        g.add_edge(b, c, 1.0);
        g.add_edge(c, a, 1.0);
        g
    }

    #[test]
    fn layout_returns_position_for_every_node() {
        let graph = make_graph();
        let layout = layout_graph(&graph, &LayoutParams::default());
        assert_eq!(layout.positions.len(), graph.node_count());
        let ids: Vec<NodeId> = layout.positions.iter().map(|(id, _)| *id).collect();
        for id in graph.node_ids() {
            assert!(ids.contains(&id));
        }
    }

    #[test]
    fn layout_is_deterministic() {
        let graph = make_graph();
        let p = LayoutParams::default();
        let l1 = layout_graph(&graph, &p);
        let l2 = layout_graph(&graph, &p);
        assert_eq!(l1.positions, l2.positions);
    }

    #[test]
    fn empty_graph_returns_empty_layout() {
        let graph = LinkGraph::new();
        let layout = layout_graph(&graph, &LayoutParams::default());
        assert!(layout.positions.is_empty());
    }

    #[test]
    fn nodes_stay_within_area() {
        let graph = make_graph();
        let params = LayoutParams {
            area: 400.0,
            ..LayoutParams::default()
        };
        let layout = layout_graph(&graph, &params);
        for (_, p) in &layout.positions {
            assert!(p.x >= -200.0 && p.x <= 200.0);
            assert!(p.y >= -200.0 && p.y <= 200.0);
        }
    }

    #[test]
    fn set_node_position_works() {
        let mut graph = LinkGraph::new();
        let id = graph.add_node("/x.md".into(), "X".into());
        assert!(graph.set_node_position(id, Pos2::new(10.0, 20.0)));
        assert_eq!(
            graph.node(id).unwrap().position,
            Some(Pos2::new(10.0, 20.0))
        );
        // Non-existent id (removed from slotmap) returns false.
        graph.remove_node(id);
        assert!(!graph.set_node_position(id, Pos2::ZERO));
    }
}
