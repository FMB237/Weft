//! Markdown preview panel.
//!
//! Renders the currently open document as formatted markdown.

use editor_core::parser::markdown_to_html;
use egui::Ui;
use std::path::PathBuf;

#[derive(Debug)]
pub struct PreviewPanel;

impl PreviewPanel {
    pub fn new() -> Self {
        Self
    }

    /// Show the preview of the current document.
    pub fn show(&mut self, ui: &mut Ui, open_document: &Option<PathBuf>, document_content: &str) {
        ui.heading("Preview");

        let Some(path) = open_document.as_ref() else {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to view its preview.");
            return;
        };

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled");
        ui.heading(format!("{name} - Preview"));
        ui.separator();

        // Render markdown as HTML.
        let mut html = markdown_to_html(document_content);
        ui.add(
            egui::TextEdit::multiline(&mut html)
                .id_salt("preview_markdown")
                .desired_width(f32::INFINITY)
                .desired_rows(20)
                .frame(false),
        );
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
