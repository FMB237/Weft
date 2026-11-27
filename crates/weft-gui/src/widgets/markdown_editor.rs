//! Markdown editor widget.
//!
//! Renders the current document as an editable text area with syntax
//! highlighting applied through `TextEdit::layouter`.

use crate::widgets::syntax_highlight::SyntaxHighlighter;
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{Event, FontId, Id, Key, Modifiers, TextEdit, TextFormat, Ui};
use std::path::PathBuf;
use weft_core::lists::{self, ListEdit};

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

/// Autocomplete suggestion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Text to insert.
    pub insert: String,
    /// Display label in the popup.
    pub label: String,
    /// Optional detail/description.
    pub detail: Option<String>,
    /// Kind of completion for icon.
    pub kind: CompletionKind,
}

/// Kind of completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// Wikilink to another note (`[[note]]`).
    Wikilink,
    /// Tag (`#tag`).
    Tag,
}

/// Payload for drag-and-drop from file tree to editor.
#[derive(Clone)]
pub(crate) struct TreeDragPayload {
    _note_path: PathBuf,
    note_name: String,
}

/// State for the autocomplete popup.
#[derive(Debug, Default)]
pub struct CompletionState {
    /// Whether the completion popup is open.
    pub open: bool,
    /// The trigger character index where completion started.
    pub trigger_offset: usize,
    /// The trigger text (e.g., "[[" or "#").
    pub trigger: String,
    /// Available completion items.
    pub items: Vec<CompletionItem>,
    /// Index of the selected item.
    pub selected: usize,
    /// The text ID of the editor this completion belongs to.
    pub text_id: Option<Id>,
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
    /// Autocomplete state for this buffer.
    pub completion: CompletionState,
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
        self.completion = CompletionState::default();
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
    #[allow(
        clippy::too_many_lines,
        reason = "editor show handles layout, highlighting, autocomplete, smart lists, and drag-drop"
    )]
    pub fn show(
        &mut self,
        pane: usize,
        ui: &mut Ui,
        buffer: &mut EditorBuffer,
        ctx: &egui::Context,
        font_size: f32,
        vault: Option<&weft_core::vault::Vault>,
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

        // Smart lists run before the text editor draws, so a consumed key never
        // reaches it. The key is only consumed when an edit applies, which
        // leaves ordinary Enter and Tab behaviour untouched.
        if let Some(edit) = list_key_edit(ui, &buffer.content, text_id) {
            apply_list_edit(buffer, &edit);
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

        // Handle drag-and-drop from file tree: insert [[note_name]] at drop position
        if let Some(payload) = output
            .inner
            .response
            .dnd_release_payload::<TreeDragPayload>()
        {
            let wikilink = format!("[[{}]]", payload.note_name);
            // Get the cursor position for insertion
            if let Some(range) = output.inner.state.cursor.char_range() {
                let insert_pos = range.primary.index;
                let byte_pos = char_to_byte(&buffer.content, insert_pos);
                buffer.content.insert_str(byte_pos, &wikilink);
                buffer.dirty = true;
                buffer.cursor = Some(insert_pos + wikilink.chars().count());
            }
        }

        // Handle paste over selection: wrap selected text in [text](url)
        if let Some((sel_start, sel_end)) = buffer.selection
            && sel_start != sel_end
            && let Some(url) = Self::detect_paste_url(ctx)
        {
            let selected_text = &buffer.content[sel_start..sel_end];
            let replacement = format!("[{selected_text}]({url})");
            buffer
                .content
                .replace_range(sel_start..sel_end, &replacement);
            buffer.dirty = true;
            // Clear selection since we've consumed it
            buffer.selection = None;
            // Position cursor after the inserted link
            buffer.cursor = Some(sel_start + replacement.len());
        }

        // Handle image paste: save to attachments folder and insert markdown image
        // NOTE: Event::PasteImage is not available in egui 0.29.
        // This feature requires a newer egui version or platform-specific clipboard handling.
        // if let Some(image) = Self::detect_paste_image(ctx)
        //     && let Some(vault) = vault
        // {
        //     match self.save_pasted_image(&image, &vault.root) {
        //         Ok(rel_path) => {
        //             let markdown = format!("![Pasted image]({})", rel_path);
        //             if let Some(range) = output.inner.state.cursor.char_range() {
        //                 let insert_pos = range.primary.index;
        //                 let byte_pos = char_to_byte(&buffer.content, insert_pos);
        //                 buffer.content.insert_str(byte_pos, &markdown);
        //                 buffer.dirty = true;
        //                 buffer.cursor = Some(insert_pos + markdown.chars().count());
        //             }
        //         }
        //         Err(e) => {
        //             log::error!("Failed to save pasted image: {}", e);
        //         }
        //     }
        // }

        if output.inner.response.changed() {
            buffer.dirty = true;
        }

        // Handle autocomplete triggers after editor is rendered
        let editor_rect = output.inner.response.rect;
        self.handle_autocomplete_triggers(ui, ctx, text_id, buffer, vault, editor_rect);

        // Keep Tab inside the editor while the caret sits in a list item, so
        // smart indentation can act on it. egui reads the filter at the start
        // of the next frame; the text editor re-applies its own (Tab moves
        // focus) every frame, so leaving the list restores normal tabbing.
        if let Some(caret) = output
            .inner
            .state
            .cursor
            .char_range()
            .map(|range| range.primary.index)
            && lists::item_at(&buffer.content, caret).is_some()
            && ui.memory(|memory| memory.has_focus(text_id))
        {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    text_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: true,
                        escape: false,
                    },
                );
            });
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

    /// Check for autocomplete triggers and show completion popup.
    fn handle_autocomplete_triggers(
        &self,
        ui: &mut Ui,
        ctx: &egui::Context,
        text_id: Id,
        buffer: &mut EditorBuffer,
        vault: Option<&weft_core::vault::Vault>,
        editor_rect: egui::Rect,
    ) {
        // Only show completions if the editor has focus
        if !ui.memory(|memory| memory.has_focus(text_id)) {
            buffer.completion.open = false;
            return;
        }

        // Get the current caret position
        let Some(state) = TextEdit::load_state(ctx, text_id) else {
            buffer.completion.open = false;
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            buffer.completion.open = false;
            return;
        };
        let caret = range.primary.index;

        // Check if we're already showing completions
        if buffer.completion.open {
            // Handle keyboard navigation
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown))
                && buffer.completion.selected < buffer.completion.items.len().saturating_sub(1)
            {
                buffer.completion.selected += 1;
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && buffer.completion.selected > 0 {
                buffer.completion.selected -= 1;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                || ui.input(|i| i.key_pressed(egui::Key::Tab))
            {
                // Insert the selected completion
                let selected_idx = buffer.completion.selected;
                let item = buffer.completion.items.get(selected_idx).cloned();
                let trigger_offset = buffer.completion.trigger_offset;
                let trigger = buffer.completion.trigger.clone();
                if let Some(item) = item {
                    self.insert_completion(buffer, trigger_offset, &trigger, &item);
                    buffer.completion.open = false;
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                buffer.completion.open = false;
            }

            // Show the completion popup
            self.show_completion_popup(ctx, &buffer.completion, text_id, editor_rect);
            return;
        }

        // Check for new triggers: [[ or #
        let text = &buffer.content;
        if caret >= 2 {
            let prefix = &text[caret.saturating_sub(2)..caret];
            if prefix == "[[" {
                // Wikilink trigger
                self.open_wikilink_completion(ctx, buffer, text_id, caret, vault);
            } else if caret >= 1 && prefix.ends_with('#') {
                // Tag trigger
                self.open_tag_completion(ctx, buffer, text_id, caret, vault);
            }
        }
    }

    /// Open wikilink completion popup.
    fn open_wikilink_completion(
        &self,
        _ctx: &egui::Context,
        buffer: &mut EditorBuffer,
        text_id: Id,
        caret: usize,
        vault: Option<&weft_core::vault::Vault>,
    ) {
        let _ = self;
        // Get available notes from vault
        let items = if let Some(vault) = vault {
            let mut items = Vec::new();
            if let Ok(files) = vault.list_files() {
                for file in files {
                    if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                        items.push(CompletionItem {
                            insert: format!("[[{stem}]]"),
                            label: stem.to_string(),
                            detail: Some(file.to_string_lossy().to_string()),
                            kind: CompletionKind::Wikilink,
                        });
                    }
                }
            }
            items
        } else {
            Vec::new()
        };

        if !items.is_empty() {
            buffer.completion = CompletionState {
                open: true,
                trigger_offset: caret - 2,
                trigger: "[[".to_string(),
                items,
                selected: 0,
                text_id: Some(text_id),
            };
        }
    }

    /// Open tag completion popup.
    fn open_tag_completion(
        &self,
        _ctx: &egui::Context,
        buffer: &mut EditorBuffer,
        text_id: Id,
        caret: usize,
        vault: Option<&weft_core::vault::Vault>,
    ) {
        let _ = self;
        // Get tags from the current document's frontmatter and other notes
        let mut tags = std::collections::BTreeSet::new();

        // Extract tags from current document
        let frontmatter = weft_core::parser::parse_frontmatter(&buffer.content);
        for tag in &frontmatter.tags {
            tags.insert(tag.clone());
        }

        // Get tags from vault
        if let Some(vault) = vault
            && let Ok(files) = vault.list_files()
        {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let fm = weft_core::parser::parse_frontmatter(&content);
                    for tag in &fm.tags {
                        tags.insert(tag.clone());
                    }
                }
            }
        }

        let items: Vec<CompletionItem> = tags
            .into_iter()
            .map(|tag| CompletionItem {
                insert: tag.clone(),
                label: format!("#{tag}"),
                detail: None,
                kind: CompletionKind::Tag,
            })
            .collect();

        if !items.is_empty() {
            buffer.completion = CompletionState {
                open: true,
                trigger_offset: caret - 1,
                trigger: "#".to_string(),
                items,
                selected: 0,
                text_id: Some(text_id),
            };
        }
    }

    /// Detect if a paste event occurred and the pasted text is a valid URL.
    fn detect_paste_url(ctx: &egui::Context) -> Option<String> {
        let input = ctx.input(Clone::clone);
        for event in &input.events {
            if let Event::Paste(text) = event {
                let trimmed = text.trim();
                if Self::is_valid_url(trimmed) {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    // NOTE: Image paste support is disabled in egui 0.29 because Event::PasteImage
    // is not available. Uncomment when upgrading to a newer egui version.
    // /// Detect if a paste event occurred with an image.
    // fn detect_paste_image(ctx: &egui::Context) -> Option<egui::ColorImage> {
    //     let input = ctx.input(|i| i.clone());
    //     for event in &input.events {
    //         if let Event::PasteImage(image) = event {
    //             return Some((**image).clone());
    //         }
    //     }
    //     None
    // }
    //
    // /// Save an image to the attachments folder and return the relative path.
    // fn save_pasted_image(
    //     &self,
    //     image: &egui::ColorImage,
    //     vault_path: &std::path::Path,
    // ) -> Result<String, String> {
    //     let attachments_dir = vault_path.join(".weft").join("attachments");
    //     std::fs::create_dir_all(&attachments_dir)
    //         .map_err(|e| format!("Failed to create attachments directory: {}", e))?;
    //
    //     let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    //     let uuid = Uuid::new_v4().simple();
    //     let filename = format!("paste-{}-{}.png", timestamp, uuid);
    //     let file_path = attachments_dir.join(&filename);
    //
    //     let rgba = image::RgbaImage::from_raw(
    //         image.size[0] as u32,
    //         image.size[1] as u32,
    //         image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect(),
    //     )
    //     .ok_or("Failed to create RgbaImage from ColorImage")?;
    //
    //     rgba.save(&file_path)
    //         .map_err(|e| format!("Failed to save image: {}", e))?;
    //
    //     Ok(format!(".weft/attachments/{}", filename))
    // }

    /// Simple URL validation - checks for common URL schemes.
    fn is_valid_url(text: &str) -> bool {
        text.starts_with("http://")
            || text.starts_with("https://")
            || text.starts_with("ftp://")
            || text.starts_with("mailto:")
    }

    /// Insert the selected completion into the buffer.
    fn insert_completion(
        &self,
        buffer: &mut EditorBuffer,
        trigger_offset: usize,
        trigger: &str,
        item: &CompletionItem,
    ) {
        let _ = self;
        let start = char_to_byte(&buffer.content, trigger_offset);
        let end = char_to_byte(&buffer.content, trigger_offset + trigger.chars().count());
        buffer.content.replace_range(start..end, &item.insert);
        buffer.cursor = Some(trigger_offset + item.insert.chars().count());
        buffer.dirty = true;
    }

    /// Show the completion popup.
    fn show_completion_popup(
        &self,
        ctx: &egui::Context,
        completion: &CompletionState,
        text_id: Id,
        editor_rect: egui::Rect,
    ) {
        use egui::{Align, Order};
        let _ = self;

        // Position the popup near the cursor area - use editor rect as base
        let cursor_x = editor_rect.min.x + 10.0;
        let cursor_y = editor_rect.max.y + 5.0;

        let popup_id = text_id.with("completion_popup");
        let items = completion.items.clone();
        let selected = completion.selected;

        egui::Area::new(popup_id)
            .order(Order::Foreground)
            .fixed_pos(egui::pos2(cursor_x, cursor_y))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    ui.set_max_width(400.0);
                    ui.set_max_height(300.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, item) in items.iter().enumerate() {
                            let is_selected = idx == selected;
                            let response = ui.selectable_label(is_selected, &item.label);
                            if response.clicked() {
                                // We can't modify completion from here due to borrow checker
                                // The insertion is handled in handle_autocomplete_triggers
                            }
                            if is_selected {
                                ui.scroll_to_cursor(Some(Align::LEFT));
                            }
                        }
                    });
                });
            });
    }
}

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// The smart-list edit for the keys pressed this frame, if any.
///
/// The editor must own focus and the caret must be collapsed. Keys are
/// consumed only when an edit applies, so the text editor still gets Enter
/// and Tab everywhere else.
fn list_key_edit(ui: &Ui, text: &str, text_id: Id) -> Option<ListEdit> {
    if !ui.memory(|memory| memory.has_focus(text_id)) {
        return None;
    }
    let state = TextEdit::load_state(ui.ctx(), text_id)?;
    let range = state.cursor.char_range()?;
    if range.primary.index != range.secondary.index {
        return None;
    }
    let caret = range.primary.index;
    let modifiers = ui.input(|input| input.modifiers);
    if modifiers.ctrl || modifiers.command || modifiers.alt {
        return None;
    }

    // Shift+Enter asks for a plain newline, so it never continues the list.
    if !modifiers.shift && ui.input(|input| input.key_pressed(Key::Enter)) {
        let edit = lists::continue_list(text, caret)?;
        ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter);
        });
        return Some(edit);
    }
    if ui.input(|input| input.key_pressed(Key::Tab)) {
        let (edit, modifiers) = if modifiers.shift {
            (lists::outdent_item(text, caret)?, Modifiers::SHIFT)
        } else {
            (lists::indent_item(text, caret)?, Modifiers::NONE)
        };
        ui.input_mut(|input| {
            input.consume_key(modifiers, Key::Tab);
        });
        return Some(edit);
    }
    None
}

/// Apply a smart-list edit to the buffer and park the caret after it.
fn apply_list_edit(buffer: &mut EditorBuffer, edit: &ListEdit) {
    let start = char_to_byte(&buffer.content, edit.start);
    let end = char_to_byte(&buffer.content, edit.end);
    buffer.content.replace_range(start..end, &edit.insert);
    buffer.cursor = Some(edit.caret);
    buffer.dirty = true;
}

/// Byte offset of a character index, clamped to the text length.
fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
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
