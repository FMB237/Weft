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

/// The live editor state of one pane: what it shows and where the user is.
///
/// One buffer per pane, not per tab: tabs keep their unsaved text, and a pane
/// only holds the tab it is showing. Hints (`cursor`, `scroll`, `selection`,
/// `scroll_to`) are one-shot requests applied on the next frame and cleared.
#[derive(Debug, Default)]
pub struct EditorBuffer {
    /// Vault-relative path of the visible note.
    pub document: Option<PathBuf>,
    /// Live text, including unsaved edits.
    pub content: String,
    /// Whether `content` differs from what is on disk.
    pub dirty: bool,
    /// Caret to place on the next frame.
    pub cursor: Option<usize>,
    /// Scroll offset to apply on the next frame.
    pub scroll: Option<f32>,
    /// Selection to place on the next frame, in character indices.
    pub selection: Option<(usize, usize)>,
    /// Character offset to bring to the top of the editor on the next frame.
    pub scroll_to: Option<usize>,
}

impl EditorBuffer {
    /// Forget the visible note and drop every hint.
    pub fn reset(&mut self) {
        self.document = None;
        self.content.clear();
        self.dirty = false;
        self.cursor = None;
        self.scroll = None;
        self.selection = None;
        self.scroll_to = None;
    }
}

#[derive(Debug)]
pub struct MarkdownEditor {
    highlighter: SyntaxHighlighter,
}

impl MarkdownEditor {
    /// Stable id of a pane's text edit for `path`, so callers can focus it.
    pub fn text_id(pane: usize, path: &std::path::Path) -> Id {
        Id::new("markdown_editor").with(pane).with(path)
    }

    pub fn new() -> Self {
        Self {
            highlighter: SyntaxHighlighter::new(),
        }
    }

    /// Render the editor for the buffer's visible note.
    ///
    /// The buffer's hints are one-shot: when set they place the caret, scroll
    /// position, or selection for this frame and are cleared, so a tab switch,
    /// a back/forward jump, or a click in the outline lands where the user
    /// asked.
    pub fn show(
        &mut self,
        pane: usize,
        ui: &mut Ui,
        buffer: &mut EditorBuffer,
        _ctx: &egui::Context,
        font_size: f32,
    ) -> EditorMetrics {
        let Some(path) = buffer.document.clone() else {
            ui.label("No document open.");
            ui.separator();
            ui.label("Select a file from the sidebar to start editing.");
            return EditorMetrics::default();
        };
        let text_id = Self::text_id(pane, &path);

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

        if let Some(index) = buffer.cursor.take() {
            let mut state = TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
            let range = CCursorRange::one(CCursor::new(index));
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), text_id);
        }
        if let Some((start, end)) = buffer.selection.take() {
            let mut state = TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
            let range = CCursorRange::two(CCursor::new(start), CCursor::new(end));
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), text_id);
        }

        let mut area = egui::ScrollArea::vertical()
            .id_salt(text_id.with("scroll"))
            .auto_shrink([false; 2]);
        if let Some(offset) = buffer.scroll.take() {
            area = area.vertical_scroll_offset(offset.max(0.0));
        }

        let output = area.show(ui, |ui| {
            let text_output = TextEdit::multiline(&mut buffer.content)
                .id(text_id)
                .font(FontId::proportional(font_size))
                .desired_width(f32::INFINITY)
                .desired_rows(1)
                .layouter(&mut layouter)
                .show(ui);
            if let Some(target) = buffer.scroll_to.take()
                && let Some(row) = row_for_char(&text_output.galley, target)
            {
                // Scrolling the row to the top is what an outline click means;
                // egui only scrolls to the caret while the user is typing.
                let rect = egui::Rect::from_min_size(
                    egui::pos2(
                        text_output.galley_pos.x,
                        text_output.galley_pos.y + row.rect.top(),
                    ),
                    egui::vec2(1.0, row.rect.height()),
                );
                ui.scroll_to_rect(rect, Some(egui::Align::TOP));
            }
            text_output
        });

        if output.inner.response.changed() {
            buffer.dirty = true;
        }

        if buffer.dirty {
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

/// The laid-out row that contains character offset `target`.
fn row_for_char(galley: &egui::Galley, target: usize) -> Option<&egui::epaint::text::Row> {
    let mut start = 0;
    for row in &galley.rows {
        // A newline is not painted as a glyph but still counts as a character.
        let length = row.glyphs.len() + usize::from(row.ends_with_newline);
        if target < start + length {
            return Some(row);
        }
        start += length;
    }
    galley.rows.last()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_editor_default() {
        let _editor = MarkdownEditor::new();
    }
}
