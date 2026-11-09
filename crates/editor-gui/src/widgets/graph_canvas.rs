//! Custom graph rendering widget using `egui::Painter`.
//!
//! Renders a force-directed link graph with pan/zoom, node selection,
//! and drag-to-reposition. Node positions are persisted via [`LayoutStore`].

use std::collections::HashMap;
use std::path::Path;

use crate::event_bus::{Event, EventBus};
use editor_core::graph::{LinkGraph, NodeId};
use editor_core::layout::{LayoutParams, layout_graph};
use editor_core::layout_store::LayoutStore;
use egui::{Align2, Color32, FontFamily, FontId, Pos2, Response, Sense, Stroke, Vec2};

/// Radius of each node circle.
const NODE_RADIUS: f32 = 16.0;
/// Stroke for graph edges.
const EDGE_STROKE: Stroke = Stroke {
    width: 1.0,
    color: Color32::from_rgb(140, 140, 140),
};
/// Stroke for node circles.
const NODE_STROKE: Stroke = Stroke {
    width: 1.5,
    color: Color32::from_rgb(110, 110, 110),
};
/// Default fill for unselected nodes.
const NODE_FILL: Color32 = Color32::from_rgb(40, 100, 180);
/// Fill for selected nodes.
const SELECTED_FILL: Color32 = Color32::from_rgb(220, 140, 50);
/// Color for node title text.
const TEXT_COLOR: Color32 = Color32::from_gray(240);
const OPEN_DOC_STROKE_WIDTH: f32 = 3.0;
const OPEN_DOC_STROKE_COLOR: Color32 = Color32::from_rgb(255, 215, 0);

/// A zoomable, pannable graph canvas.
#[derive(Debug)]
pub struct GraphCanvas {
    /// Pan offset in screen space.
    pub pan: Vec2,
    /// Zoom factor (1.0 = no zoom).
    pub zoom: f32,
    /// Currently selected node, if any.
    pub selected: Option<NodeId>,
    /// Cached force-directed layout positions, keyed by node id.
    layout: Option<Vec<(NodeId, Pos2)>>,
    /// Number of nodes/edges the cached layout was computed for.
    layout_node_count: usize,
    layout_edge_count: usize,
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
            layout: None,
            layout_node_count: 0,
            layout_edge_count: 0,
            centered: false,
        }
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
    fn node_graph_pos(
        node: &editor_core::graph::GraphNode,
        layout_store: &LayoutStore,
    ) -> Option<Pos2> {
        layout_store.position(&node.path).or(node.position)
    }

    /// Ensure the cached layout is up to date with the current graph.
    fn ensure_layout(&mut self, graph: &LinkGraph) {
        let node_count = graph.node_count();
        let edge_count = graph.edge_count();
        if self.layout.is_some()
            && self.layout_node_count == node_count
            && self.layout_edge_count == edge_count
        {
            return;
        }
        let layout = layout_graph(graph, &LayoutParams::default());
        self.layout = Some(layout.positions);
        self.layout_node_count = node_count;
        self.layout_edge_count = edge_count;
    }

    /// Render the graph canvas into `ui` and return the interaction response.
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

        self.ensure_layout(graph);

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

        // Draw edges first (behind nodes).
        for edge in graph.edges() {
            let src_node = graph.node(edge.source);
            let tgt_node = graph.node(edge.target);
            if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
                let sp = Self::node_graph_pos(src, layout_store)
                    .unwrap_or_else(|| *pos_map.get(&edge.source).unwrap_or(&Pos2::ZERO));
                let tp = Self::node_graph_pos(tgt, layout_store)
                    .unwrap_or_else(|| *pos_map.get(&edge.target).unwrap_or(&Pos2::ZERO));
                painter.line_segment([self.to_screen(sp), self.to_screen(tp)], EDGE_STROKE);
            }
        }

        // Draw nodes.
        for node_id in graph.node_ids() {
            let Some(node) = graph.node(node_id) else {
                continue;
            };
            let screen_pos = match Self::node_graph_pos(node, layout_store) {
                Some(p) => self.to_screen(p),
                None => continue,
            };
            let is_selected = self.selected == Some(node_id);
            let fill = if is_selected {
                SELECTED_FILL
            } else {
                NODE_FILL
            };
            painter.circle_filled(screen_pos, NODE_RADIUS, fill);
            painter.circle_stroke(screen_pos, NODE_RADIUS, NODE_STROKE);
            // Highlight the node for the currently open document so the graph
            // stays in sync with the editor (Phase 6.1 cross-panel sync).
            if let Some(path) = selected_path
                && node.path == path
            {
                painter.circle_stroke(
                    screen_pos,
                    NODE_RADIUS + OPEN_DOC_STROKE_WIDTH,
                    Stroke {
                        width: OPEN_DOC_STROKE_WIDTH,
                        color: OPEN_DOC_STROKE_COLOR,
                    },
                );
            }
            painter.text(
                screen_pos,
                Align2::CENTER_CENTER,
                &node.title,
                FontId {
                    size: 12.0,
                    family: FontFamily::Proportional,
                },
                TEXT_COLOR,
            );
        }

        response
    }

    /// Handle pan/zoom/select/drag input.
    pub fn handle_input(
        &mut self,
        response: &Response,
        ctx: &egui::Context,
        graph: &mut LinkGraph,
        layout_store: &mut LayoutStore,
        event_bus: &mut EventBus,
    ) {
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

        // Select a node on click.
        if response.clicked()
            && let Some(cursor) = response.interact_pointer_pos()
        {
            let selection = graph.node_ids().iter().copied().find_map(|id| {
                let node = graph.node(id)?;
                let pos = Self::node_graph_pos(node, layout_store)?;
                let screen = self.to_screen(pos);
                if (screen - cursor).length() <= NODE_RADIUS {
                    Some(id)
                } else {
                    None
                }
            });
            if let Some(id) = selection {
                self.selected = Some(id);
                if let Some(node) = graph.node(id) {
                    event_bus.publish(Event::NodeSelected(node.path.clone()));
                }
            }
        }

        // Drag the selected node with the left mouse button.
        if let Some(selected) = self.selected
            && response.dragged_by(egui::PointerButton::Primary)
        {
            let Some(node) = graph.node(selected).cloned() else {
                self.selected = None;
                return;
            };
            let screen_pos = Self::node_graph_pos(&node, layout_store).map(|p| self.to_screen(p));
            if let Some(screen_pos) = screen_pos {
                let new_screen = screen_pos + response.drag_delta();
                let new_graph = self.to_graph(new_screen);
                graph.set_node_position(selected, new_graph);
                layout_store.set_position(node.path, new_graph);
            }
        }
    }
}

impl Default for GraphCanvas {
    fn default() -> Self {
        Self::new()
    }
}
