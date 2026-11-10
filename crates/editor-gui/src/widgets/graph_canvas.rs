//! Custom graph rendering widget using `egui::Painter`.
//!
//! Renders a force-directed link graph with pan/zoom, node selection,
//! drag-to-reposition, degree-sized nodes, edge arrows, hover tooltips, and
//! visually distinct placeholders for unresolved wikilinks. Node positions are
//! persisted via [`LayoutStore`].

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::event_bus::{Event, EventBus};
use editor_core::clustering::cluster_graph;
use editor_core::graph::{GraphNode, LinkGraph, NodeId};
use editor_core::layout::{LayoutParams, layout_graph};
use editor_core::layout_store::LayoutStore;
use egui::{Align2, Color32, FontFamily, FontId, Id, Pos2, Response, Sense, Shape, Stroke, Vec2};

/// Base radius of a node circle.
const NODE_RADIUS: f32 = 13.0;
/// Extra radius added per connection (square root of the degree).
const DEGREE_SCALE: f32 = 3.0;
/// Largest node radius, so hubs do not swallow the canvas.
const MAX_NODE_RADIUS: f32 = 30.0;
/// Arrowhead size in screen pixels.
const ARROW_LENGTH: f32 = 8.0;
const OPEN_DOC_STROKE_WIDTH: f32 = 3.0;
/// Maximum number of characters shown in a node label.
const LABEL_MAX_CHARS: usize = 22;
/// Palette used when node colouring by cluster is enabled.
const CLUSTER_COLORS: [Color32; 8] = [
    Color32::from_rgb(64, 132, 196),
    Color32::from_rgb(214, 122, 74),
    Color32::from_rgb(98, 172, 120),
    Color32::from_rgb(176, 118, 196),
    Color32::from_rgb(206, 178, 74),
    Color32::from_rgb(76, 178, 184),
    Color32::from_rgb(196, 98, 132),
    Color32::from_rgb(136, 148, 206),
];

/// Theme-resolved colors for one graph render pass.
#[derive(Debug, Clone, Copy)]
struct GraphColors {
    edge: Stroke,
    node_stroke: Stroke,
    node: Color32,
    placeholder: Color32,
    selected: Color32,
    text: Color32,
    placeholder_text: Color32,
    open_doc: Color32,
}

impl GraphColors {
    fn from_palette(palette: crate::theme::Palette) -> Self {
        Self {
            edge: Stroke::new(1.0, palette.graph_edge),
            node_stroke: Stroke::new(1.5, palette.text_muted.gamma_multiply(0.7)),
            node: palette.graph_node,
            placeholder: palette.graph_placeholder,
            selected: palette.warning,
            text: palette.text,
            placeholder_text: palette.text_muted,
            open_doc: palette.success,
        }
    }
}

/// A zoomable, pannable graph canvas.
#[derive(Debug)]
pub struct GraphCanvas {
    /// Pan offset in screen space.
    pub pan: Vec2,
    /// Zoom factor (1.0 = no zoom).
    pub zoom: f32,
    /// Currently selected node, if any.
    pub selected: Option<NodeId>,
    /// Whether nodes are tinted by their detected community.
    pub color_by_cluster: bool,
    /// Cached force-directed layout positions, keyed by node id.
    layout: Option<Vec<(NodeId, Pos2)>>,
    /// Hash of node/edge identity the cached layout was computed for.
    layout_signature: u64,
    /// Cached cluster colours, keyed by node id.
    cluster_colors: Option<HashMap<NodeId, Color32>>,
    /// Hash of node/edge identity the cached colours were computed for.
    cluster_signature: u64,
    /// Whether the initial centering pan has been applied.
    centered: bool,
}

