//! Markdown preview panel.
//!
//! Renders the currently open document with `egui_commonmark`, and shows TOML
//! frontmatter as a metadata card instead of raw body text. Wikilinks are
//! rewritten into ordinary Markdown links first, so they render as links and
//! a click comes back as an [`PreviewLink`].

use crate::theme::palette;
use crate::widgets::chrome::{badge, card, empty_state};
use egui::{RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;
use weft_core::document::Frontmatter;
use weft_core::parser::{linkify_wikilinks, parse_frontmatter, split_frontmatter, wikilink_target};

/// A link the user clicked in the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLink {
    /// The wikilink target as written, without brackets (`note`,
    /// `folder/note`, `note#Heading`).
    pub target: String,
    /// Whether the click asked for a new tab: any modifier, or middle click.
    pub new_tab: bool,
}

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
    ///
    /// Returns the wikilink the user clicked, if any. External links are left
    /// to the platform, which opens them in the browser.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        open_document: &Option<PathBuf>,
        document_content: &str,
    ) -> Option<PreviewLink> {
        if open_document.is_none() {
            empty_state(
                ui,
                "Nothing to preview",
                "Open a note and its rendered Markdown will appear here.",
            );
            return None;
        }

        let frontmatter = parse_frontmatter(document_content);
        let (_, body) = split_frontmatter(document_content);

        if has_frontmatter(&frontmatter) {
            show_frontmatter_card(ui, &frontmatter);
            ui.add_space(8.0);
        }

        let body = linkify_wikilinks(body);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().url_in_tooltip = true;
                CommonMarkViewer::new().show(ui, &mut self.cache, &body);
            });

        // A link click becomes an `OpenUrl` output for this frame. Claim the
        // wikilink scheme and put anything else back for the platform.
        let opened = ui.ctx().output_mut(|output| output.open_url.take());
        let opened = opened?;
        if let Some(target) = wikilink_target(&opened.url) {
            return Some(PreviewLink {
                target: target.to_string(),
                new_tab: opened.new_tab,
            });
        }
        ui.ctx().output_mut(|output| output.open_url = Some(opened));
        None
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
