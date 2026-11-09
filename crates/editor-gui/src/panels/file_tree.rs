//! File explorer panel with virtual scrolling.
//!
//! Lists the files in the currently open vault and allows opening them for editing.
//! Only the visible subset of rows is rendered each frame (virtual scrolling), so
//! large vaults stay performant.

use crate::event_bus::Event;
use crate::event_bus::EventBus;
use editor_core::vault::Vault;
use egui::{Id, ScrollArea, Ui, Vec2};

/// Height of a single file-tree row in points.
const ITEM_HEIGHT: f32 = 20.0;

#[derive(Debug)]
pub struct FileTreePanel {
    selected: Option<String>,
    /// Persisted vertical scroll offset between frames.
    scroll_offset: f32,
}

impl FileTreePanel {
    pub fn new() -> Self {
        Self {
            selected: None,
            scroll_offset: 0.0,
        }
    }

    /// Show the file tree, populated from the open vault.
    ///
    /// Uses virtual scrolling: only rows overlapping the visible viewport are
    /// rendered, while spacer space is reserved for the off-screen rows above
    /// and below so the scroll bar and offset remain correct.
    ///
    /// Clicking a file publishes an [`Event::NodeSelected`] on the bus so
    /// any panel (editor, graph, preview) can react to the selection.
    pub fn show(&mut self, ui: &mut Ui, vault: Option<&mut Vault>, event_bus: &mut EventBus) {
        ui.heading("Files");

        let Some(vault) = vault else {
            ui.label("No vault open.");
            ui.separator();
            ui.label("Use File > Open Vault to open a vault.");
            return;
        };

        let Ok(files) = vault.list_files() else {
            ui.label("Failed to list vault files.");
            return;
        };

        if files.is_empty() {
            ui.label("(no files)");
            ui.separator();
            ui.label("Create a new file to get started.");
            return;
        }

        let id = Id::new("file_tree_virtual_scroll");
        let prev_offset = self.scroll_offset;
        let viewport_height = ui.available_height();
        let total_height = ITEM_HEIGHT * files.len() as f32;

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let visible_start = (prev_offset / ITEM_HEIGHT).floor() as usize;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let mut visible_end = ((prev_offset + viewport_height) / ITEM_HEIGHT).ceil() as usize + 1;
        visible_end = visible_end.min(files.len()).max(visible_start);

        let output = ScrollArea::vertical()
            .id_salt(id)
            .min_scrolled_height(total_height)
            .show(ui, |ui| {
                // Reserve space for rows above the visible window.
                if visible_start > 0 {
                    ui.add_space(visible_start as f32 * ITEM_HEIGHT);
                }

                let inner_width = ui.available_width();
                for file in files.iter().take(visible_end).skip(visible_start) {
                    let name = file.to_string_lossy();
                    let selected = self.selected.as_deref() == Some(name.as_ref());

                    // Fixed-height row so the virtual scroll math stays consistent.
                    ui.allocate_ui(Vec2::new(inner_width, ITEM_HEIGHT), |ui| {
                        if ui.selectable_label(selected, &*name).clicked() {
                            self.selected = Some(name.to_string());
                            event_bus.publish(Event::NodeSelected(file.clone()));
                        }
                    });
                }

                // Reserve space for rows below the visible window.
                let below = files.len().saturating_sub(visible_end);
                if below > 0 {
                    ui.add_space(below as f32 * ITEM_HEIGHT);
                }
            });

        // Persist the real scroll offset from egui's state for next frame.
        self.scroll_offset = output.state.offset.y;
    }
}

impl Default for FileTreePanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_tree_default() {
        let _panel = FileTreePanel::new();
    }

    #[test]
    fn visible_range_is_clamped() {
        let files_len = 100;
        let offset = 500.0;
        let viewport = 200.0;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let start = (offset / ITEM_HEIGHT).floor() as usize;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let mut end = ((offset + viewport) / ITEM_HEIGHT).ceil() as usize + 1;
        end = end.min(files_len).max(start);
        assert!(start < files_len);
        assert!(end <= files_len);
        assert!(end >= start);
    }
}
