//! Backlinks panel.
//!
//! Lists notes that link to the currently open document using
//! [`GraphQuery::backlinks`]. Clicking a backlink opens the source note.

use crate::event_bus::{Event, EventBus};
use crate::theme::palette;
use crate::widgets::chrome::section_header;
use editor_core::graph::LinkGraph;
use editor_core::graph_queries::GraphQuery;
use egui::{RichText, Ui};
use std::path::Path;

#[derive(Debug, Default)]
pub struct BacklinksPanel;

impl BacklinksPanel {
    pub fn new() -> Self {
        Self
    }

    /// Show the backlinks for `open_document`.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        graph: &LinkGraph,
        open_document: Option<&Path>,
        event_bus: &mut EventBus,
    ) {
        let palette = palette(ui.ctx());
        section_header(ui, "Backlinks");

        let Some(path) = open_document else {
            ui.label(
                RichText::new("Open a note to see which notes link to it.")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            return;
        };

        let Some(node_id) = graph.node_by_path(path) else {
            ui.label(
                RichText::new("This note is not in the graph yet.")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            return;
        };

        let mut backlinks = GraphQuery::new(graph).backlinks(node_id);
        backlinks.sort_by(|a, b| a.0.cmp(&b.0));
        backlinks.dedup_by(|a, b| a.0 == b.0);

        if backlinks.is_empty() {
            ui.label(
                RichText::new("No other note links here yet.")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            return;
        }

        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (source, title) in backlinks {
                        let label = if title.trim().is_empty() {
                            source.display().to_string()
                        } else {
                            title
                        };
                        if ui.link(label).clicked() {
                            event_bus.publish(Event::NodeSelected(source));
                        }
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlinks_panel_default() {
        let _panel = BacklinksPanel::new();
    }
}
