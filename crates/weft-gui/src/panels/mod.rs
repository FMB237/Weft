//! Panel module.
//!
//! Defines the [`PanelId`] enum, the [`Panels`] container holding all UI
//! panels, and sub-modules for each individual panel.

pub mod backlinks;
pub mod calendar;
pub mod editor_split;
pub mod file_tree;
pub mod graph;
pub mod outline;
pub mod preview;
pub mod properties;
pub mod search;
pub mod tasks;
pub mod views;

use crate::panels::backlinks::BacklinksPanel;
use crate::panels::calendar::CalendarPanel;
use crate::panels::file_tree::FileTreePanel;
use crate::panels::graph::GraphPanel;
use crate::panels::outline::OutlinePanel;
use crate::panels::preview::PreviewPanel;
use crate::panels::properties::PropertiesPanel;
use crate::panels::search::SearchPanel;
use crate::panels::tasks::TasksPanel;
use crate::panels::views::ViewsPanel;
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
    Properties,
    Views,
    Tasks,
    Calendar,
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
    /// Right side panel: preview.
    pub preview: PreviewPanel,
    /// Graph visualization panel (Phase 5 placeholder).
    pub graph: GraphPanel,
    /// Search panel.
    pub search: SearchPanel,
    /// Backlinks for the open note.
    pub backlinks: BacklinksPanel,
    /// Headings of the open note.
    pub outline: OutlinePanel,
    /// Properties panel for editing frontmatter.
    pub properties: PropertiesPanel,
    /// Views panel for table/cards/board views.
    pub views: ViewsPanel,
    /// Tasks panel for managing tasks across the vault.
    pub tasks: TasksPanel,
    /// Calendar panel for viewing notes by date.
    pub calendar: CalendarPanel,
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
            preview: PreviewPanel::new(),
            graph: GraphPanel::new(),
            search: SearchPanel::new(),
            backlinks: BacklinksPanel::new(),
            outline: OutlinePanel::new(),
            properties: PropertiesPanel::new(),
            views: ViewsPanel::new(),
            tasks: TasksPanel::new(),
            calendar: CalendarPanel::new(),
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
