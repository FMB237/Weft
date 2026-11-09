//! Markdown editor panel.

pub struct EditorPanel {
    content: String,
}

impl EditorPanel {
    pub fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        egui::TextEdit::multiline(&mut self.content)
            .hint_text("Type markdown here...")
            .show(ui);
    }
}

impl Default for EditorPanel {
    fn default() -> Self {
        Self::new()
    }
}
