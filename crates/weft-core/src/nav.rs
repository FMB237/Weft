//! Per-pane back/forward navigation.
//!
//! A pane remembers where the user has been, not just where they are. Each
//! entry carries the note path plus the caret position, so stepping back
//! lands in the same place in the text instead of at the top of the file.
//!
//! Like the workspace model this lives in `weft-core`: it is pure data, and
//! the rules (a new visit clears the forward stack, revisiting the current
//! note does not create a step) are worth testing without egui.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many steps a pane keeps in each direction before dropping the oldest.
pub const DEFAULT_CAPACITY: usize = 64;

/// One place the user visited.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntry {
    /// Vault-relative path of the note.
    pub path: PathBuf,
    /// Character offset of the caret when the entry was left.
    pub cursor: Option<usize>,
    /// Vertical scroll offset, in points.
    #[serde(default)]
    pub scroll: u32,
}

impl NavEntry {
    /// An entry for `path` at the start of the note.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            cursor: None,
            scroll: 0,
        }
    }

    /// An entry for `path` with a known caret position.
    pub fn at(path: impl Into<PathBuf>, cursor: Option<usize>) -> Self {
        Self {
            path: path.into(),
            cursor,
            scroll: 0,
        }
    }

    /// Whether this entry points at `path`.
    pub fn is(&self, path: &Path) -> bool {
        self.path == path
    }
}

/// A pane's back/forward stacks and its current entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavHistory {
    back: Vec<NavEntry>,
    forward: Vec<NavEntry>,
    current: Option<NavEntry>,
    capacity: usize,
}