impl GraphCanvas {
    /// Create a new empty graph canvas.
    pub fn new() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            selected: None,
            color_by_cluster: true,
            layout: None,
            layout_signature: 0,
            cluster_colors: None,
            cluster_signature: 0,
            centered: false,
        }
    }

    /// Reset view state and force a new layout, e.g. when the vault changes.
    pub fn reset(&mut self) {
        let color_by_cluster = self.color_by_cluster;
        *self = Self::new();
        self.color_by_cluster = color_by_cluster;
    }

    /// Map a graph-space position to screen space.
    fn to_screen(&self, p: Pos2) -> Pos2 {
        Pos2::new(self.pan.x + p.x * self.zoom, self.pan.y + p.y * self.zoom)
    }

    /// Map a screen-space position to graph space.
    fn to_graph(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.pan.x) / self.zoom,
            (p.y - self.pan.y) / self.zoom,
        )
    }

    /// Resolve the graph-space position of a node, preferring the stored
    /// position (from [`LayoutStore`] / drags) over the layout position.
    fn node_graph_pos(node: &GraphNode, layout_store: &LayoutStore) -> Option<Pos2> {
        layout_store.position(&node.path).or(node.position)
    }

    /// Ensure the cached layout is up to date with the current graph.
    fn ensure_layout(&mut self, graph: &LinkGraph) {
        let signature = Self::graph_signature(graph);
        if self.layout.is_some() && self.layout_signature == signature {
            return;
        }
        let layout = layout_graph(graph, &LayoutParams::default());
        self.layout = Some(layout.positions);
        self.layout_signature = signature;
    }

    /// Recompute cluster colours when the graph identity changes.
    ///
    /// Isolated nodes and graphs with a single community keep the default
    /// fill; only multi-node communities get palette colours.
    fn ensure_cluster_colors(&mut self, graph: &LinkGraph) {
        let signature = Self::graph_signature(graph);
        if self.cluster_colors.is_some() && self.cluster_signature == signature {
            return;
        }

        let clusters = cluster_graph(graph);
        let mut colors = HashMap::new();
        if clusters.len() > 1 {
            let mut palette_index = 0;
            for cluster in &clusters {
                if cluster.nodes.len() < 2 {
                    continue;
                }
                let color = CLUSTER_COLORS[palette_index % CLUSTER_COLORS.len()];
                palette_index += 1;
                for node in &cluster.nodes {
                    colors.insert(*node, color);
                }
            }
        }

        self.cluster_colors = Some(colors);
        self.cluster_signature = signature;
    }

    /// Identity hash for the graph, so the layout recomputes when nodes or
    /// edges change even if the counts stay the same.
    fn graph_signature(graph: &LinkGraph) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut paths: Vec<_> = graph
            .node_ids()
            .into_iter()
            .filter_map(|id| graph.node(id).map(|node| node.path.clone()))
            .collect();
        paths.sort();
        for path in paths {
            path.hash(&mut hasher);
        }
        let mut edges: Vec<_> = graph
            .edges()
            .into_iter()
            .filter_map(|edge| {
                Some((
                    graph.node(edge.source)?.path.clone(),
                    graph.node(edge.target)?.path.clone(),
                ))
            })
            .collect();
        edges.sort();
        for edge in edges {
            edge.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Render the graph canvas into `ui` and return the interaction response.
    #[expect(
        clippy::too_many_lines,
        reason = "node, edge, label, and tooltip painting are one render pass"
    )]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        graph: &LinkGraph,
        layout_store: &LayoutStore,
        selected_path: Option<&Path>,
    ) -> Response {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter();
        let colors = GraphColors::from_palette(crate::theme::palette(ui.ctx()));

        self.ensure_layout(graph);
        self.ensure_cluster_colors(graph);

        // Center the graph in the viewport on first show.
        if !self.centered
            && graph.node_count() > 0
            && let Some(positions) = self.layout.as_ref()
        {
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for (_id, pos) in positions {
                min_x = min_x.min(pos.x);
                min_y = min_y.min(pos.y);
                max_x = max_x.max(pos.x);
                max_y = max_y.max(pos.y);
            }
            let graph_center = Pos2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
            let screen_center = rect.center();
            self.pan = screen_center - graph_center * self.zoom;
            self.centered = true;
        }

        // Build a lookup of layout positions by node id.
        let pos_map: HashMap<NodeId, Pos2> = self
            .layout
            .as_ref()
            .map(|positions| positions.iter().copied().collect())
            .unwrap_or_default();

        let degrees = Self::degrees(graph);
        let radius_of = |id: NodeId| node_radius(degrees.get(&id).copied().unwrap_or(0));

        // Draw edges first (behind nodes), with an arrowhead at the target.
        for edge in graph.edges() {
            let src_node = graph.node(edge.source);
            let tgt_node = graph.node(edge.target);
            if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
                let Some(sp) = Self::node_graph_pos(src, layout_store)
                    .or_else(|| pos_map.get(&edge.source).copied())
                else {
                    continue;
                };
                let Some(tp) = Self::node_graph_pos(tgt, layout_store)
                    .or_else(|| pos_map.get(&edge.target).copied())
                else {
                    continue;
                };
                let sp = self.to_screen(sp);
                let tp = self.to_screen(tp);

                let src_radius = radius_of(edge.source);
                let tgt_radius = radius_of(edge.target);
                let delta = tp - sp;
                let distance = delta.length();
                if distance <= src_radius + tgt_radius {
                    continue;
                }

                let direction = delta / distance;
                let start = sp + direction * src_radius;
                let end = tp - direction * tgt_radius;
                painter.line_segment([start, end], colors.edge);

                let perpendicular = Vec2::new(-direction.y, direction.x);
                let base = end - direction * ARROW_LENGTH;
                painter.add(Shape::convex_polygon(
                    vec![
                        end,
                        base + perpendicular * (ARROW_LENGTH * 0.5),
                        base - perpendicular * (ARROW_LENGTH * 0.5),
                    ],
                    colors.edge.color,
                    Stroke::NONE,
                ));
            }
        }

        let mut hovered: Option<NodeId> = None;
        let hover_position = response.hover_pos();

        // Draw nodes.
        for node_id in graph.node_ids() {
            let Some(node) = graph.node(node_id) else {
                continue;
            };
            let Some(graph_pos) =
                Self::node_graph_pos(node, layout_store).or_else(|| pos_map.get(&node_id).copied())
            else {
                continue;
            };
            let screen_pos = self.to_screen(graph_pos);
            let radius = radius_of(node_id);

            if let Some(hover_position) = hover_position
                && (screen_pos - hover_position).length() <= radius
            {
                hovered = Some(node_id);
            }

            let is_selected = self.selected == Some(node_id);
            let fill = if is_selected {
                colors.selected
            } else if !node.resolved {
                colors.placeholder
            } else if self.color_by_cluster {
                self.cluster_colors
                    .as_ref()
                    .and_then(|cluster_colors| cluster_colors.get(&node_id).copied())
                    .unwrap_or(colors.node)
            } else {
                colors.node
            };
            painter.circle_filled(screen_pos, radius, fill);
            painter.circle_stroke(screen_pos, radius, colors.node_stroke);

            // Highlight the node for the currently open document so the graph
            // stays in sync with the editor.
            if let Some(path) = selected_path
                && node.path == path
            {
                painter.circle_stroke(
                    screen_pos,
                    radius + OPEN_DOC_STROKE_WIDTH,
                    Stroke {
                        width: OPEN_DOC_STROKE_WIDTH,
                        color: colors.open_doc,
                    },
                );
            }

            // Labels sit outside the circle so they stay readable.
            let text_color = if node.resolved {
                colors.text
            } else {
                colors.placeholder_text
            };
            painter.text(
                screen_pos + Vec2::new(0.0, radius + 3.0),
                Align2::CENTER_TOP,
                node_label(node),
                FontId {
                    size: 11.0,
                    family: FontFamily::Proportional,
                },
                text_color,
            );
        }

        // Tooltip for the node under the pointer.
        if !response.dragged()
            && let Some(node_id) = hovered
            && let Some(node) = graph.node(node_id)
        {
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                Id::new("graph_node_tooltip"),
                |ui| {
                    let title = if node.title.trim().is_empty() {
                        node.path.display().to_string()
                    } else {
                        node.title.clone()
                    };
                    ui.label(title);
                    ui.label(node.path.display().to_string());
                    if !node.resolved {
                        ui.colored_label(
                            crate::theme::palette(ui.ctx()).warning,
                            "Missing note — click to create it",
                        );
                    }
                },
            );
        }

        response
    }

    /// Total (incoming + outgoing) degree per node.
    fn degrees(graph: &LinkGraph) -> HashMap<NodeId, usize> {
        let mut degrees: HashMap<NodeId, usize> = HashMap::new();
        for edge in graph.edges() {
            *degrees.entry(edge.source).or_default() += 1;
            *degrees.entry(edge.target).or_default() += 1;
        }
        degrees
    }

    /// Handle pan/zoom/select/drag input.
    pub fn handle_input(
        &mut self,
        response: &Response,
        ctx: &egui::Context,
        graph: &mut LinkGraph,
        layout_store: &mut LayoutStore,
        event_bus: &mut EventBus,
    ) -> bool {
        let mut moved_node = false;
        // Zoom with scroll wheel when hovered.
        if response.hovered() {
            let scroll = ctx.input(|i| i.raw_scroll_delta);
            if scroll.y != 0.0 {
                let zoom_factor = (-scroll.y * 0.001).exp();
                self.zoom = (self.zoom * zoom_factor).clamp(0.2, 5.0);
                // Zoom around the cursor position.
                if let Some(cursor) = response.interact_pointer_pos() {
                    let before = self.to_graph(cursor);
                    self.pan = cursor - before * self.zoom;
                }
            }
        }

        // Pan with middle or right drag.
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            self.pan += response.drag_delta();
        }

        let degrees = Self::degrees(graph);

        // Select a node on click; clicking empty space clears the selection.
        if response.clicked()
            && let Some(cursor) = response.interact_pointer_pos()
        {
            let selection = graph.node_ids().iter().copied().find_map(|id| {
                let node = graph.node(id)?;
                let pos = Self::node_graph_pos(node, layout_store)?;
                let screen = self.to_screen(pos);
                let radius = node_radius(degrees.get(&id).copied().unwrap_or(0));
                if (screen - cursor).length() <= radius {
                    Some(id)
                } else {
                    None
                }
            });
            match selection {
                Some(id) => {
                    self.selected = Some(id);
                    if let Some(node) = graph.node(id) {
                        let event = if node.resolved {
                            Event::NodeSelected(node.path.clone())
                        } else {
                            Event::CreateNote(node.path.clone())
                        };
                        event_bus.publish(event);
                    }
                }
                None => self.selected = None,
            }
        }

        // Drag the selected node with the left mouse button.
        if let Some(selected) = self.selected
            && response.dragged_by(egui::PointerButton::Primary)
        {
            let Some(node) = graph.node(selected).cloned() else {
                self.selected = None;
                return moved_node;
            };
            let screen_pos = Self::node_graph_pos(&node, layout_store).map(|p| self.to_screen(p));
            if let Some(screen_pos) = screen_pos {
                let new_screen = screen_pos + response.drag_delta();
                let new_graph = self.to_graph(new_screen);
                graph.set_node_position(selected, new_graph);
                layout_store.set_position(node.path, new_graph);
                moved_node = true;
            }
        }

        moved_node
    }
}

