//! Markdown editor widget.
//!
//! Renders the current document as an editable text area with syntax
//! highlighting applied through `TextEdit::layouter`.

use crate::widgets::syntax_highlight::SyntaxHighlighter;
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{FontId, Id, TextEdit, TextFormat, Ui};
use std::path::PathBuf;

/// Where the caret and scroll ended up after a frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EditorMetrics {
    /// Character offset of the caret.
    pub cursor: Option<usize>,
    /// Vertical scroll offset of the editor, in points.
    pub scroll: f32,
}

impl EditorMetrics {
    /// Scroll as the whole-point value stored in a tab or nav entry.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to a small non-negative range before the cast"
    )]
    pub fn scroll_points(&self) -> u32 {
        self.scroll.clamp(0.0, 1_000_000.0).round() as u32
    }
}

#[derive(Debug)]
pub struct MarkdownEditor {
    highlighter: SyntaxHighlighter,
}

impl MarkdownEditor {
    /// Stable id of the text edit for `path`, so callers can move focus to it.
    pub fn text_id(path: &std::path::Path) -> Id {
        Id::new("markdown_editor").with(path)
    }

    pub fn new() -> Self {
        Self {
            highlighter: SyntaxHighlighter::new(),
        }
    }

    /// Render the editor for the currently open document.
    ///
    /// `cursor` and `scroll` are one-shot restore hints: when set, they place
    /// the caret or scroll position for this frame and are cleared, so a tab
    /// switch or a back/forward jump lands where the user left off.
    #[expect(
        clippy::too_many_arguments,
        reason = "the editor takes the shared document state explicitly"
    )]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        open_document: &Option<PathBuf>,
        document_content: &mut String,
        dirty: &mut bool,
        _ctx: &egui::Context,
        font_size: f32,
        cursor: &mut Option<usize>,
        scroll: &mut Option<f32>,
        selection: &mut Option<(usize, usize)>,
    ) -> EditorMetrics {
        let Some(path) = open_document else {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to start editing.");
            return EditorMetrics::default();
        };
        let text_id = Self::text_id(path);

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

        if let Some(index) = cursor.take() {
            let mut state = TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
            let range = CCursorRange::one(CCursor::new(index));
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), text_id);
        }
        if let Some((start, end)) = selection.take() {
            let mut state = TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
            let range = CCursorRange::two(CCursor::new(start), CCursor::new(end));
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), text_id);
        }

        let mut area = egui::ScrollArea::vertical()
            .id_salt(text_id.with("scroll"))
            .auto_shrink([false; 2]);
        if let Some(offset) = scroll.take() {
            area = area.vertical_scroll_offset(offset.max(0.0));
        }

        let output = area.show(ui, |ui| {
            TextEdit::multiline(document_content)
                .id(text_id)
                .font(FontId::proportional(font_size))
                .desired_width(f32::INFINITY)
                .desired_rows(1)
                .layouter(&mut layouter)
                .show(ui)
        });

        if output.inner.response.changed() {
            *dirty = true;
        }

        if *dirty {
            ui.colored_label(egui::Color32::from_rgb(220, 140, 50), "unsaved changes");
        }

        EditorMetrics {
            cursor: output
                .inner
                .state
                .cursor
                .char_range()
                .map(|range| range.primary.index),
            scroll: output.state.offset.y,
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
