//! Editor split panel.
//!
//! Central panel that splits between the markdown editor widget and the
//! preview pane. Each pane of the workspace owns one of these, so the panel
//! takes a pane index for its egui ids.

use crate::panels::preview::{PreviewLink, PreviewPanel};
use crate::theme::{palette, space};
use crate::widgets::chrome::{badge, empty_state, section_header};
use crate::widgets::{EditorBuffer, EditorMetrics, MarkdownEditor};
use egui::{RichText, Ui};
use weft_core::workspace::TabMode;

#[derive(Debug)]
pub struct EditorSplitPanel {
    editor: MarkdownEditor,
    preview: PreviewPanel,
    /// Split ratio between editor and preview (0.0 = all editor, 1.0 = all preview).
    pub split_ratio: f32,
    /// Current view mode for this pane.
    pub mode: TabMode,
}

impl EditorSplitPanel {
    pub fn new() -> Self {
        Self {
            editor: MarkdownEditor::new(),
            preview: PreviewPanel::new(),
            split_ratio: 0.5,
            mode: TabMode::Split,
        }
    }

    /// Show one pane: its header, the preview, and the editable source.
    ///
    /// Returns the caret and scroll to remember, and the wikilink the user
    /// clicked in the preview (if any) for the app to open.
    #[allow(
        clippy::too_many_arguments,
        reason = "show function has many parameters for editor setup"
    )]
    pub fn show(
        &mut self,
        pane: usize,
        ui: &mut Ui,
        buffer: &mut EditorBuffer,
        ctx: &egui::Context,
        font_size: f32,
        breadcrumbs: &[String],
        vault: Option<&weft_core::vault::Vault>,
    ) -> (EditorMetrics, Option<PreviewLink>) {
        let palette = palette(ui.ctx());
        let Some(document) = buffer.document.clone() else {
            empty_state(
                ui,
                "No document open",
                "Pick a note in the sidebar or create one with Ctrl+N.",
            );
            return (EditorMetrics::default(), None);
        };

        ui.horizontal(|ui| {
            for (index, crumb) in breadcrumbs.iter().enumerate() {
                let last = index + 1 == breadcrumbs.len();
                if index > 0 {
                    ui.label(RichText::new("/").size(12.0).color(palette.text_muted));
                }
                let text = RichText::new(crumb).size(if last { 15.0 } else { 12.0 });
                if last {
                    ui.label(text.strong());
                } else {
                    ui.label(text.color(palette.text_muted));
                }
            }
            if buffer.dirty {
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

        let mut link = None;
        let mut metrics = EditorMetrics::default();

        match self.mode {
            TabMode::Source => {
                // Source only - just show the editor
                section_header(ui, "Source");
                metrics = self.editor.show(pane, ui, buffer, ctx, font_size, vault);
            }
            TabMode::Preview => {
                // Preview only - just show the preview
                section_header(ui, "Preview");
                link = self
                    .preview
                    .show(ui, &Some(document.clone()), &buffer.content);
            }
            TabMode::Split => {
                // Split view - show both editor and preview
                let total_width = ui.available_width().max(1.0);
                let min_pane = (total_width * 0.2).clamp(80.0, 200.0);
                let max_preview = (total_width - min_pane).max(min_pane);
                let split_ratio = self.split_ratio.clamp(0.0, 1.0);
                let preview_width =
                    (total_width * (1.0 - split_ratio)).clamp(min_pane, max_preview);

                // egui remembers a panel's absolute width. A width remembered from a
                // wider layout (before a split opened, or after the window grew)
                // would swallow the editor, so drop it whenever it no longer matches
                // the ratio and let `default_width` place the divider again.
                let split_id = egui::Id::new(("editor_preview_split", pane));
                if let Some(state) = egui::containers::panel::PanelState::load(ui.ctx(), split_id)
                    && (state.size().x - preview_width).abs() > 1.0
                {
                    ui.ctx().data_mut(|data| {
                        data.remove::<egui::containers::panel::PanelState>(split_id);
                    });
                }

                // Draggable divider: the preview pane owns the right side, and the
                // editor fills the remaining space.
                let response = egui::SidePanel::right(split_id)
                    .resizable(true)
                    .default_width(preview_width)
                    .width_range(min_pane..=max_preview)
                    .show_inside(ui, |ui| {
                        section_header(ui, "Preview");
                        link = self
                            .preview
                            .show(ui, &Some(document.clone()), &buffer.content);
                    });

                let actual_preview_width = response.response.rect.width();
                self.split_ratio = (1.0 - actual_preview_width / total_width).clamp(0.0, 1.0);

                section_header(ui, "Source");
                metrics = self.editor.show(pane, ui, buffer, ctx, font_size, vault);
            }
        }

        (metrics, link)
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