impl Default for GraphCanvas {
    fn default() -> Self {
        Self::new()
    }
}

/// Radius for a node with the given total degree.
fn node_radius(degree: usize) -> f32 {
    (NODE_RADIUS + (degree as f32).sqrt() * DEGREE_SCALE).min(MAX_NODE_RADIUS)
}

/// Short label for a node, falling back to the file stem.
fn node_label(node: &GraphNode) -> String {
    let raw = if node.title.trim().is_empty() {
        node.path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("untitled")
    } else {
        node.title.trim()
    };
    if raw.chars().count() <= LABEL_MAX_CHARS {
        raw.to_string()
    } else {
        let mut label: String = raw.chars().take(LABEL_MAX_CHARS - 1).collect();
        label.push('…');
        label
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::many_single_char_names,
        reason = "graph fixtures use a, b, c for nodes"
    )]

    use super::*;
    use std::path::PathBuf;

    #[test]
    fn node_radius_grows_with_degree_and_is_capped() {
        assert!(node_radius(0) < node_radius(4));
        assert!(node_radius(10_000) <= MAX_NODE_RADIUS);
    }

    #[test]
    fn graph_signature_changes_with_edges_not_just_counts() {
        let mut graph = LinkGraph::new();
        let a = graph.add_node(PathBuf::from("a.md"), "A".into());
        let b = graph.add_node(PathBuf::from("b.md"), "B".into());
        let c = graph.add_node(PathBuf::from("c.md"), "C".into());
        graph.add_edge(a, b, 1.0);
        let first = GraphCanvas::graph_signature(&graph);

        graph.remove_outgoing_edges(a);
        graph.add_edge(a, c, 1.0);
        let second = GraphCanvas::graph_signature(&graph);

        assert_ne!(first, second);
    }

    #[test]
    fn node_label_uses_stem_for_untitled_nodes() {
        let mut graph = LinkGraph::new();
        let id = graph.add_node(PathBuf::from("notes/alpha.md"), String::new());
        let node = graph.node(id).expect("node exists").clone();
        assert_eq!(node_label(&node), "alpha");
    }

    #[test]
    fn cluster_colors_group_connected_nodes_and_skip_isolated() {
        let mut graph = LinkGraph::new();
        let a = graph.add_node(PathBuf::from("a.md"), "A".into());
        let b = graph.add_node(PathBuf::from("b.md"), "B".into());
        let c = graph.add_node(PathBuf::from("c.md"), "C".into());
        let d = graph.add_node(PathBuf::from("d.md"), "D".into());
        let e = graph.add_node(PathBuf::from("e.md"), "E".into());
        let isolated = graph.add_node(PathBuf::from("solo.md"), "Solo".into());
        graph.add_edge(a, b, 1.0);
        graph.add_edge(b, c, 1.0);
        graph.add_edge(d, e, 1.0);

        let mut canvas = GraphCanvas::new();
        canvas.ensure_cluster_colors(&graph);
        let colors = canvas
            .cluster_colors
            .as_ref()
            .expect("cluster colours computed");

        assert_eq!(colors.get(&a), colors.get(&b));
        assert_eq!(colors.get(&b), colors.get(&c));
        assert_eq!(colors.get(&d), colors.get(&e));
        assert_ne!(colors.get(&a), colors.get(&d));
        assert!(!colors.contains_key(&isolated));
    }
}