impl NavHistory {
    /// An empty history with [`DEFAULT_CAPACITY`] steps per direction.
    pub fn new() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            ..Self::default()
        }
    }

    /// An empty history that keeps at most `capacity` steps per direction.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            ..Self::default()
        }
    }

    /// The entry the pane is showing.
    pub fn current(&self) -> Option<&NavEntry> {
        self.current.as_ref()
    }

    /// Number of steps available going back.
    pub fn back_len(&self) -> usize {
        self.back.len()
    }

    /// Number of steps available going forward.
    pub fn forward_len(&self) -> usize {
        self.forward.len()
    }

    /// Whether [`Self::back`] would move.
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Whether [`Self::forward`] would move.
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Record a visit to `entry`.
    ///
    /// Visiting the note already shown only updates its caret, so clicking the
    /// same note twice does not fill the history with duplicates. Any other
    /// visit clears the forward stack, as in a browser.
    pub fn visit(&mut self, entry: NavEntry) {
        if let Some(current) = &mut self.current
            && current.path == entry.path
        {
            // Same note: keep where the caret and scroll were unless the
            // caller is pointing at a specific place.
            if entry.cursor.is_some() {
                current.cursor = entry.cursor;
            }
            return;
        }
        if let Some(current) = self.current.take() {
            self.back.push(current);
            self.trim();
        }
        self.forward.clear();
        self.current = Some(entry);
    }

    /// Update the caret and scroll of the current entry in place.
    ///
    /// This is for "the user moved the caret", not "the user went somewhere",
    /// so it never adds a step.
    pub fn update_current(&mut self, cursor: Option<usize>, scroll: u32) {
        if let Some(current) = &mut self.current {
            current.cursor = cursor;
            current.scroll = scroll;
        }
    }

    /// Step back and return the entry to show.
    pub fn back(&mut self) -> Option<NavEntry> {
        let target = self.back.pop()?;
        if let Some(current) = self.current.replace(target) {
            self.forward.push(current);
            self.trim_forward();
        }
        self.current.clone()
    }

    /// Step forward and return the entry to show.
    pub fn forward(&mut self) -> Option<NavEntry> {
        let target = self.forward.pop()?;
        if let Some(current) = self.current.replace(target) {
            self.back.push(current);
            self.trim();
        }
        self.current.clone()
    }

    /// Drop every step, keeping only the current entry.
    pub fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
    }

    /// Forget entries that point at `path`, e.g. after a delete.
    pub fn forget(&mut self, path: &Path) {
        self.back.retain(|entry| !entry.is(path));
        self.forward.retain(|entry| !entry.is(path));
        if self.current.as_ref().is_some_and(|entry| entry.is(path)) {
            self.current = None;
        }
    }

    /// Forget everything under `prefix`, e.g. after a folder delete.
    pub fn forget_under(&mut self, prefix: &Path) {
        let stale = |entry: &NavEntry| entry.path.starts_with(prefix);
        self.back.retain(|entry| !stale(entry));
        self.forward.retain(|entry| !stale(entry));
        if self.current.as_ref().is_some_and(stale) {
            self.current = None;
        }
    }

    /// Rewrite `from` to `to` after a rename, in every direction.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        let rewrite = |entry: &mut NavEntry| {
            if entry.path == from {
                entry.path = to.to_path_buf();
            } else if let Ok(suffix) = entry.path.strip_prefix(from) {
                entry.path = to.join(suffix);
            }
        };
        for entry in &mut self.back {
            rewrite(entry);
        }
        for entry in &mut self.forward {
            rewrite(entry);
        }
        if let Some(current) = &mut self.current {
            rewrite(current);
        }
    }

    fn trim(&mut self) {
        if self.back.len() > self.capacity {
            let excess = self.back.len() - self.capacity;
            self.back.drain(0..excess);
        }
    }

    fn trim_forward(&mut self) {
        if self.forward.len() > self.capacity {
            let excess = self.forward.len() - self.capacity;
            self.forward.drain(0..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(history: &NavHistory) -> Vec<String> {
        history
            .back
            .iter()
            .chain(history.current.iter())
            .chain(history.forward.iter().rev())
            .map(|entry| entry.path.display().to_string())
            .collect()
    }

    #[test]
    fn back_walks_the_visit_stack_and_forward_returns() {
        let mut history = NavHistory::new();
        for name in ["a.md", "b.md", "c.md"] {
            history.visit(NavEntry::new(name));
        }
        assert!(history.can_go_back());
        assert!(!history.can_go_forward());

        assert_eq!(history.back().expect("back").path, PathBuf::from("b.md"));
        assert_eq!(history.back().expect("back").path, PathBuf::from("a.md"));
        assert!(history.back().is_none(), "the first visit is the end");

        assert_eq!(
            history.forward().expect("forward").path,
            PathBuf::from("b.md")
        );
        assert_eq!(
            history.forward().expect("forward").path,
            PathBuf::from("c.md")
        );
        assert!(history.forward().is_none());
    }

    #[test]
    fn a_new_visit_clears_the_forward_stack() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.visit(NavEntry::new("b.md"));
        history.back();
        assert!(history.can_go_forward());

        history.visit(NavEntry::new("c.md"));

        assert!(!history.can_go_forward());
        assert_eq!(entries(&history), ["a.md", "c.md"]);
    }

    #[test]
    fn revisiting_the_current_note_only_updates_the_caret() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.update_current(Some(12), 40);
        history.visit(NavEntry::at("a.md", Some(30)));

        assert_eq!(history.back_len(), 0);
        let current = history.current().expect("current");
        assert_eq!(current.cursor, Some(30));
        assert_eq!(current.scroll, 40, "scroll survives a caret-only revisit");
    }

    #[test]
    fn update_current_never_adds_a_step() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.update_current(Some(5), 10);

        assert_eq!(history.back_len(), 0);
        assert_eq!(history.forward_len(), 0);
        assert_eq!(history.current().expect("current").cursor, Some(5));
    }

    #[test]
    fn capacity_drops_the_oldest_steps() {
        let mut history = NavHistory::with_capacity(2);
        for name in ["a.md", "b.md", "c.md", "d.md"] {
            history.visit(NavEntry::new(name));
        }
        assert_eq!(history.back_len(), 2);
        assert_eq!(history.back().expect("back").path, PathBuf::from("c.md"));
        assert_eq!(history.back().expect("back").path, PathBuf::from("b.md"));
        assert!(history.back().is_none(), "a.md was dropped");
    }

    #[test]
    fn deleting_a_note_forgets_its_entries() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.visit(NavEntry::new("b.md"));
        history.visit(NavEntry::new("a.md"));

        history.forget(Path::new("a.md"));

        assert!(history.current().is_none());
        assert_eq!(history.back_len(), 1, "the visit to b.md survives");
        assert_eq!(history.back().expect("back").path, PathBuf::from("b.md"));
        assert!(history.back().is_none());
        assert_eq!(history.forward_len(), 0);
    }

    #[test]
    fn deleting_a_folder_forgets_entries_under_it() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("daily/one.md"));
        history.visit(NavEntry::new("daily/two.md"));
        history.visit(NavEntry::new("notes/keep.md"));

        history.forget_under(Path::new("daily"));

        assert_eq!(entries(&history), ["notes/keep.md"]);
    }

    #[test]
    fn renaming_rewrites_every_entry() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.visit(NavEntry::new("b.md"));
        history.visit(NavEntry::new("a.md"));

        history.rename(Path::new("a.md"), Path::new("z.md"));

        assert_eq!(entries(&history), ["z.md", "b.md", "z.md"]);
    }

    #[test]
    fn renaming_a_folder_rewrites_entries_under_it() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("old/one.md"));

        history.rename(Path::new("old"), Path::new("new"));

        assert_eq!(entries(&history), ["new/one.md"]);
    }

    #[test]
    fn clear_keeps_only_the_current_entry() {
        let mut history = NavHistory::new();
        history.visit(NavEntry::new("a.md"));
        history.visit(NavEntry::new("b.md"));

        history.clear();

        assert_eq!(entries(&history), ["b.md"]);
    }
}
