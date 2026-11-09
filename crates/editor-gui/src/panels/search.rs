//! Search panel.

#[derive(Debug)]
pub struct SearchPanel {
    query: String,
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
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("Search");
        ui.text_edit_singleline(&mut self.query);
    }
}
