//! Markdown editor widget.
//!
//! Renders the current document as an editable text area with syntax
//! highlighting applied through `TextEdit::layouter`.

use crate::widgets::syntax_highlight::SyntaxHighlighter;
use egui::text::LayoutJob;
use egui::{FontId, Id, TextEdit, TextFormat, Ui};
use std::path::PathBuf;

#[derive(Debug)]
pub struct MarkdownEditor {
    /// Stable egui id for the text edit widget, so state persists across frames.
    id: Id,
    highlighter: SyntaxHighlighter,
}

impl MarkdownEditor {
    pub fn new() -> Self {
        Self {
            id: Id::new("markdown_editor"),
            highlighter: SyntaxHighlighter::new(),
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
        font_size: f32,
    ) {
        let Some(path) = open_document else {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to start editing.");
            return;
        };

        let base_format = TextFormat {
            font_id: FontId::proportional(font_size),
            color: ui.visuals().text_color(),
            ..Default::default()
        };
        let highlighter = &self.highlighter;
        let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
            let mut job = LayoutJob::default();
            job.wrap.max_width = wrap_width;
            let scale = font_size / 14.0;

            for (line_index, line) in text.split('\n').enumerate() {
                if line_index > 0 {
                    job.append("\n", 0.0, base_format.clone());
                }
                for (format, span) in highlighter.highlight(line) {
                    let mut format = format;
                    if format.color == TextFormat::default().color {
                        format.color = base_format.color;
                    }
                    format.font_id.size *= scale;
                    job.append(span, 0.0, format);
                }
            }

            ui.fonts(|fonts| fonts.layout_job(job))
        };

        let response = TextEdit::multiline(document_content)
            .id(self.id.with(path))
            .font(FontId::proportional(font_size))
            .desired_width(f32::INFINITY)
            .desired_rows(20)
            .layouter(&mut layouter)
            .show(ui);

        if response.response.changed() {
            *dirty = true;
        }

        if *dirty {
            ui.colored_label(egui::Color32::from_rgb(220, 140, 50), "unsaved changes");
        }
    }

    /// Apply syntax highlighting to a single line (used by tests and callers
    /// that need the raw spans).
    pub fn highlight_line<'a>(
        &self,
        line: &'a str,
        highlighter: &'a SyntaxHighlighter,
    ) -> Vec<(egui::TextFormat, &'a str)> {
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
