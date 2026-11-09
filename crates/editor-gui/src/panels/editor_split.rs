//! Editor split panel.
//!
//! Central panel that splits between the markdown editor widget and the
//! preview pane.

use std::path::PathBuf;

use crate::widgets::{MarkdownEditor, SyntaxHighlighter};
use crate::panels::preview::PreviewPanel;
use egui::Ui;

pub struct EditorSplitPanel {
    editor: MarkdownEditor,
    highlighter: SyntaxHighlighter,
    preview: PreviewPanel,
    /// Split ratio between editor and preview (0.0 = all editor, 1.0 = all preview).
    pub split_ratio: f32,
}

impl EditorSplitPanel {
    pub fn new() -> Self {
        Self {
            editor: MarkdownEditor::new(),
            highlighter: SyntaxHighlighter::new(),
            preview: PreviewPanel::new(),
            split_ratio: 0.5,
        }
    }

    /// Show the editor split panel.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        open_document: &Option<PathBuf>,
        document_content: &mut String,
        dirty: &mut bool,
        ctx: &egui::Context,
        split_ratio: &mut f32,
    ) {
        // Split between editor (left) and preview (right) with draggable divider.
        ui.columns(2, |columns| {
            // Left column: Editor.
            columns[0].heading("Editor");
            self.editor.show(
                &mut columns[0],
                open_document,
                document_content,
                dirty,
                ctx,
            );

            // Right column: Preview.
            columns[1].heading("Preview");
            self.preview.show(&mut columns[1], open_document, document_content);
        });

        self.split_ratio = *split_ratio;
    }

    /// Get the current split ratio.
    pub fn split_ratio(&self) -> f32 {
        self.split_ratio
    }
}

impl Default for EditorSplitPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_split_default() {
        let _panel = EditorSplitPanel::new();
    }
}