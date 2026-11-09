//! Markdown preview panel.
//!
//! Renders the currently open document with `egui_commonmark`, and shows TOML
//! frontmatter as a metadata card instead of raw body text.

use editor_core::document::Frontmatter;
use editor_core::parser::{parse_frontmatter, split_frontmatter};
use egui::Ui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;

#[derive(Debug)]
pub struct PreviewPanel {
    cache: CommonMarkCache,
}

impl PreviewPanel {
    pub fn new() -> Self {
        Self {
            cache: CommonMarkCache::default(),
        }
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
        ui.horizontal(|ui| {
            ui.heading(name);
            ui.label("— preview");
        });
        ui.separator();

        let frontmatter = parse_frontmatter(document_content);
        let (_, body) = split_frontmatter(document_content);

        if has_frontmatter(&frontmatter) {
            show_frontmatter_card(ui, &frontmatter);
            ui.add_space(8.0);
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().url_in_tooltip = true;
                CommonMarkViewer::new().show(ui, &mut self.cache, body);
            });
    }
}

fn has_frontmatter(frontmatter: &Frontmatter) -> bool {
    frontmatter.title.is_some()
        || !frontmatter.tags.is_empty()
        || frontmatter.created.is_some()
        || frontmatter.modified.is_some()
        || !frontmatter.other.is_empty()
}

fn show_frontmatter_card(ui: &mut Ui, frontmatter: &Frontmatter) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(title) = &frontmatter.title {
            ui.heading(title);
        }
        if !frontmatter.tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Tags:");
                for tag in &frontmatter.tags {
                    ui.label(egui::RichText::new(format!("#{tag}")).monospace());
                }
            });
        }
        if let Some(created) = &frontmatter.created {
            ui.label(format!("Created: {created}"));
        }
        if let Some(modified) = &frontmatter.modified {
            ui.label(format!("Modified: {modified}"));
        }
        for (key, value) in &frontmatter.other {
            ui.label(format!("{key}: {value}"));
        }
    });
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
