//! Workspace layout: panes, tabs, and session restore.
//!
//! A [`Workspace`] is the set of notes the window has open: one or two
//! [`Pane`]s, each holding an ordered list of [`Tab`]s and one active tab. The
//! model is plain data — no `egui` types — so tab behaviour is unit-testable
//! without driving the UI, and the whole workspace persists to
//! `<vault>/.weft/workspace.toml`.
//!
//! Unsaved buffers and the "reopen closed tab" stack are deliberately not
//! serialized: a restored session should read from disk, and a crash-recovery
//! buffer is a separate concern (see `PLAN.md`, M4 data safety).

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Directory holding per-vault bookkeeping inside the vault root.
pub const WORKSPACE_DIR: &str = ".weft";
/// File name of the persisted workspace inside [`WORKSPACE_DIR`].
pub const WORKSPACE_FILE_NAME: &str = "workspace.toml";
/// Weft v1 supports a single split, so at most two panes.
pub const MAX_PANES: usize = 2;
/// How many closed tabs can be reopened.
pub const CLOSED_STACK_LIMIT: usize = 20;

/// How a tab renders its note.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabMode {
    /// Markdown source only.
    #[default]
    Source,
    /// Source and rendered preview side by side.
    Split,
    /// Rendered preview only.
    Preview,
}

impl TabMode {
    /// Human-readable label for menus and the status bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Split => "Split",
            Self::Preview => "Preview",
        }
    }
}

/// How the two panes are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitAxis {
    /// Panes side by side.
    #[default]
    Horizontal,
    /// Panes stacked.
    Vertical,
}

/// One open note, with the view state that belongs to that tab.
///
/// `path` is always vault-relative. `cursor` is a character offset into the
/// note, and `scroll` is the editor's vertical scroll offset in points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tab {
    /// Vault-relative path of the note.
    pub path: PathBuf,
    /// Source, split, or preview.
    pub mode: TabMode,
    /// Character offset of the caret, restored when the tab is activated.
    pub cursor: Option<usize>,
    /// Vertical scroll offset, restored when the tab is activated.
    pub scroll: f32,
    /// Buffer edited in memory but not yet written to disk.
    ///
    /// Not persisted: a restored session reads the file, and crash recovery
    /// has its own snapshot mechanism.
    #[serde(skip)]
    pub unsaved: Option<String>,
}

impl Tab {
    /// A tab for `path`, using the default view mode.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Self::default()
        }
    }

    /// File name shown on the tab, or the whole path when it has none.
    pub fn title(&self) -> String {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.path.display().to_string(), ToString::to_string)
    }

    /// Whether this tab holds unsaved edits.
    pub fn is_dirty(&self) -> bool {
        self.unsaved.is_some()
    }
}

/// One pane: an ordered list of tabs and the index of the visible one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pane {
    /// Open tabs, left to right.
    pub tabs: Vec<Tab>,
    /// Index of the visible tab. Kept in range by [`Pane::sanitize`].
    pub active: usize,
}

impl Pane {
    /// An empty pane with no tabs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of open tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Whether the pane has no tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The visible tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// The visible tab for mutation, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// Append `tab`, activate it, and return its index.
    pub fn push(&mut self, tab: Tab) -> usize {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.active
    }

    /// Make `index` visible. Out-of-range indices are ignored.
    pub fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// Remove the tab at `index`, returning it when it existed.
    ///
    /// The active index shifts so the neighbouring tab becomes visible, which
    /// is what closing a middle tab should do.
    pub fn close(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if index < self.active {
            self.active -= 1;
        } else if index == self.active {
            self.active = index.min(self.tabs.len() - 1);
        }
        Some(tab)
    }

    /// Drop tabs that cannot be restored and clamp the active index.
    pub fn sanitize(&mut self) {
        self.tabs.retain(|tab| is_safe_tab_path(&tab.path));
        if self.tabs.is_empty() {
            self.active = 0;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
    }
}

/// The window's open notes: up to two panes, a split axis, and the closed-tab
/// stack used by "reopen closed tab".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// Panes, left to right or top to bottom.
    pub panes: Vec<Pane>,
    /// Index of the pane that receives new tabs.
    pub focused: usize,
    /// How the panes are arranged.
    pub axis: SplitAxis,
    /// Position of the divider between panes, 0.0–1.0.
    pub split_ratio: f32,
    /// Recently closed tabs, oldest first. Not persisted.
    #[serde(skip)]
    pub closed: Vec<Tab>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            panes: vec![Pane::new()],
            focused: 0,
            axis: SplitAxis::Horizontal,
            split_ratio: 0.5,
            closed: Vec::new(),
        }
    }
}

