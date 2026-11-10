//! Search panel.
//!
//! Searches the open vault through `editor_core::index::SearchIndex` and lets
//! the user open a result with the mouse or arrow keys + Enter.

use crate::event_bus::{Event, EventBus};
use editor_core::index::{SearchIndex, SearchOptions, SearchResult};
use egui::{Key, TextEdit, Ui};

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
                .hint_text("Search notes…")
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

            let trimmed = self.query.trim();
            if !trimmed.is_empty() {
                match index {
                    Some(index) => match index.search(trimmed, &SearchOptions::default()) {
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
            if !self.query.trim().is_empty() && self.error.is_none() {
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
                    let label = format!("{}\n{}", result.title, result.snippet);
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
}
