//! Graph visualization panel.
//!
//! Renders the link graph via [`GraphCanvas`] and persists node positions
//! back to [`LayoutStore`] so they survive between sessions.

use crate::event_bus::EventBus;
use crate::widgets::GraphCanvas;
use egui::Ui;
use editor_core::graph::LinkGraph;
use editor_core::layout_store::LayoutStore;

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
        graph: &LinkGraph,
        layout_store: &mut LayoutStore,
        event_bus: &mut EventBus,
        selected_path: Option<&std::path::Path>,
    ) {
        ui.heading("Graph View");
        let mut graph_mut = graph.clone();
        let response = self.canvas.show(ui, &graph_mut, layout_store, selected_path);
        self.canvas
            .handle_input(&response, ui.ctx(), &mut graph_mut, layout_store, event_bus);
    }
}

impl Default for GraphPanel {
    fn default() -> Self {
        Self::new()
    }
}