impl Workspace {
    /// An empty workspace with one pane.
    pub fn new() -> Self {
        Self::default()
    }

    /// A workspace showing a single note.
    pub fn with_tab(path: impl Into<PathBuf>) -> Self {
        let mut workspace = Self::new();
        workspace.open(path);
        workspace
    }

    /// Whether no note is open in any pane.
    pub fn is_empty(&self) -> bool {
        self.panes.iter().all(Pane::is_empty)
    }

    /// Total number of open tabs across all panes.
    pub fn tab_count(&self) -> usize {
        self.panes.iter().map(Pane::len).sum()
    }

    /// The pane that receives new tabs.
    pub fn focused_pane(&self) -> &Pane {
        let index = self.focused.min(self.panes.len().saturating_sub(1));
        &self.panes[index]
    }

    /// The pane that receives new tabs, for mutation.
    pub fn focused_pane_mut(&mut self) -> &mut Pane {
        let index = self.focused.min(self.panes.len().saturating_sub(1));
        &mut self.panes[index]
    }

    /// The visible tab of the focused pane.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.focused_pane().active_tab()
    }

    /// The visible tab of the focused pane, for mutation.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.focused_pane_mut().active_tab_mut()
    }

    /// Vault-relative path of the visible tab.
    pub fn active_path(&self) -> Option<&Path> {
        self.active_tab().map(|tab| tab.path.as_path())
    }

    /// Whether `path` is open in any pane.
    pub fn is_open(&self, path: &Path) -> bool {
        self.panes
            .iter()
            .any(|pane| pane.tabs.iter().any(|tab| tab.path == path))
    }

    /// Show `path` in the focused pane, reusing an existing tab when possible.
    ///
    /// Returns the index of the visible tab. Opening the note that is already
    /// visible is a no-op.
    pub fn open(&mut self, path: impl Into<PathBuf>) -> usize {
        let path = path.into();
        let pane_index = self.focused.min(self.panes.len().saturating_sub(1));
        self.open_in(pane_index, path)
    }

    /// Show `path` in a specific pane, reusing a tab in that pane when it is
    /// already open there and appending one otherwise.
    pub fn open_in(&mut self, pane_index: usize, path: impl Into<PathBuf>) -> usize {
        let path = path.into();
        if self.panes.is_empty() {
            self.panes.push(Pane::new());
        }
        let pane_index = pane_index.min(self.panes.len() - 1);
        self.focused = pane_index;
        let pane = &mut self.panes[pane_index];
        if let Some(index) = pane.tabs.iter().position(|tab| tab.path == path) {
            pane.activate(index);
            return index;
        }
        pane.push(Tab::new(path))
    }

    /// Show `path` in the focused pane in a new tab, even when it is open.
    pub fn open_new(&mut self, path: impl Into<PathBuf>) -> usize {
        let pane = self.focused_pane_mut();
        pane.push(Tab::new(path))
    }

    /// Make the tab at `index` visible in the focused pane.
    pub fn activate(&mut self, index: usize) {
        self.focused_pane_mut().activate(index);
    }

    /// Switch to the next (`delta > 0`) or previous tab, wrapping around.
    pub fn cycle(&mut self, delta: isize) {
        let pane = self.focused_pane_mut();
        let len = pane.tabs.len();
        if len < 2 {
            return;
        }
        let len = isize::try_from(len).unwrap_or(isize::MAX);
        let current = isize::try_from(pane.active).unwrap_or(0);
        let next = (current + delta).rem_euclid(len);
        pane.active = usize::try_from(next).unwrap_or(0);
    }

    /// Close the visible tab of the focused pane.
    pub fn close_active(&mut self) -> Option<Tab> {
        let pane = self.focused_pane_mut();
        let index = pane.active;
        self.close_tab(self.focused, index)
    }

    /// Close one tab, remembering it for [`Workspace::reopen_closed`].
    ///
    /// Returns the closed tab, including any unsaved buffer.
    pub fn close_tab(&mut self, pane_index: usize, index: usize) -> Option<Tab> {
        let pane = self.panes.get_mut(pane_index)?;
        let tab = pane.close(index)?;
        self.closed.push(tab.clone());
        if self.closed.len() > CLOSED_STACK_LIMIT {
            self.closed.remove(0);
        }
        if pane.is_empty() && self.panes.len() > 1 {
            self.panes.remove(pane_index);
            self.focused = self.focused.min(self.panes.len() - 1);
        }
        Some(tab)
    }

    /// Keep only the visible tab in the focused pane.
    pub fn close_others(&mut self) {
        let pane = self.focused_pane_mut();
        let keep = pane.active;
        let mut removed = Vec::new();
        for (index, tab) in pane.tabs.iter().enumerate() {
            if index != keep {
                removed.push(tab.clone());
            }
        }
        if let Some(tab) = pane.tabs.get(keep).cloned() {
            pane.tabs = vec![tab];
            pane.active = 0;
        }
        self.remember_closed(removed);
    }

    /// Close every tab to the right of the visible one in the focused pane.
    pub fn close_to_right(&mut self) {
        let pane = self.focused_pane_mut();
        if pane.active + 1 >= pane.tabs.len() {
            return;
        }
        let removed: Vec<Tab> = pane.tabs.split_off(pane.active + 1);
        self.remember_closed(removed);
    }

    /// Reopen the most recently closed tab in the focused pane.
    pub fn reopen_closed(&mut self) -> Option<usize> {
        let tab = self.closed.pop()?;
        // Restore in the pane it came from when that pane still exists.
        let pane_index = self.focused.min(self.panes.len().saturating_sub(1));
        let pane = &mut self.panes[pane_index];
        Some(pane.push(tab))
    }

    /// Move a tab between panes, or reorder within one.
    ///
    /// Returns `false` when either index is out of range.
    pub fn move_tab(
        &mut self,
        from_pane: usize,
        from_index: usize,
        to_pane: usize,
        to_index: usize,
    ) -> bool {
        if from_pane >= self.panes.len() || to_pane >= self.panes.len() {
            return false;
        }
        if from_index >= self.panes[from_pane].tabs.len() {
            return false;
        }
        let tab = self.panes[from_pane].tabs.remove(from_index);
        let source_len = self.panes[from_pane].tabs.len();
        if from_pane == to_pane {
            if source_len == 0 {
                self.panes[from_pane].tabs.push(tab);
                self.panes[from_pane].active = 0;
                return true;
            }
            let index = to_index.min(source_len);
            self.panes[to_pane].tabs.insert(index, tab);
            self.panes[to_pane].active = index;
            return true;
        }
        let target_len = self.panes[to_pane].tabs.len();
        let index = to_index.min(target_len);
        self.panes[to_pane].tabs.insert(index, tab);
        self.panes[to_pane].active = index;
        if source_len == 0 {
            self.panes.remove(from_pane);
            self.focused = self.focused.min(self.panes.len() - 1);
        }
        true
    }

    /// Split the focused pane and move its visible tab into the new pane.
    ///
    /// Returns `false` when the pane limit is reached or there is no tab to
    /// move. A tab is moved rather than duplicated so the same note never has
    /// two diverging buffers.
    pub fn split(&mut self) -> bool {
        if self.panes.len() >= MAX_PANES {
            return false;
        }
        let source = self.focused.min(self.panes.len().saturating_sub(1));
        let Some(tab) = self.panes[source]
            .tabs
            .get(self.panes[source].active)
            .cloned()
        else {
            return false;
        };
        let index = self.panes[source].active;
        self.panes[source].close(index);
        let new_index = source + 1;
        let mut pane = Pane::new();
        pane.push(tab);
        self.panes.insert(new_index, pane);
        self.focused = new_index;
        true
    }

    /// Close the focused pane, keeping its tabs in the remaining pane.
    ///
    /// Returns `false` when there is only one pane.
    pub fn merge(&mut self) -> bool {
        if self.panes.len() < 2 {
            return false;
        }
        let index = self.focused.min(self.panes.len() - 1);
        let pane = self.panes.remove(index);
        let target = index.saturating_sub(1);
        for tab in pane.tabs {
            self.panes[target].tabs.push(tab);
        }
        self.focused = target;
        true
    }

    /// Rewrite tab paths after a rename inside the vault.
    pub fn rename_path(&mut self, from: &Path, to: &Path) {
        for pane in &mut self.panes {
            for tab in &mut pane.tabs {
                if tab.path == from {
                    tab.path = to.to_path_buf();
                } else if let Ok(relative) = tab.path.strip_prefix(from) {
                    tab.path = to.join(relative);
                }
            }
            pane.sanitize();
        }
    }

    /// Drop tabs for a deleted file or directory.
    pub fn remove_path(&mut self, path: &Path) {
        let mut removed = Vec::new();
        for pane in &mut self.panes {
            let mut index = 0;
            while index < pane.tabs.len() {
                if pane.tabs[index].path == path || pane.tabs[index].path.starts_with(path) {
                    removed.push(pane.tabs.remove(index));
                } else {
                    index += 1;
                }
            }
            pane.sanitize();
        }
        if self.panes.is_empty() {
            self.panes.push(Pane::new());
        }
        self.focused = self.focused.min(self.panes.len() - 1);
        self.remember_closed(removed);
    }

    /// Drop invalid tabs, clamp indices, and normalize the split ratio.
    pub fn sanitize(&mut self) {
        if self.panes.is_empty() {
            self.panes.push(Pane::new());
        }
        while self.panes.len() > MAX_PANES {
            let extra = self.panes.remove(MAX_PANES);
            if let Some(last) = self.panes.last_mut() {
                last.tabs.extend(extra.tabs);
            }
        }
        for pane in &mut self.panes {
            pane.sanitize();
        }
        self.focused = self.focused.min(self.panes.len() - 1);
        if !self.split_ratio.is_finite() {
            self.split_ratio = 0.5;
        }
        self.split_ratio = self.split_ratio.clamp(0.1, 0.9);
    }

    /// Path of the workspace file inside `vault_root`.
    pub fn file_path(vault_root: &Path) -> PathBuf {
        vault_root.join(WORKSPACE_DIR).join(WORKSPACE_FILE_NAME)
    }

    /// Load the workspace for `vault_root`, falling back to an empty one.
    ///
    /// Session restore must never block startup, so this swallows errors. Use
    /// [`Workspace::try_load`] when the caller wants to report them.
    pub fn load(vault_root: &Path) -> Self {
        Self::try_load(vault_root).unwrap_or_default()
    }

    /// Load the workspace for `vault_root`, reporting an unreadable file.
    ///
    /// A missing file is not an error: it is simply a first run.
    pub fn try_load(vault_root: &Path) -> Result<Self> {
        let path = Self::file_path(vault_root);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("read workspace {}", path.display()));
            }
        };
        let mut workspace: Self =
            toml::from_str(&raw).with_context(|| format!("parse workspace {}", path.display()))?;
        workspace.sanitize();
        Ok(workspace)
    }

    /// Persist the workspace for `vault_root`.
    ///
    /// The write is atomic so a crash mid-save cannot truncate the previous
    /// session.
    pub fn save(&self, vault_root: &Path) -> Result<()> {
        let path = Self::file_path(vault_root);
        let toml = self.to_toml().context("serialize workspace")?;
        crate::file_ops::write_atomic(&path, &toml)
            .with_context(|| format!("save workspace {}", path.display()))?;
        Ok(())
    }

    /// Serialize the workspace as TOML.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("serialize workspace to TOML")
    }

    /// Parse a workspace from TOML, sanitizing it.
    pub fn from_toml(raw: &str) -> Result<Self> {
        let mut workspace: Self = toml::from_str(raw).context("parse workspace TOML")?;
        workspace.sanitize();
        Ok(workspace)
    }

    fn remember_closed(&mut self, tabs: impl IntoIterator<Item = Tab>) {
        for tab in tabs {
            self.closed.push(tab);
        }
        let overflow = self.closed.len().saturating_sub(CLOSED_STACK_LIMIT);
        if overflow > 0 {
            self.closed.drain(..overflow);
        }
    }
}

