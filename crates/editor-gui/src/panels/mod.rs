//! Panel module.
//!
//! Defines the [`PanelId`] enum, the [`Panels`] container holding all UI
//! panels, and sub-modules for each individual panel.

pub mod editor_split;
pub mod file_tree;
pub mod graph;
pub mod preview;
pub mod search;

use crate::panels::editor_split::EditorSplitPanel;
use crate::panels::file_tree::FileTreePanel;
use crate::panels::graph::GraphPanel;
use crate::panels::preview::PreviewPanel;
use crate::panels::search::SearchPanel;
use serde::{Deserialize, Serialize};

/// Identifier for each dockable panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum PanelId {
    #[default]
    FileTree,
    EditorSplit,
    Preview,
    Graph,
    Search,
}

/// Container for all UI panels.
///
/// Holds the active panel selection, theme settings, and each panel's
/// instance. Panel `show` methods borrow `EditorApp` mutably one at a time.
#[derive(Debug)]
pub struct Panels {
    /// Currently active panel.
    pub active: PanelId,
    /// Left side panel: file tree.
    pub file_tree: FileTreePanel,
    /// Central panel: editor + preview split.
    pub editor_split: EditorSplitPanel,
    /// Right side panel: preview.
    pub preview: PreviewPanel,
    /// Graph visualization panel (Phase 5 placeholder).
    pub graph: GraphPanel,
    /// Search panel.
    pub search: SearchPanel,
}

impl Default for Panels {
    fn default() -> Self {
        Self::new()
    }
}

impl Panels {
    /// Create a new set of panels with default state.
    pub fn new() -> Self {
        Self {
            active: PanelId::default(),
            file_tree: FileTreePanel::new(),
            editor_split: EditorSplitPanel::new(),
            preview: PreviewPanel::new(),
            graph: GraphPanel::new(),
            search: SearchPanel::new(),
        }
    }

    /// Set the active panel.
    pub fn set_active(&mut self, panel: PanelId) {
        self.active = panel;
    }

    /// Return the currently active panel id.
    pub fn active_panel(&self) -> PanelId {
        self.active
    }
}
