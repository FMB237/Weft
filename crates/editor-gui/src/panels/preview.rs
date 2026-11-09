//! Markdown preview panel.
//!
//! Renders the currently open document as formatted markdown.

use std::path::PathBuf;
use egui::Ui;
use editor_core::parser::markdown_to_html;

pub struct PreviewPanel;

impl PreviewPanel {
    pub fn new() -> Self {
        Self
    }

    /// Show the preview of the current document.
    pub fn show(&mut self, ui: &mut Ui, open_document: &Option<PathBuf>, document_content: &str) {
        ui.heading("Preview");

        if open_document.is_none() {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to view its preview.");
            return;
        }

        ui.heading(&format!("{} - Preview", open_document.as_ref().unwrap().file_name().unwrap().to_string_lossy()));
        ui.separator();

        // Render markdown as HTML.
        let html = markdown_to_html(document_content);
        ui.add(egui::TextEdit::multiline(&mut String::from(html))
            .id_source("preview_markdown")
            .desired_width(f32::INFINITY)
            .desired_rows(20)
            .frame(false));
    }
}

impl Default for PreviewPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_default() {
        let _panel = PreviewPanel::new();
    }
}