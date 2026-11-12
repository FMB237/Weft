//! Markdown preview panel.
//!
//! Renders the currently open document with `egui_commonmark`, and shows TOML
//! frontmatter as a metadata card instead of raw body text.

use crate::theme::palette;
use crate::widgets::chrome::{badge, card, empty_state};
use egui::{RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;
use weft_core::document::Frontmatter;
use weft_core::parser::{parse_frontmatter, split_frontmatter};

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
        if open_document.is_none() {
            empty_state(
                ui,
                "Nothing to preview",
                "Open a note and its rendered Markdown will appear here.",
            );
            return;
        }

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
    let palette = palette(ui.ctx());
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(title) = &frontmatter.title {
            ui.label(RichText::new(title).size(18.0).strong());
        }
        if !frontmatter.tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Tags").color(palette.text_muted).size(11.0));
                for tag in &frontmatter.tags {
                    badge(ui, &format!("#{tag}"), palette.accent);
                }
            });
        }
        if let Some(created) = &frontmatter.created {
            ui.label(
                RichText::new(format!("Created {created}"))
                    .size(11.0)
                    .color(palette.text_muted),
            );
        }
        if let Some(modified) = &frontmatter.modified {
            ui.label(
                RichText::new(format!("Modified {modified}"))
                    .size(11.0)
                    .color(palette.text_muted),
            );
        }
        for (key, value) in &frontmatter.other {
            ui.label(
                RichText::new(format!("{key}: {value}"))
                    .size(11.0)
                    .color(palette.text_muted),
            );
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
