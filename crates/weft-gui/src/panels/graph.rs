//! Graph visualization panel.
//!
//! Renders the link graph via [`GraphCanvas`] and persists node positions
//! back to [`LayoutStore`] so they survive between sessions.

use crate::event_bus::EventBus;
use crate::theme::palette;
use crate::widgets::GraphCanvas;
use crate::widgets::chrome::{badge, section_header};
use egui::{RichText, Ui};
use weft_core::graph::LinkGraph;
use weft_core::layout_store::LayoutStore;

#[derive(Debug)]
pub struct GraphPanel {
    canvas: GraphCanvas,
}

impl GraphPanel {
    pub fn new() -> Self {
        Self {
            canvas: GraphCanvas::new(),
        }
    }

    /// Show the graph visualization panel.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        graph: &mut LinkGraph,
        layout_store: &mut LayoutStore,
        event_bus: &mut EventBus,
        selected_path: Option<&std::path::Path>,
    ) -> bool {
        let palette = palette(ui.ctx());
        section_header(ui, "Graph");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.canvas.color_by_cluster, "Colour by cluster")
                .on_hover_text("Tint nodes by detected community");
            ui.separator();
            badge(ui, &format!("{} nodes", graph.node_count()), palette.accent);
            badge(
                ui,
                &format!("{} links", graph.edge_count()),
                palette.text_muted,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("drag to pan · scroll to zoom")
                        .size(11.0)
                        .color(palette.text_muted),
                );
            });
        });
        ui.add_space(2.0);
        let response = self.canvas.show(ui, graph, layout_store, selected_path);
        self.canvas
            .handle_input(&response, ui.ctx(), graph, layout_store, event_bus)
    }
    /// Reset pan/zoom/selection and recompute the layout (e.g. new vault).
    pub fn reset(&mut self) {
        self.canvas.reset();
    }
}

impl Default for GraphPanel {
    fn default() -> Self {
        Self::new()
    }
}