/// Whether a tab path is vault-relative and safe to restore.
///
/// Absolute paths, parent traversals, and hidden bookkeeping paths are
/// rejected so a hand-edited `workspace.toml` cannot point the editor outside
/// the vault.
pub fn is_safe_tab_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return false;
    }
    path.components().all(|component| match component {
        Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(paths: &[&str]) -> Workspace {
        let mut workspace = Workspace::new();
        for path in paths {
            workspace.open(*path);
        }
        workspace
    }

    #[test]
    fn opening_a_note_adds_one_tab() {
        let workspace = ws(&["a.md", "b.md"]);
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_path(), Some(Path::new("b.md")));
    }

    #[test]
    fn opening_the_same_note_reuses_its_tab() {
        let mut workspace = ws(&["a.md", "b.md"]);
        workspace.open("a.md");
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_path(), Some(Path::new("a.md")));
    }

    #[test]
    fn opening_a_note_again_in_a_new_tab_is_allowed() {
        let mut workspace = ws(&["a.md"]);
        workspace.open_new("a.md");
        assert_eq!(workspace.tab_count(), 2);
    }

    #[test]
    fn closing_the_active_tab_selects_the_neighbour() {
        let mut workspace = ws(&["a.md", "b.md", "c.md"]);
        workspace.activate(1);
        let closed = workspace.close_active();
        assert_eq!(closed.map(|tab| tab.path), Some(PathBuf::from("b.md")));
        assert_eq!(workspace.active_path(), Some(Path::new("c.md")));
    }

    #[test]
    fn closing_the_last_tab_leaves_an_empty_pane() {
        let mut workspace = ws(&["a.md"]);
        workspace.close_active();
        assert!(workspace.is_empty());
        assert_eq!(workspace.panes.len(), 1);
    }

    #[test]
    fn reopening_restores_the_most_recent_tab() {
        let mut workspace = ws(&["a.md", "b.md"]);
        workspace.close_active();
        assert_eq!(workspace.reopen_closed(), Some(1));
        assert_eq!(workspace.active_path(), Some(Path::new("b.md")));
    }

    #[test]
    fn closed_stack_is_bounded() {
        let mut workspace = Workspace::new();
        for index in 0..CLOSED_STACK_LIMIT + 5 {
            workspace.open(format!("note-{index}.md"));
            workspace.close_active();
        }
        assert_eq!(workspace.closed.len(), CLOSED_STACK_LIMIT);
        assert_eq!(workspace.closed[0].path, PathBuf::from("note-5.md"));
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        let mut workspace = ws(&["a.md", "b.md", "c.md"]);
        workspace.cycle(1);
        assert_eq!(workspace.active_path(), Some(Path::new("a.md")));
        workspace.cycle(-1);
        assert_eq!(workspace.active_path(), Some(Path::new("c.md")));
    }

    #[test]
    fn close_others_keeps_only_the_active_tab() {
        let mut workspace = ws(&["a.md", "b.md", "c.md"]);
        workspace.activate(1);
        workspace.close_others();
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_path(), Some(Path::new("b.md")));
        assert_eq!(workspace.closed.len(), 2);
    }

    #[test]
    fn close_to_right_keeps_the_left_tabs() {
        let mut workspace = ws(&["a.md", "b.md", "c.md"]);
        workspace.activate(0);
        workspace.close_to_right();
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.closed.len(), 2);
    }

    #[test]
    fn splitting_moves_the_active_tab_into_a_second_pane() {
        let mut workspace = ws(&["a.md", "b.md"]);
        assert!(workspace.split());
        assert_eq!(workspace.panes.len(), 2);
        assert_eq!(workspace.panes[0].len(), 1);
        assert_eq!(
            workspace.panes[1].active_tab().map(Tab::title),
            Some("b.md".to_string())
        );
        assert_eq!(workspace.focused, 1);
        assert!(!workspace.split(), "only one split is supported");
    }

    #[test]
    fn splitting_keeps_unsaved_content_with_the_tab() {
        let mut workspace = ws(&["a.md"]);
        if let Some(tab) = workspace.active_tab_mut() {
            tab.unsaved = Some("# draft".to_string());
        }
        assert!(workspace.split());
        let moved = workspace
            .active_tab()
            .and_then(|tab| tab.unsaved.as_deref());
        assert_eq!(moved, Some("# draft"));
    }

    #[test]
    fn moving_a_tab_between_panes_closes_an_emptied_pane() {
        let mut workspace = ws(&["a.md", "b.md"]);
        assert!(workspace.split());
        assert!(workspace.move_tab(1, 0, 0, 0));
        assert_eq!(workspace.panes.len(), 1);
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.panes[0].active, 0);
    }

    #[test]
    fn moving_a_tab_within_a_pane_reorders_it() {
        let mut workspace = ws(&["a.md", "b.md", "c.md"]);
        assert!(workspace.move_tab(0, 2, 0, 0));
        let order: Vec<_> = workspace.panes[0]
            .tabs
            .iter()
            .map(|tab| tab.path.clone())
            .collect();
        assert_eq!(
            order,
            vec![
                PathBuf::from("c.md"),
                PathBuf::from("a.md"),
                PathBuf::from("b.md")
            ]
        );
    }

    #[test]
    fn merging_panes_keeps_every_tab() {
        let mut workspace = ws(&["a.md", "b.md"]);
        workspace.split();
        assert!(workspace.merge());
        assert_eq!(workspace.panes.len(), 1);
        assert_eq!(workspace.tab_count(), 2);
    }

    #[test]
    fn renaming_a_folder_rewrites_nested_tabs() {
        let mut workspace = ws(&["old/note.md", "keep.md"]);
        workspace.rename_path(Path::new("old"), Path::new("new"));
        assert_eq!(
            workspace.panes[0].tabs[0].path,
            PathBuf::from("new/note.md")
        );
        assert_eq!(workspace.panes[0].tabs[1].path, PathBuf::from("keep.md"));
    }

    #[test]
    fn deleting_a_folder_drops_its_tabs() {
        let mut workspace = ws(&["old/a.md", "old/b.md", "keep.md"]);
        workspace.remove_path(Path::new("old"));
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_path(), Some(Path::new("keep.md")));
        assert_eq!(workspace.closed.len(), 2);
    }

    #[test]
    fn tab_paths_must_stay_inside_the_vault() {
        assert!(is_safe_tab_path(Path::new("notes/a.md")));
        assert!(!is_safe_tab_path(Path::new("../escape.md")));
        assert!(!is_safe_tab_path(Path::new("/tmp/abs.md")));
        assert!(!is_safe_tab_path(Path::new(".weft/workspace.toml")));
        assert!(!is_safe_tab_path(Path::new("")));
    }

    #[test]
    fn sanitize_drops_unsafe_tabs_and_clamps_indices() {
        let raw = r#"
split_ratio = 4.0
focused = 9

[[panes]]
active = 7
tabs = [
  { path = "note.md" },
  { path = "../escape.md" },
]
"#;
        let workspace = Workspace::from_toml(raw).expect("parse workspace");
        assert_eq!(workspace.panes.len(), 1);
        assert_eq!(workspace.panes[0].tabs.len(), 1);
        assert_eq!(workspace.panes[0].active, 0);
        assert_eq!(workspace.focused, 0);
        assert!((workspace.split_ratio - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_workspace_parses_from_empty_toml() {
        let workspace = Workspace::from_toml("").expect("parse empty workspace");
        assert!(workspace.is_empty());
        assert_eq!(workspace.panes.len(), 1);
    }

    #[test]
    fn workspace_round_trips_without_unsaved_buffers() {
        let mut workspace = ws(&["a.md", "b.md"]);
        if let Some(tab) = workspace.active_tab_mut() {
            tab.unsaved = Some("draft".to_string());
            tab.cursor = Some(12);
            tab.scroll = 40.5;
            tab.mode = TabMode::Preview;
        }
        workspace.split();

        let toml = workspace.to_toml().expect("serialize");
        assert!(!toml.contains("draft"), "unsaved buffers are not persisted");

        let restored = Workspace::from_toml(&toml).expect("restore");
        assert_eq!(restored.panes.len(), workspace.panes.len());
        assert_eq!(restored.tab_count(), workspace.tab_count());
        assert_eq!(restored.active_path(), workspace.active_path());
        assert_eq!(restored.focused, workspace.focused);
        assert!((restored.split_ratio - workspace.split_ratio).abs() < f32::EPSILON);
        let tab = restored.active_tab().expect("active tab");
        assert_eq!(tab.cursor, Some(12));
        assert!((tab.scroll - 40.5).abs() < f32::EPSILON);
        assert_eq!(tab.mode, TabMode::Preview);
        assert!(tab.unsaved.is_none());
    }

    #[test]
    fn save_and_load_round_trip_through_the_vault() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut workspace = ws(&["notes/a.md", "notes/b.md"]);
        workspace.split();
        workspace.save(dir.path()).expect("save workspace");

        let path = Workspace::file_path(dir.path());
        assert!(path.exists(), "workspace file is written");
        assert!(path.starts_with(dir.path().join(WORKSPACE_DIR)));

        let restored = Workspace::load(dir.path());
        assert_eq!(restored.tab_count(), 2);
        assert_eq!(restored.panes.len(), 2);
    }

    #[test]
    fn load_of_missing_or_broken_file_yields_empty_workspace() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(Workspace::load(dir.path()).is_empty());

        let path = Workspace::file_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("make .weft");
        std::fs::write(&path, "this is not toml = = =").expect("write garbage");
        assert!(Workspace::load(dir.path()).is_empty());
        assert!(
            Workspace::try_load(dir.path()).is_err(),
            "try_load reports the unreadable file"
        );
    }
}
