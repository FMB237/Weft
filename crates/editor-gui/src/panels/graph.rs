//! Graph visualization panel.
//!
//! Renders the link graph via [`GraphCanvas`] and persists node positions
//! back to [`LayoutStore`] so they survive between sessions.

use crate::event_bus::EventBus;
use crate::widgets::GraphCanvas;
use editor_core::graph::LinkGraph;
use editor_core::layout_store::LayoutStore;
use egui::Ui;

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
        ui.heading("Graph View");
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
