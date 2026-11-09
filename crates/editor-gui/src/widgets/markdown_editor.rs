//! Markdown editor widget.
//!
//! Renders the current document as an editable text area with syntax
//! highlighting applied via the [`SyntaxHighlighter`].

use crate::widgets::syntax_highlight::SyntaxHighlighter;
use egui::{Id, TextEdit, Ui};
use std::path::PathBuf;

pub struct MarkdownEditor {
    /// Stable egui id for the text edit widget, so state persists across frames.
    id: Id,
}

impl MarkdownEditor {
    pub fn new() -> Self {
        Self {
            id: Id::new("markdown_editor"),
        }
    }

    /// Render the editor for the currently open document.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        open_document: &Option<PathBuf>,
        document_content: &mut String,
        dirty: &mut bool,
        _ctx: &egui::Context,
    ) {
        if open_document.is_none() {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to start editing.");
            return;
        }

        let response = TextEdit::multiline(document_content)
            .id(self.id)
            .desired_width(f32::INFINITY)
            .desired_rows(20)
            .show(ui);

        if response.response.changed() {
            *dirty = true;
        }

        if *dirty {
            ui.colored_label(egui::Color32::from_rgb(220, 140, 50), "unsaved changes");
        }
    }

    /// Apply syntax highlighting to a single line (placeholder implementation).
    pub fn highlight_line<'a>(&self, line: &'a str, highlighter: &'a SyntaxHighlighter) -> Vec<(egui::TextFormat, &'a str)> {
        highlighter.highlight(line)
    }
}

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_editor_default() {
        let _editor = MarkdownEditor::new();
    }
}