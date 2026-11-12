//! Editor split panel.
//!
//! Central panel that splits between the markdown editor widget and the
//! preview pane.

use std::path::PathBuf;

use crate::panels::preview::PreviewPanel;
use crate::theme::{palette, space};
use crate::widgets::MarkdownEditor;
use crate::widgets::chrome::{badge, empty_state, section_header};
use egui::{RichText, Ui};

#[derive(Debug)]
pub struct EditorSplitPanel {
    editor: MarkdownEditor,
    preview: PreviewPanel,
    /// Split ratio between editor and preview (0.0 = all editor, 1.0 = all preview).
    pub split_ratio: f32,
}

impl EditorSplitPanel {
    pub fn new() -> Self {
        Self {
            editor: MarkdownEditor::new(),
            preview: PreviewPanel::new(),
            split_ratio: 0.5,
        }
    }

    /// Show the editor split panel.
    #[expect(
        clippy::too_many_arguments,
        reason = "panel show methods take the shared document state explicitly"
    )]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        open_document: &Option<PathBuf>,
        document_content: &mut String,
        dirty: &mut bool,
        ctx: &egui::Context,
        split_ratio: &mut f32,
        font_size: f32,
    ) {
        let palette = palette(ui.ctx());
        if open_document.is_none() {
            empty_state(
                ui,
                "No document open",
                "Pick a note in the sidebar or create one with Ctrl+N.",
            );
            return;
        }

        ui.horizontal(|ui| {
            let name = open_document
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled");
            ui.label(RichText::new(name).strong().size(15.0));
            if *dirty {
                badge(ui, "Unsaved", palette.warning);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Markdown")
                        .size(11.0)
                        .color(palette.text_muted),
                );
            });
        });
        ui.add_space(space::XS);
        ui.separator();

        let total_width = ui.available_width().max(1.0);
        let min_pane = (total_width * 0.2).clamp(80.0, 200.0);
        let max_preview = (total_width - min_pane).max(min_pane);
        let preview_width = (total_width * (1.0 - *split_ratio)).clamp(min_pane, max_preview);

        // Draggable divider: the preview pane owns the right side, and the
        // editor fills the remaining space.
        let response = egui::SidePanel::right("editor_preview_split")
            .resizable(true)
            .default_width(preview_width)
            .width_range(min_pane..=max_preview)
            .show_inside(ui, |ui| {
                section_header(ui, "Preview");
                self.preview
                    .show(ui, open_document, document_content.as_str());
            });

        let actual_preview_width = response.response.rect.width();
        *split_ratio = (1.0 - actual_preview_width / total_width).clamp(0.0, 1.0);
        self.split_ratio = *split_ratio;

        section_header(ui, "Source");
        self.editor
            .show(ui, open_document, document_content, dirty, ctx, font_size);
    }

    /// Get the current split ratio.
    pub fn split_ratio(&self) -> f32 {
        self.split_ratio
    }
}

impl Default for EditorSplitPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_split_default() {
        let _panel = EditorSplitPanel::new();
    }
}
