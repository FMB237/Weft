//! Outline panel.
//!
//! Lists the headings of the note currently in the editor. The headings come
//! from `weft_core::outline`, which reads the live buffer, so a heading shows
//! up as soon as it is typed instead of after a save.
//!
//! The panel does not scroll the editor itself: it returns a [`Jump`] and the
//! app decides how to move the caret, which keeps the widget free of app state.

use crate::theme::{palette, space};
use crate::widgets::chrome::section_header;
use egui::{RichText, Ui};
use weft_core::outline::{self, Heading};

/// Where the user wants the editor to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jump {
    /// Character (not byte) offset of the heading line in the buffer.
    pub char_offset: usize,
}

/// The outline of the open note.
#[derive(Debug, Default)]
pub struct OutlinePanel {
    /// Filter over heading text.
    filter: String,
}

impl OutlinePanel {
    /// A panel with an empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the outline for `content`, highlighting the section containing
    /// `cursor` (a character offset into `content`).
    ///
    /// Returns the heading the user clicked, if any.
    pub fn show(&mut self, ui: &mut Ui, content: &str, cursor: Option<usize>) -> Option<Jump> {
        let palette = palette(ui.ctx());
        let headings = outline::outline(content);
        section_header(ui, "Outline");

        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Filter headings")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(space::XS);

        if headings.is_empty() {
            ui.label(
                RichText::new("No headings in this note.")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            return None;
        }

        let needle = self.filter.trim().to_lowercase();
        let cursor_byte = char_to_byte(content, cursor.unwrap_or(0));
        let active = outline::heading_at(&headings, cursor_byte);

        let mut jump = None;
        egui::ScrollArea::vertical()
            .id_salt("outline_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut shown = 0;
                for (index, heading) in headings.iter().enumerate() {
                    if !needle.is_empty() && !heading.text.to_lowercase().contains(&needle) {
                        continue;
                    }
                    shown += 1;
                    if Self::row(ui, heading, active == Some(index)) {
                        jump = Some(Jump {
                            char_offset: byte_to_char(content, heading.offset),
                        });
                    }
                }
                if shown == 0 {
                    ui.label(
                        RichText::new("No headings match.")
                            .size(12.0)
                            .color(palette.text_muted),
                    );
                }
            });
        jump
    }

    /// One heading row, indented by level.
    fn row(ui: &mut Ui, heading: &Heading, active: bool) -> bool {
        let palette = palette(ui.ctx());
        // Level 1 is the largest; deeper levels step down but stay readable.
        let size = (14.5 - f32::from(heading.level.saturating_sub(1)) * 0.6).max(11.0);
        let indent = f32::from(heading.level.saturating_sub(1)) * 10.0;
        let mut clicked = false;

        ui.horizontal(|ui| {
            ui.add_space(indent);
            let mut text = RichText::new(heading.label()).size(size);
            if heading.level <= 2 {
                text = text.strong();
            }
            if active {
                text = text.color(palette.accent);
            }
            let response = ui.add(
                egui::Label::new(text)
                    .selectable(false)
                    .sense(egui::Sense::click()),
            );
            if active {
                // A thin accent bar makes the current section easy to find.
                let rect = response.rect;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.left() - indent - 6.0, rect.top()),
                        egui::vec2(2.0, rect.height()),
                    ),
                    1.0,
                    palette.accent,
                );
            }
            if response.clicked() {
                clicked = true;
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        });
        clicked
    }
}

/// Byte offset of a character offset, clamped to the string.
fn char_to_byte(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Character offset of a byte offset, clamped to the string.
fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_convert_both_ways() {
        let text = "héllo\nwörld\n";
        assert_eq!(char_to_byte(text, 0), 0);
        assert_eq!(char_to_byte(text, 1), 1);
        assert_eq!(char_to_byte(text, 2), 3, "é is two bytes");
        assert_eq!(byte_to_char(text, 3), 2);
        assert_eq!(byte_to_char(text, usize::MAX), text.chars().count());
        assert_eq!(char_to_byte(text, usize::MAX), text.len());
    }
}
