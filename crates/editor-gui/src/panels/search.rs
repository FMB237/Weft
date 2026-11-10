//! Search panel.
//!
//! Searches the open vault through `editor_core::index::SearchIndex` and lets
//! the user open a result with the mouse or arrow keys + Enter.

use crate::event_bus::{Event, EventBus};
use editor_core::index::{SearchIndex, SearchOptions, SearchResult};
use egui::{Key, TextEdit, Ui};

/// A raw search box value split into free text and `#tag` filters.
#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedQuery {
    text: String,
    tags: Vec<String>,
}

/// Split `foo #rust #notes` into `text = "foo"` and two tag filters.
fn parse_query(input: &str) -> ParsedQuery {
    let mut words = Vec::new();
    let mut tags = Vec::new();

    for token in input.split_whitespace() {
        if let Some(tag) = token.strip_prefix('#') {
            let tag = tag.trim_matches(|c: char| {
                matches!(c, ',' | ';' | ':' | '!' | '?' | '.' | '(' | ')' | '[' | ']')
            });
            if !tag.is_empty() {
                tags.push(tag.to_string());
                continue;
            }
        }
        words.push(token.to_string());
    }

    ParsedQuery {
        text: words.join(" "),
        tags,
    }
}

#[derive(Debug)]
pub struct SearchPanel {
    query: String,
    last_query: String,
    results: Vec<SearchResult>,
    selected: usize,
    focus_requested: bool,
    error: Option<String>,
}

impl Default for SearchPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchPanel {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            last_query: String::new(),
            results: Vec::new(),
            selected: 0,
            focus_requested: false,
            error: None,
        }
    }

    /// Ask for keyboard focus on the next frame (used when the panel opens).
    pub fn focus(&mut self) {
        self.focus_requested = true;
    }

    /// Force a re-query on the next frame (the index changed).
    pub fn invalidate(&mut self) {
        self.last_query.clear();
    }

    /// Show the search UI. Clicking or pressing Enter on a result publishes
    /// [`Event::NodeSelected`]. Returns `true` when the user asked to close.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        index: Option<&SearchIndex>,
        event_bus: &mut EventBus,
    ) -> bool {
        let mut close = false;
        ui.horizontal(|ui| {
            ui.heading("Search");
            if ui.small_button("Close").clicked() {
                close = true;
            }
        });

        let response = ui.add(
            TextEdit::singleline(&mut self.query)
                .hint_text("Search notes… (#tag to filter)")
                .desired_width(f32::INFINITY),
        );
        if self.focus_requested {
            response.request_focus();
            self.focus_requested = false;
        }

        if self.query != self.last_query {
            self.last_query.clone_from(&self.query);
            self.results.clear();
            self.selected = 0;
            self.error = None;

            let parsed = parse_query(self.query.trim());
            if !parsed.text.is_empty() || !parsed.tags.is_empty() {
                let options = SearchOptions {
                    limit: 20,
                    tags: parsed.tags,
                };
                match index {
                    Some(index) => match index.search(&parsed.text, &options) {
                        Ok(results) => self.results = results,
                        Err(error) => self.error = Some(error.to_string()),
                    },
                    None => self.error = Some("No vault is indexed.".into()),
                }
            }
        }

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(220, 100, 80), error);
        }

        if self.results.is_empty() {
            let parsed = parse_query(self.query.trim());
            if (!parsed.text.is_empty() || !parsed.tags.is_empty()) && self.error.is_none() {
                ui.label("No matches.");
            }
            return close;
        }

        // Keyboard navigation.
        if ui.input(|input| input.key_pressed(Key::ArrowDown)) {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }
        if ui.input(|input| input.key_pressed(Key::ArrowUp)) {
            self.selected = self.selected.saturating_sub(1);
        }
        if ui.input(|input| input.key_pressed(Key::Enter))
            && let Some(result) = self.results.get(self.selected)
        {
            event_bus.publish(Event::NodeSelected(result.path.clone()));
        }

        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, result) in self.results.iter().enumerate() {
                    let tags = if result.tags.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "\n{}",
                            result
                                .tags
                                .iter()
                                .map(|tag| format!("#{tag}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    };
                    let label = format!("{}{}\n{}", result.title, tags, result.snippet);
                    let response = ui.selectable_label(index == self.selected, label);
                    if response.clicked() {
                        self.selected = index;
                        event_bus.publish(Event::NodeSelected(result.path.clone()));
                    }
                }
            });

        close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_panel_default() {
        let panel = SearchPanel::new();
        assert!(panel.query.is_empty());
        assert!(panel.results.is_empty());
    }

    #[test]
    fn parse_query_separates_tags_and_text() {
        let parsed = parse_query("roadmap #Rust, #notes");
        assert_eq!(parsed.text, "roadmap");
        assert_eq!(parsed.tags, vec!["Rust", "notes"]);
    }

    #[test]
    fn parse_query_keeps_standalone_hash_and_supports_tag_only() {
        assert_eq!(parse_query("# ").text, "#");
        let parsed = parse_query("#project");
        assert!(parsed.text.is_empty());
        assert_eq!(parsed.tags, vec!["project"]);
    }
}
