//! Main eframe application.
//!
//! Handles app lifecycle: persistence (save/load vault state, window
// position, layout), theme, panels, and wiring between the GUI and
//! weft-core.

use std::path::{Component, Path, PathBuf};

use weft_core::export::{export_document, export_vault_bundle};
use weft_core::find::{self, Match as FindMatch};
use weft_core::fuzzy::{self, Ranked};
use weft_core::graph::LinkGraph;
use weft_core::graph_updater::GraphUpdater;
use weft_core::history::{DEFAULT_RETENTION, HistoryStore, Snapshot};
use weft_core::index::SearchIndex;
use weft_core::layout_store::LayoutStore;
use weft_core::nav::{NavEntry, NavHistory};

use crate::widgets::EditorMetrics;
use weft_core::vault::Vault;
use weft_core::watcher::{WatchEvent, WatchHandle};
use weft_core::workspace::Workspace;

use eframe::Storage;
use egui::{CentralPanel, Key, SidePanel, TopBottomPanel};

use crate::event_bus::{Event, EventBus};
use crate::panels::{PanelId, Panels};
use crate::settings::{Settings, SettingsPanel};
use crate::theme::palette;
use crate::widgets::chrome::{
    TabStripAction, badge, document_tabs, icon_button, status_bar, tab_bar, toast,
};
use weft_core::plugins::{PluginEvent, PluginManager};

/// Persisted application state saved between sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppState {
    /// Last opened vault path.
    pub vault_path: Option<PathBuf>,
    /// Last opened document (relative to the vault root).
    pub open_document: Option<PathBuf>,
    /// Window size/position hints.
    pub window_rect: Option<[f32; 4]>,
    /// Last active panel.
    pub active_panel: PanelId,
    /// Editor/preview split ratio (0.0 = all editor, 1.0 = all preview).
    pub editor_split_ratio: f32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            vault_path: None,
            open_document: None,
            window_rect: None,
            active_panel: PanelId::default(),
            editor_split_ratio: 0.5,
        }
    }
}

impl AppState {
    const STORAGE_KEY: &'static str = "editor_app_state";

    pub fn load(storage: Option<&dyn Storage>) -> Self {
        let Some(raw) = storage.and_then(|s| s.get_string(Self::STORAGE_KEY)) else {
            return Self::default();
        };
        match serde_json::from_str::<AppState>(&raw) {
            Ok(state) => state,
            Err(error) => {
                // A stale or hand-edited session must not fail silently: the
                // default state has no vault, which looks like data loss.
                eprintln!("Ignoring unreadable saved session ({error}); starting fresh.");
                Self::default()
            }
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) {
        if let Ok(json) = serde_json::to_string(self) {
            storage.set_string(Self::STORAGE_KEY, json);
        }
    }
}

/// The main editor application.
#[expect(
    clippy::struct_excessive_bools,
    reason = "UI flags are independent toggles, not a single state enum"
)]
pub struct EditorApp {
    /// Persisted app state.
    pub(crate) state: AppState,
    /// Currently open vault.
    pub(crate) vault: Option<Vault>,
    /// Open panes and tabs, and where the session left off.
    pub(crate) workspace: Workspace,
    /// Currently open document (relative path within vault).
    pub(crate) open_document: Option<PathBuf>,
    /// Document content being edited.
    pub(crate) document_content: String,
    /// Whether the current document has unsaved changes.
    pub(crate) dirty: bool,
    /// Link graph of all vault documents.
    pub(crate) graph: LinkGraph,
    /// Layout store for node positions.
    pub(crate) layout_store: LayoutStore,
    /// Cross-panel event bus.
    pub(crate) event_bus: EventBus,
    /// UI panels.
    pub(crate) panels: Panels,
    /// Last save timestamp for auto-save.
    last_auto_save: std::time::Instant,

    /// Open export dialog modal state, if any.
    export_state: Option<ExportState>,

    /// Persisted user preferences.
    pub(crate) settings: Settings,
    /// Whether the preferences dialog is open.
    pub(crate) settings_open: bool,
    /// Preferences panel widget state.
    pub(crate) settings_panel: SettingsPanel,
    /// Plugin manager for extending editor functionality.
    pub(crate) plugins: PluginManager,
    /// Open vault dialog state.
    pub(crate) vault_open_dialog_open: bool,
    /// Path input for the open vault dialog.
    pub(crate) vault_open_path: String,
    /// Split ratio between editor and preview (0.0 = all editor, 1.0 = all preview).
    pub(crate) editor_split_ratio: f32,
    /// Whether the in-note find bar is open.
    find_open: bool,
    /// Find query.
    find_query: String,
    /// Replacement text.
    find_replacement: String,
    /// Whether the find bar matches case exactly.
    find_match_case: bool,
    /// Current matches for the query.
    find_hits: Vec<FindMatch>,
    /// Index into `find_hits` of the highlighted match.
    find_index: usize,
    /// Whether the query field should take focus this frame.
    find_focus: bool,
    /// Text range (character indices) to select in the editor.
    pending_selection: Option<(usize, usize)>,
    /// Whether the editor should take keyboard focus this frame, so the
    /// selected match is visibly highlighted.
    focus_editor: bool,
    /// Whether the quick switcher is open.
    quick_open: bool,
    /// Quick switcher query.
    quick_input: String,
    /// Notes to search, open tabs first.
    quick_pool: Vec<PathBuf>,
    /// Ranked match for the current query.
    quick_results: Vec<Ranked>,
    /// Highlighted result.
    quick_selected: usize,
    /// Whether the command palette is open.
    pub(crate) command_palette_open: bool,
    /// Command palette input.
    pub(crate) command_palette_input: String,
    /// Action waiting on the user to resolve unsaved changes.
    pending_action: Option<PendingAction>,
    /// Last title sent to the window manager, to avoid redundant commands.
    last_window_title: String,
    /// Full-text search index for the open vault.
    search_index: Option<SearchIndex>,
    /// Background filesystem watcher for the open vault.
    watcher: Option<WatchHandle>,
    /// Per-pane back/forward stacks, indexed like `workspace.panes`.
    nav: Vec<NavHistory>,
    /// Caret to place when the active buffer is next rendered.
    restore_cursor: Option<usize>,
    /// Vertical scroll to apply when the active buffer is next rendered.
    restore_scroll: Option<f32>,
    /// Caret and scroll reported by the last editor frame.
    editor_metrics: Option<EditorMetrics>,
    /// Save snapshots for the open vault.
    history: Option<HistoryStore>,
    /// Whether the file-recovery dialog is open.
    recovery_open: bool,
    /// Snapshots listed in the recovery dialog, newest first.
    recovery_snapshots: Vec<Snapshot>,
    /// Index of the selected recovery snapshot.
    recovery_selected: Option<usize>,
    /// Contents of the selected snapshot, shown as a preview.
    recovery_preview: String,
    /// Whether the search panel is visible.
    search_open: bool,
    /// Notice shown when a file changed on disk while the buffer was dirty.
    external_change: Option<String>,
    /// Whether the currently applied style is dark, for system-theme tracking.
    theme_is_dark: bool,
}

impl std::fmt::Debug for EditorApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorApp")
            .field("vault", &self.vault)
            .field("open_document", &self.open_document)
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

impl EditorApp {
    /// Create a new app instance, loading persisted state from storage.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::load(cc.storage);
        let settings = Settings::load(cc.storage);
        crate::theme::install_fonts(&cc.egui_ctx);
        settings.theme.apply(&cc.egui_ctx);
        let theme_is_dark = settings.theme.is_dark(&cc.egui_ctx);

        let mut panels = Panels::new();
        panels.set_active(state.active_panel);
        let initial_ratio = state.editor_split_ratio;

        let mut app = Self {
            state,
            vault: None,
            workspace: Workspace::new(),
            open_document: None,
            document_content: String::new(),
            dirty: false,
            graph: LinkGraph::new(),
            layout_store: LayoutStore::new(),
            event_bus: EventBus::new(),
            panels,
            last_auto_save: std::time::Instant::now(),
            export_state: None,
            settings,
            settings_open: false,
            settings_panel: SettingsPanel::default(),
            plugins: PluginManager::new(),
            vault_open_dialog_open: false,
            vault_open_path: String::new(),
            editor_split_ratio: initial_ratio,
            find_open: false,
            find_query: String::new(),
            find_replacement: String::new(),
            find_match_case: false,
            find_hits: Vec::new(),
            find_index: 0,
            find_focus: false,
            pending_selection: None,
            focus_editor: false,
            quick_open: false,
            quick_input: String::new(),
            quick_pool: Vec::new(),
            quick_results: Vec::new(),
            quick_selected: 0,
            command_palette_open: false,
            command_palette_input: String::new(),
            pending_action: None,
            last_window_title: String::new(),
            search_index: None,
            watcher: None,
            nav: vec![NavHistory::new(), NavHistory::new()],
            restore_cursor: None,
            restore_scroll: None,
            editor_metrics: None,
            history: None,
            recovery_open: false,
            recovery_snapshots: Vec::new(),
            recovery_selected: None,
            recovery_preview: String::new(),
            search_open: false,
            external_change: None,
            theme_is_dark,
        };

        // Re-open the last vault and its workspace, if they still exist. The
        // workspace file is authoritative; `state.open_document` is only a
        // fallback for sessions saved before tabs existed.
        if let Some(vault_path) = app.state.vault_path.clone()
            && vault_path.exists()
        {
            app.open_vault(&vault_path);
        }

        app
    }

    /// Open a vault at the given path.
    pub(crate) fn open_vault(&mut self, path: &Path) {
        // Persist the outgoing session before switching vaults.
        self.save_workspace();
        match Vault::open(path) {
            Ok(vault) => {
                self.state.vault_path = Some(path.to_path_buf());
                self.history = Some(HistoryStore::new(path));
                self.vault = Some(vault);
                self.layout_store = self
                    .layout_path()
                    .and_then(|p| LayoutStore::load_from_file(&p).ok())
                    .unwrap_or_default();
                self.open_document = None;
                self.document_content.clear();
                self.dirty = false;
                self.last_auto_save = std::time::Instant::now();
                self.restore_workspace(path);
                self.panels.graph.reset();
                self.watcher = WatchHandle::start(path).ok();
                self.rebuild_search_index();
                self.panels.search.invalidate();
                self.rebuild_graph();
                let _ = self
                    .plugins
                    .dispatch(&PluginEvent::VaultOpened(path.to_path_buf()));
            }
            Err(e) => {
                eprintln!("Failed to open vault at {}: {e}", path.display());
            }
        }
    }

    /// Load the saved workspace for `root` and show its active tab.
    ///
    /// A corrupt file is reported in the conflict banner instead of silently
    /// discarding the user's session.
    fn restore_workspace(&mut self, root: &Path) {
        let workspace = match Workspace::try_load(root) {
            Ok(workspace) => workspace,
            Err(error) => {
                self.external_change = Some(format!("Could not restore the workspace: {error}"));
                Workspace::new()
            }
        };
        self.workspace = workspace;

        // Sessions written before tabs existed have only `open_document`.
        if self.workspace.is_empty()
            && let Some(doc) = self.state.open_document.clone()
            && root.join(&doc).is_file()
        {
            self.workspace.open(doc);
        }
        self.load_active_tab();
    }

    /// Persist panes, tabs, and view state for the current vault.
    pub(crate) fn save_workspace(&mut self) {
        self.stash_active_tab();
        let Some(vault) = &self.vault else { return };
        if let Err(error) = self.workspace.save(&vault.root) {
            eprintln!("Failed to save the workspace: {error}");
        }
    }

    /// Copy the live editor buffer into the active tab.
    ///
    /// Tabs own their unsaved buffers so switching away from a dirty note does
    /// not discard edits, and so closing a tab can still offer to save it.
    fn stash_active_tab(&mut self) {
        let dirty = self.dirty;
        let content = std::mem::take(&mut self.document_content);
        if let Some(tab) = self.workspace.active_tab_mut() {
            tab.unsaved = if dirty { Some(content.clone()) } else { None };
        }
        self.document_content = content;
    }

    /// Load the active tab's buffer into the editor fields.
    ///
    /// Use this for a user-visible move between notes: it records a navigation
    /// step. Code that is already moving through history calls
    /// [`Self::load_buffer`] instead, so back/forward does not push new steps.
    fn load_active_tab(&mut self) {
        self.load_buffer();
        self.record_nav_visit();
    }

    /// Load the active tab's buffer into the editor fields, without touching
    /// the navigation stacks.
    fn load_buffer(&mut self) {
        self.dirty = false;
        self.document_content.clear();
        self.open_document = None;
        self.restore_cursor = None;
        self.restore_scroll = None;
        self.last_auto_save = std::time::Instant::now();

        let Some(tab) = self.workspace.active_tab() else {
            return;
        };
        self.open_document = Some(tab.path.clone());
        self.restore_cursor = tab.cursor;
        if tab.scroll > 0.0 {
            self.restore_scroll = Some(tab.scroll);
        }
        if let Some(buffer) = &tab.unsaved {
            self.document_content = buffer.clone();
            self.dirty = true;
            return;
        }
        let Some(vault) = &self.vault else { return };
        match vault.read(&tab.path) {
            Ok(content) => self.document_content = content,
            Err(error) => {
                self.external_change =
                    Some(format!("Could not read {}: {error}", tab.path.display()));
            }
        }
    }

    /// Push the visible note onto the focused pane's history stack.
    fn record_nav_visit(&mut self) {
        let Some(path) = self.open_document.clone() else {
            return;
        };
        let pane = self.workspace.focused;
        if let Some(history) = self.nav.get_mut(pane) {
            history.update_current(self.restore_cursor, 0);
            history.visit(NavEntry::at(path, self.restore_cursor));
        }
    }

    /// Step back or forward in the focused pane's history.
    fn navigate(&mut self, forward: bool) {
        let pane = self.workspace.focused;
        let Some(history) = self.nav.get_mut(pane) else {
            return;
        };
        let target = if forward {
            history.forward()
        } else {
            history.back()
        };
        let Some(target) = target else {
            return;
        };
        self.stash_active_tab();
        self.workspace.focused = pane;
        self.workspace.open(target.path.clone());
        self.load_buffer();
        if target.cursor.is_some() {
            self.restore_cursor = target.cursor;
        }
        if target.scroll > 0 {
            self.restore_scroll = Some(target.scroll as f32);
        }
        self.record_nav_position();
    }

    /// Store the caret and scroll of the visible note in its tab and its
    /// history entry.
    fn record_nav_position(&mut self) {
        let Some(metrics) = self.editor_metrics else {
            return;
        };
        if let Some(tab) = self.workspace.active_tab_mut() {
            tab.cursor = metrics.cursor;
            tab.scroll = metrics.scroll;
        }
        let pane = self.workspace.focused;
        if let Some(history) = self.nav.get_mut(pane) {
            history.update_current(metrics.cursor, metrics.scroll_points());
        }
    }

    /// Switch to a tab in a pane, preserving the buffer of the tab left behind.
    fn switch_to_tab(&mut self, pane: usize, index: usize) {
        let Some(target) = self.workspace.panes.get(pane) else {
            return;
        };
        if pane == self.workspace.focused && target.active == index {
            return;
        }
        self.stash_active_tab();
        self.workspace.focused = pane;
        self.workspace.activate(index);
        self.load_active_tab();
    }

    /// Path of the persisted graph layout inside the current vault.
    fn layout_path(&self) -> Option<PathBuf> {
        self.vault
            .as_ref()
            .map(|vault| vault.root.join(".editor").join("layout.json"))
    }

    /// Persist graph node positions for the current vault.
    pub(crate) fn save_layout(&self) {
        let Some(path) = self.layout_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = self.layout_store.save_to_file(&path) {
            eprintln!("Failed to save graph layout at {}: {e}", path.display());
        }
    }

    /// Rebuild the link graph from the current vault's files.
    ///
    /// Every wikilink becomes a directed weighted edge through
    /// [`GraphUpdater`], so the graph reflects the vault instead of only its
    /// node list.
    pub(crate) fn rebuild_graph(&mut self) {
        let Some(vault) = &self.vault else { return };

        let mut graph = LinkGraph::new();
        {
            let mut updater = GraphUpdater::new(&mut graph, vault);
            let _ = updater.rebuild();
        }
        self.layout_store.apply_to_graph(&mut graph);
        self.graph = graph;
    }

    /// Rebuild the full-text index from the current vault.
    pub(crate) fn rebuild_search_index(&mut self) {
        let Some(vault) = &self.vault else {
            self.search_index = None;
            return;
        };

        let Ok(mut index) = SearchIndex::new() else {
            self.search_index = None;
            return;
        };
        if let Ok(files) = vault.list_files() {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let _ = index.add_document(&file, &content);
                }
            }
        }
        self.search_index = Some(index);
    }

    /// Open a document in a tab of the focused pane.
    pub(crate) fn open_document(&mut self, path: &Path) {
        let Some(vault) = &self.vault else { return };
        let relative = vault_relative(&vault.root, path);
        self.stash_active_tab();
        self.workspace.open(relative);
        self.load_active_tab();
        let _ = self
            .plugins
            .dispatch(&PluginEvent::DocumentOpened(path.to_path_buf()));
    }

    /// Open a document. Tabs make switching lossless, so there is nothing to
    /// confirm: a dirty note keeps its buffer in its own tab.
    pub(crate) fn request_open_document(&mut self, path: &Path) {
        self.open_document(path);
    }

    /// Close the current document, asking the user first when it is dirty.
    pub(crate) fn request_close_document(&mut self) {
        if self.open_document.is_none() {
            return;
        }
        if self.dirty {
            self.pending_action = Some(PendingAction::CloseDocument);
        } else {
            self.close_document_now();
        }
    }

    /// Close the active tab, activating the neighbouring one.
    fn close_document_now(&mut self) {
        let pane = self.workspace.focused;
        let index = self.workspace.focused_pane().active;
        self.workspace.close_tab(pane, index);
        self.load_active_tab();
    }

    /// Save the current document to the vault.
    ///
    /// The editor buffer is the exact file contents (including frontmatter),
    /// so it is written back verbatim. This preserves formatting and unknown
    /// frontmatter keys instead of round-tripping through a serializer.
    pub(crate) fn save_document(&mut self) {
        let Some(vault) = &self.vault else { return };
        let Some(path) = self.open_document.clone() else {
            return;
        };

        // Snapshot what is on disk now, before the new buffer replaces it.
        // Snapshot failures are derived-data failures: they must not block the
        // save the user asked for.
        if let Ok(previous) = vault.read(&path)
            && let Some(history) = &self.history
        {
            let _ = history.record_with_retention(&path, &previous, DEFAULT_RETENTION);
        }

        if vault.write(&path, &self.document_content).is_ok() {
            self.dirty = false;
            if let Some(tab) = self.workspace.active_tab_mut() {
                tab.unsaved = None;
            }
            self.last_auto_save = std::time::Instant::now();
            if let Some(index) = &mut self.search_index {
                let _ = index.add_document(&path, &self.document_content);
            }
            self.event_bus.publish(Event::DocumentSaved(path.clone()));
            let _ = self.plugins.dispatch(&PluginEvent::DocumentSaved(path));
            self.rebuild_graph();
        }
    }

    /// Create a missing note at a vault-relative path (clicked graph
    /// placeholder), index it, and open it.
    pub(crate) fn create_note_at(&mut self, path: &Path) {
        let Some(vault) = &self.vault else { return };
        if !is_safe_note_path(path) {
            self.external_change = Some(format!(
                "Refusing to create a note outside the vault: {}",
                path.display()
            ));
            return;
        }

        // Clicking a placeholder that now exists should just open it.
        if vault.read(path).is_ok() {
            self.request_open_document(path);
            return;
        }

        if vault.write(path, "").is_ok() {
            if let Some(index) = &mut self.search_index {
                let _ = index.add_document(path, "");
            }
            self.rebuild_graph();
            self.request_open_document(path);
        }
    }

    /// Create a new note in the vault root and open it.
    pub(crate) fn new_note(&mut self) {
        let Some(vault) = &self.vault else { return };
        let path = unique_child_path(vault, Path::new(""), "Untitled", "md");
        if vault.write(&path, "").is_ok() {
            if let Some(index) = &mut self.search_index {
                let _ = index.add_document(&path, "");
            }
            self.rebuild_graph();
            self.request_open_document(&path);
        }
    }

    /// Auto-save the current document if dirty and enough time has passed.
    pub(crate) fn maybe_auto_save(&mut self) {
        if !self.dirty {
            return;
        }
        let interval = self.settings.auto_save_interval_secs;
        if interval == 0 || self.last_auto_save.elapsed().as_secs() < interval {
            return;
        }
        self.save_document();
        self.last_auto_save = std::time::Instant::now();
    }

    /// Process pending events from the cross-panel event bus.
    ///
    /// This runs once per frame after panel rendering, so events published
    /// during this frame's panel `show` calls are dispatched before the next
    /// frame.
    pub(crate) fn process_events(&mut self, ctx: &egui::Context) {
        for event in self.event_bus.drain() {
            match event {
                Event::NodeSelected(path) | Event::NodeOpened(path) => {
                    self.request_open_document(&path);
                }
                Event::CreateNote(path) => {
                    self.create_note_at(&path);
                }
                Event::GraphChanged | Event::VaultChanged => {
                    self.rebuild_graph();
                }
                Event::EntryRenamed(from, to) => {
                    // Keep the live buffer: a rename must not discard edits or
                    // re-read the file from disk.
                    self.workspace.rename_path(&from, &to);
                    for history in &mut self.nav {
                        history.rename(&from, &to);
                        history.update_current(
                            self.editor_metrics.and_then(|m| m.cursor),
                            self.editor_metrics.map_or(0, |m| m.scroll_points()),
                        );
                    }
                    self.open_document = self.workspace.active_path().map(Path::to_path_buf);
                    self.rebuild_search_index();
                    self.panels.search.invalidate();
                    self.rebuild_graph();
                }
                Event::EntryDeleted(path) => {
                    self.stash_active_tab();
                    let keep = self
                        .dirty
                        .then(|| (self.workspace.focused, self.workspace.focused_pane().active));
                    self.workspace.remove_path_keeping(&path, keep);
                    for history in &mut self.nav {
                        history.forget_under(&path);
                        history.forget(&path);
                    }
                    self.load_active_tab();
                    self.rebuild_search_index();
                    self.panels.search.invalidate();
                    self.rebuild_graph();
                }
                Event::DocumentSaved(_path) => {
                    // Document already saved by the panel; mark not dirty.
                    self.dirty = false;
                }
                Event::ThemeChanged => {
                    self.settings.theme.apply(ctx);
                    self.theme_is_dark = self.settings.theme.is_dark(ctx);
                }
            }
        }
    }

    /// Poll the background watcher and apply any events that arrived.
    pub(crate) fn poll_watcher(&mut self) {
        let mut events = Vec::new();
        if let Some(watcher) = &self.watcher {
            while let Some(event) = watcher.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.apply_watch_event(event);
        }
    }

    /// Apply a single filesystem change without clobbering a dirty buffer.
    fn apply_watch_event(&mut self, event: WatchEvent) {
        self.apply_graph_event(&event);
        match event {
            WatchEvent::Created(path) | WatchEvent::Modified(path) => {
                self.index_path(&path);
                if self.open_document.as_deref() == Some(path.as_path()) {
                    if self.dirty {
                        self.external_change = Some(format!(
                            "{} changed on disk; your unsaved edits were kept.",
                            path.display()
                        ));
                    } else if let Some(vault) = &self.vault
                        && let Ok(content) = vault.read(&path)
                        && content != self.document_content
                    {
                        self.document_content = content;
                    }
                }
                self.panels.search.invalidate();
            }
            WatchEvent::Deleted(path) => {
                if let Some(index) = &mut self.search_index {
                    let _ = index.remove_document(&path);
                }
                self.stash_active_tab();
                let active_deleted = self
                    .open_document
                    .as_deref()
                    .is_some_and(|open| open == path || open.starts_with(&path));
                let keep = (active_deleted && self.dirty)
                    .then(|| (self.workspace.focused, self.workspace.focused_pane().active));
                if keep.is_some() {
                    self.external_change = Some(format!(
                        "{} was deleted on disk; your buffer was kept.",
                        path.display()
                    ));
                }
                self.workspace.remove_path_keeping(&path, keep);
                self.load_active_tab();
                self.panels.search.invalidate();
            }
            WatchEvent::Renamed(from, to) => {
                self.rebuild_search_index();
                self.panels.search.invalidate();
                self.workspace.rename_path(&from, &to);
                self.open_document = self.workspace.active_path().map(Path::to_path_buf);
            }
        }
    }

    /// Keep the graph in sync with a filesystem event.
    ///
    /// Created/modified files are applied incrementally through
    /// [`GraphUpdater`]; deletions and renames rebuild so links from other
    /// notes fall back to unresolved placeholders.
    fn apply_graph_event(&mut self, event: &WatchEvent) {
        if matches!(event, WatchEvent::Created(_) | WatchEvent::Modified(_)) {
            let Some(vault) = &self.vault else { return };
            let mut updater = GraphUpdater::new(&mut self.graph, vault);
            let _ = updater.apply_event(event);
        } else {
            self.rebuild_graph();
        }
    }

    /// Index a single file into the search index, ignoring read failures.
    fn index_path(&mut self, path: &Path) {
        let Some(vault) = &self.vault else { return };
        let Ok(content) = vault.read(path) else {
            return;
        };
        if let Some(index) = &mut self.search_index {
            let _ = index.add_document(path, &content);
        }
    }

    /// Handle global keyboard shortcuts.
    pub(crate) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let input = ctx.input(std::clone::Clone::clone);
        let modifiers = input.modifiers;
        self.handle_history_input(&input, ctx);

        if input.key_pressed(Key::Escape) && self.find_open {
            self.find_open = false;
            self.pending_selection = None;
            ctx.request_repaint();
        }

        let command = modifiers.mac_cmd || modifiers.ctrl;
        if !command {
            return;
        }

        if modifiers.shift {
            if input.key_pressed(Key::F) {
                self.search_open = !self.search_open;
                if self.search_open {
                    self.panels.search.focus();
                }
                ctx.request_repaint();
            }
            if input.key_pressed(Key::O) {
                self.vault_open_dialog_open = true;
                ctx.request_repaint();
            }
            if input.key_pressed(Key::P) {
                if let Some(doc) = &self.open_document {
                    self.export_state = Some(ExportState::Document {
                        doc_path: doc.clone(),
                        output: String::new(),
                    });
                }
                ctx.request_repaint();
            }
            if input.key_pressed(Key::B) {
                self.export_state = Some(ExportState::Vault {
                    output: String::new(),
                });
                ctx.request_repaint();
            }
            if input.key_pressed(Key::T) {
                if self.workspace.reopen_closed().is_some() {
                    self.load_active_tab();
                }
                ctx.request_repaint();
            }
            if input.key_pressed(Key::Tab) {
                self.cycle_tab(-1);
                ctx.request_repaint();
            }
            return;
        }

        if input.key_pressed(Key::S) {
            self.save_document();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::O) {
            self.open_quick_switcher();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::N) {
            self.new_note();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::F) {
            self.open_find_bar();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::P) {
            self.command_palette_open = !self.command_palette_open;
            self.command_palette_input.clear();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::G) {
            self.panels.active = match self.panels.active {
                PanelId::Graph => PanelId::EditorSplit,
                _ => PanelId::Graph,
            };
            ctx.request_repaint();
        }
        if input.key_pressed(Key::W) {
            self.request_close_document();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::T) {
            self.new_note();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::Tab) {
            self.cycle_tab(1);
            ctx.request_repaint();
        }
    }

    /// Back/forward on Alt+arrows and the mouse back/forward buttons.
    fn handle_history_input(&mut self, input: &egui::InputState, ctx: &egui::Context) {
        let modifiers = input.modifiers;
        if modifiers.alt && !modifiers.ctrl && !modifiers.command {
            if input.key_pressed(Key::ArrowLeft) {
                self.navigate(false);
                ctx.request_repaint();
            }
            if input.key_pressed(Key::ArrowRight) {
                self.navigate(true);
                ctx.request_repaint();
            }
        }
        if input.pointer.button_pressed(egui::PointerButton::Extra1) {
            self.navigate(false);
            ctx.request_repaint();
        }
        if input.pointer.button_pressed(egui::PointerButton::Extra2) {
            self.navigate(true);
            ctx.request_repaint();
        }
    }

    /// Switch between the tabs of the focused pane.
    fn cycle_tab(&mut self, delta: isize) {
        if self.workspace.tab_count() < 2 {
            return;
        }
        self.stash_active_tab();
        self.workspace.cycle(delta);
        self.load_active_tab();
    }

    /// `vault / folder / note.md` for the editor header.
    fn breadcrumbs(&self) -> Vec<String> {
        let Some(path) = &self.open_document else {
            return Vec::new();
        };
        let mut crumbs = Vec::new();
        if let Some(vault) = &self.vault
            && let Some(name) = vault.root.file_name().and_then(|name| name.to_str())
        {
            crumbs.push(name.to_string());
        }
        for component in path.components() {
            if let std::path::Component::Normal(part) = component {
                crumbs.push(part.to_string_lossy().into_owned());
            }
        }
        crumbs
    }

    /// Window title reflecting the open document and dirty state.
    fn window_title(&self) -> String {
        let Some(path) = &self.open_document else {
            return "Weft".to_string();
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled");
        let dirty = if self.dirty { " *" } else { "" };
        format!("{name}{dirty} — Weft")
    }

    /// Whether any tab holds edits that are not on disk.
    fn has_unsaved_tabs(&self) -> bool {
        self.dirty
            || self
                .workspace
                .panes
                .iter()
                .any(|pane| pane.tabs.iter().any(|tab| tab.unsaved.is_some()))
    }

    /// Apply an action chosen from the document tab strip.
    fn apply_tab_action(&mut self, action: TabStripAction) {
        match action {
            TabStripAction::NewTab => self.new_note(),
            TabStripAction::Activate(index) => {
                self.switch_to_tab(self.workspace.focused, index);
            }
            TabStripAction::CloseOthers => {
                self.stash_active_tab();
                self.workspace.close_others();
                self.load_active_tab();
            }
            TabStripAction::CloseToRight => {
                self.stash_active_tab();
                self.workspace.close_to_right();
                self.load_active_tab();
            }
            TabStripAction::ReopenClosed => {
                if self.workspace.reopen_closed().is_some() {
                    self.load_active_tab();
                }
            }
            TabStripAction::Close(index) => self.request_close_tab(index),
        }
    }

    /// Close one tab, asking first when it is the active dirty tab.
    fn request_close_tab(&mut self, index: usize) {
        if index == self.workspace.focused_pane().active && self.dirty {
            self.pending_action = Some(PendingAction::CloseDocument);
            return;
        }
        let pane = self.workspace.focused;
        self.stash_active_tab();
        self.workspace.close_tab(pane, index);
        self.load_active_tab();
    }

    /// Save the active buffer and every other tab with unsaved edits.
    ///
    /// Tabs keep their own buffers, so quitting must not only look at the
    /// active note. Failures leave the buffer in place and are reported, so a
    /// rejected write never looks like a successful save.
    fn save_all_dirty_tabs(&mut self) {
        self.save_document();

        let mut pending: Vec<(PathBuf, String)> = Vec::new();
        for pane in &mut self.workspace.panes {
            for tab in &mut pane.tabs {
                if let Some(buffer) = &tab.unsaved {
                    pending.push((tab.path.clone(), buffer.clone()));
                }
            }
        }

        let mut failures = Vec::new();
        {
            let Some(vault) = self.vault.as_ref() else {
                return;
            };
            for (path, buffer) in &pending {
                if let Err(error) = vault.write(path, buffer) {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
        }

        if failures.is_empty() {
            for pane in &mut self.workspace.panes {
                for tab in &mut pane.tabs {
                    tab.unsaved = None;
                }
            }
            self.rebuild_search_index();
            self.panels.search.invalidate();
            self.rebuild_graph();
        } else {
            self.external_change = Some(format!("Could not save: {}", failures.join("; ")));
        }
    }

    /// Resolve a pending action after the user chooses save/discard/cancel.
    fn finish_pending_action(&mut self, ctx: &egui::Context, save: bool) {
        let Some(action) = self.pending_action.take() else {
            return;
        };
        if save {
            match action {
                PendingAction::Quit => self.save_all_dirty_tabs(),
                PendingAction::CloseDocument => self.save_document(),
            }
        }
        match action {
            PendingAction::CloseDocument => self.close_document_now(),
            PendingAction::Quit => {
                self.dirty = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Load the open note's snapshots and show the recovery dialog.
    fn open_recovery_dialog(&mut self) {
        let Some(path) = self.open_document.clone() else {
            return;
        };
        let Some(history) = self.history.as_ref() else {
            return;
        };
        match history.snapshots(&path) {
            Ok(snapshots) => {
                let preview = snapshots
                    .first()
                    .and_then(|snapshot| history.read(snapshot).ok())
                    .unwrap_or_default();
                self.recovery_selected = (!snapshots.is_empty()).then_some(0);
                self.recovery_snapshots = snapshots;
                self.recovery_preview = preview;
                self.recovery_open = true;
            }
            Err(error) => {
                self.external_change = Some(format!("Could not read file history: {error}"));
            }
        }
    }

    /// Put the selected snapshot into the editor buffer without touching disk.
    fn restore_selected_snapshot(&mut self) {
        let Some(index) = self.recovery_selected else {
            return;
        };
        let Some(snapshot) = self.recovery_snapshots.get(index).cloned() else {
            return;
        };
        let Some(content) = self
            .history
            .as_ref()
            .and_then(|history| history.read(&snapshot).ok())
        else {
            self.external_change = Some("Could not read the selected snapshot.".to_string());
            return;
        };
        self.document_content = content;
        self.dirty = true;
        self.last_auto_save = std::time::Instant::now();
        self.external_change = Some(format!(
            "Restored the {} snapshot into the editor; save to keep it.",
            snapshot.label()
        ));
        self.recovery_open = false;
    }

    /// Render the file-recovery dialog.
    fn render_recovery_dialog(&mut self, ctx: &egui::Context) {
        if !self.recovery_open {
            return;
        }
        let mut close = false;
        let mut restore = false;

        // Size the list and preview to the viewport so the action row stays
        // visible on small screens instead of being pushed off the bottom.
        let screen_height = ctx.screen_rect().height();
        let pane_height = ((screen_height - 260.0) * 0.7).clamp(120.0, 340.0);
        let max_height = (screen_height - 160.0).max(240.0);

        egui::Window::new("File recovery")
            .collapsible(false)
            .resizable(true)
            .default_size([680.0, (pane_height + 150.0).min(max_height)])
            .max_height(max_height)
            .show(ctx, |ui| {
                match &self.open_document {
                    Some(path) => {
                        ui.label(path.display().to_string());
                    }
                    None => {
                        ui.label("No note is open.");
                    }
                }
                ui.separator();

                if self.recovery_snapshots.is_empty() {
                    ui.label(
                        "No snapshots yet. Weft records the previous contents of a note \
                         whenever it is saved.",
                    );
                } else {
                    ui.horizontal_top(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("recovery_list")
                            .max_width(220.0)
                            .max_height(pane_height)
                            .show(ui, |ui| {
                                for (index, snapshot) in self.recovery_snapshots.iter().enumerate()
                                {
                                    let selected = self.recovery_selected == Some(index);
                                    if ui.selectable_label(selected, snapshot.label()).clicked() {
                                        self.recovery_selected = Some(index);
                                        self.recovery_preview = self
                                            .history
                                            .as_ref()
                                            .and_then(|history| history.read(snapshot).ok())
                                            .unwrap_or_default();
                                    }
                                }
                            });
                        ui.separator();
                        egui::ScrollArea::both()
                            .id_salt("recovery_preview")
                            .max_height(pane_height)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.recovery_preview).monospace(),
                                    )
                                    .wrap(),
                                );
                            });
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.recovery_selected.is_some(),
                            egui::Button::new("Restore into editor"),
                        )
                        .clicked()
                    {
                        restore = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        if restore {
            self.restore_selected_snapshot();
        }
        if close {
            self.recovery_open = false;
        }
    }

    /// Render the "unsaved changes" confirmation dialog when needed.
    fn render_unsaved_dialog(&mut self, ctx: &egui::Context) {
        if self.pending_action.is_none() {
            return;
        }

        let mut resolve: Option<bool> = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("The current note has unsaved changes.");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        resolve = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        resolve = Some(false);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_action = None;
                    }
                });
            });

        if let Some(save) = resolve {
            self.finish_pending_action(ctx, save);
        }
    }

    /// Render the export dialog modal if an export is pending.
    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(export) = std::mem::take(&mut self.export_state) else {
            return;
        };
        let doc_path = match &export {
            ExportState::Document { doc_path, .. } => Some(doc_path.clone()),
            ExportState::Vault { .. } => None,
        };
        let mut output = match &export {
            ExportState::Document { output, .. } | ExportState::Vault { output } => output.clone(),
        };
        let vault_root = self.vault.as_ref().map(|v| v.root.clone());
        let mut do_export = false;
        let mut do_cancel = false;

        egui::Window::new("Export")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Export");
                ui.separator();
                match &export {
                    ExportState::Document { doc_path, .. } => {
                        ui.label(format!("Document: {}", doc_path.display()));
                    }
                    ExportState::Vault { .. } => {
                        if let Some(root) = &vault_root {
                            ui.label(format!("Vault: {}", root.display()));
                        }
                    }
                }
                ui.label("Output path:");
                ui.text_edit_singleline(&mut output);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Export").clicked() {
                        do_export = true;
                    }
                    if ui.button("Cancel").clicked() {
                        do_cancel = true;
                    }
                });
            });

        if do_cancel {
            self.export_state = None;
        } else if do_export {
            let Some(vault) = self.vault.as_ref() else {
                self.export_state = None;
                return;
            };
            let result = match doc_path {
                Some(ref p) => export_document(vault, p, Path::new(&output)).map(|_| ()),
                None => export_vault_bundle(vault, Path::new(&output)).map(|_| ()),
            };
            match result {
                Ok(()) => {
                    self.export_state = None;
                }
                Err(e) => {
                    self.export_state = Some(match doc_path {
                        Some(p) => ExportState::Document {
                            doc_path: p,
                            output,
                        },
                        None => ExportState::Vault { output },
                    });
                    eprintln!("Export failed: {e}");
                }
            }
        } else {
            // Window closed or reshow: keep the dialog open with any edits.
            self.export_state = Some(match doc_path {
                Some(p) => ExportState::Document {
                    doc_path: p,
                    output,
                },
                None => ExportState::Vault { output },
            });
        }
    }

    /// Render the preferences dialog modal if open.
    fn render_settings_dialog(&mut self, ctx: &egui::Context) {
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                let changed = self.settings_panel.show(ui, &mut self.settings);
                if changed {
                    // Apply theme immediately.
                    self.settings.theme.apply(ctx);
                    self.theme_is_dark = self.settings.theme.is_dark(ctx);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.settings_open = false;
                    }
                });
            });
    }

    /// Render the open vault dialog modal if open.
    fn render_vault_open_dialog(&mut self, ctx: &egui::Context) {
        let mut open_path: Option<PathBuf> = None;
        let mut vault_path = self.vault_open_path.clone();

        egui::Window::new("Open Vault")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Vault path:");
                ui.text_edit_singleline(&mut vault_path);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        let path = PathBuf::from(vault_path.trim());
                        if !path.as_os_str().is_empty() {
                            open_path = Some(path);
                        }
                    }
                    if ui.button("Cancel").clicked() {}
                });
            });

        self.vault_open_path = vault_path;

        if let Some(path) = open_path {
            self.open_vault(&path);
            self.vault_open_dialog_open = false;
        }
    }

    /// Render the command palette modal if open.
    /// Show the in-note find bar, seeding it from any open selection.
    fn open_find_bar(&mut self) {
        self.find_open = true;
        self.find_focus = true;
        self.refresh_find_hits();
        if !self.find_hits.is_empty() {
            self.select_find_hit();
        }
    }

    /// Recompute matches for the current query.
    fn refresh_find_hits(&mut self) {
        self.find_hits = find::find_all(
            &self.document_content,
            &self.find_query,
            self.find_match_case,
        );
        if self.find_index >= self.find_hits.len() {
            self.find_index = 0;
        }
    }

    /// Ask the editor to select the highlighted match.
    fn select_find_hit(&mut self) {
        let Some(hit) = self.find_hits.get(self.find_index).copied() else {
            self.pending_selection = None;
            return;
        };
        self.pending_selection = Some((
            char_index(&self.document_content, hit.start),
            char_index(&self.document_content, hit.end),
        ));
    }

    /// Move the highlight to the next or previous match, wrapping around.
    fn step_find(&mut self, forward: bool) {
        self.refresh_find_hits();
        if self.find_hits.is_empty() {
            self.pending_selection = None;
            return;
        }
        let len = self.find_hits.len();
        self.find_index = if forward {
            (self.find_index + 1) % len
        } else {
            (self.find_index + len - 1) % len
        };
        self.select_find_hit();
    }

    /// Replace the highlighted match and highlight the following one.
    fn replace_current_hit(&mut self) {
        self.refresh_find_hits();
        let Some(hit) = self.find_hits.get(self.find_index).copied() else {
            return;
        };
        let replacement = self.find_replacement.clone();
        self.document_content
            .replace_range(hit.start..hit.end, &replacement);
        self.dirty = true;
        self.refresh_find_hits();
        if self.find_hits.is_empty() {
            self.pending_selection = None;
        } else {
            self.find_index = self.find_index.min(self.find_hits.len() - 1);
            self.select_find_hit();
        }
    }

    /// Replace every match in the open note.
    fn replace_all_hits(&mut self) {
        let (text, count) = find::replace_all(
            &self.document_content,
            &self.find_query,
            &self.find_replacement,
            self.find_match_case,
        );
        if count == 0 {
            return;
        }
        self.document_content = text;
        self.dirty = true;
        self.refresh_find_hits();
        self.pending_selection = None;
        self.external_change = Some(format!("Replaced {count} occurrence(s)."));
    }

    /// Render the find and replace bar above the editor.
    fn render_find_bar(&mut self, ui: &mut egui::Ui) {
        if !self.find_open {
            return;
        }
        let palette = palette(ui.ctx());
        let action = self.find_bar_controls(ui, &palette);

        let status = if self.find_query.is_empty() {
            "Type to search this note".to_string()
        } else if self.find_hits.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} of {}", self.find_index + 1, self.find_hits.len())
        };
        ui.label(
            egui::RichText::new(status)
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.separator();

        if action.changed {
            self.find_index = 0;
            self.refresh_find_hits();
            self.select_find_hit();
        }
        if action.next {
            self.step_find(true);
            self.focus_editor = true;
        }
        if action.previous {
            self.step_find(false);
            self.focus_editor = true;
        }
        if action.replace {
            self.replace_current_hit();
            self.focus_editor = true;
        }
        if action.replace_all {
            self.replace_all_hits();
        }
        if action.close {
            self.find_open = false;
            self.pending_selection = None;
            self.focus_editor = true;
        }
    }

    /// The find bar's inputs and buttons.
    fn find_bar_controls(
        &mut self,
        ui: &mut egui::Ui,
        palette: &crate::theme::Palette,
    ) -> FindBarAction {
        let mut action = FindBarAction::default();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Find")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.find_query)
                    .hint_text("Find in note")
                    .desired_width(180.0),
            );
            if self.find_focus {
                response.request_focus();
                self.find_focus = false;
            }
            if response.changed() {
                action.changed = true;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                action.next = true;
            }

            ui.label(
                egui::RichText::new("Replace")
                    .size(12.0)
                    .color(palette.text_muted),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.find_replacement)
                    .hint_text("Replacement")
                    .desired_width(180.0),
            );

            action.changed |= ui
                .selectable_label(self.find_match_case, "Aa")
                .on_hover_text("Match case")
                .clicked();
            action.previous = ui.button("Previous").clicked();
            action.next |= ui.button("Next").clicked();
            action.replace = ui.button("Replace").clicked();
            action.replace_all = ui.button("Replace all").clicked();
            action.close = ui.button("×").on_hover_text("Close (Esc)").clicked();
        });
        action
    }

    /// Open the quick switcher and snapshot the candidate notes.
    ///
    /// Open tabs come first so an empty query lists what the user is already
    /// working on; the rest of the vault follows.
    fn open_quick_switcher(&mut self) {
        let Some(vault) = &self.vault else {
            self.external_change = Some("Open a vault first.".to_string());
            return;
        };
        let mut pool: Vec<PathBuf> = self
            .workspace
            .panes
            .iter()
            .flat_map(|pane| pane.tabs.iter().map(|tab| tab.path.clone()))
            .collect();
        if let Ok(files) = vault.list_files() {
            for path in files {
                if !pool.contains(&path) {
                    pool.push(path);
                }
            }
        }
        self.quick_pool = pool;
        self.quick_input.clear();
        self.quick_selected = 0;
        self.quick_open = true;
        self.refresh_quick_results();
    }

    /// Recompute the ranked results for the current query.
    fn refresh_quick_results(&mut self) {
        const LIMIT: usize = 60;
        if self.quick_input.trim().is_empty() {
            self.quick_results = self
                .quick_pool
                .iter()
                .take(LIMIT)
                .map(|path| Ranked {
                    path: path.clone(),
                    score: 0,
                })
                .collect();
        } else {
            self.quick_results = fuzzy::rank(
                self.quick_input.trim(),
                self.quick_pool.iter().map(PathBuf::as_path),
                LIMIT,
            );
        }
        self.quick_selected = self
            .quick_selected
            .min(self.quick_results.len().saturating_sub(1));
    }

    /// Render the quick switcher overlay.
    fn render_quick_switcher(&mut self, ctx: &egui::Context) {
        if !self.quick_open {
            return;
        }

        let mut close = false;
        let mut open: Option<PathBuf> = None;
        let width = (ctx.screen_rect().width() - 80.0).clamp(320.0, 640.0);
        let center = ctx.available_rect().center();
        let rect = egui::Rect::from_center_size(center, egui::vec2(width, 420.0));

        egui::Window::new("Quick switch")
            .collapsible(false)
            .resizable(true)
            .title_bar(false)
            .current_pos(rect.min)
            .default_width(width)
            .max_height(460.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.quick_input)
                        .hint_text("Jump to a note...")
                        .desired_width(f32::INFINITY),
                );
                if response.changed() {
                    self.quick_selected = 0;
                    self.refresh_quick_results();
                }
                if !response.has_focus() {
                    response.request_focus();
                }

                let input = ui.ctx().input(|i| {
                    (
                        i.key_pressed(Key::ArrowDown),
                        i.key_pressed(Key::ArrowUp),
                        i.key_pressed(Key::Enter),
                        i.key_pressed(Key::Escape),
                    )
                });
                let (down, up, enter, escape) = input;
                if down && !self.quick_results.is_empty() {
                    self.quick_selected = (self.quick_selected + 1) % self.quick_results.len();
                }
                if up && !self.quick_results.is_empty() {
                    self.quick_selected = self
                        .quick_selected
                        .checked_sub(1)
                        .unwrap_or(self.quick_results.len() - 1);
                }
                if escape {
                    close = true;
                }
                if enter {
                    open = self
                        .quick_results
                        .get(self.quick_selected)
                        .map(|r| r.path.clone());
                }

                ui.separator();
                if self.quick_results.is_empty() {
                    ui.label(
                        egui::RichText::new("No note matches.").color(palette(ui.ctx()).text_muted),
                    );
                }
                egui::ScrollArea::vertical()
                    .id_salt("quick_results")
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for index in 0..self.quick_results.len() {
                            let path = &self.quick_results[index].path;
                            let label = path.display().to_string();
                            let selected = index == self.quick_selected;
                            if ui.selectable_label(selected, label).clicked() {
                                open = Some(path.clone());
                            }
                        }
                    });
            });

        if let Some(path) = open {
            close = true;
            self.request_open_document(&path);
        }
        if close {
            self.quick_open = false;
        }
    }

    fn render_command_palette(&mut self, ctx: &egui::Context) {
        let width = 500.0;
        let height = 300.0;
        let center = ctx.available_rect().center();
        let rect = egui::Rect::from_center_size(center, egui::vec2(width, height));

        egui::Window::new("Command Palette")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .current_pos(rect.min)
            .show(ctx, |ui| {
                ui.heading("Command Palette");
                ui.separator();
                ui.text_edit_singleline(&mut self.command_palette_input);
                ui.separator();

                ui.label("Commands:");
                ui.label("  Ctrl+S - Save document");
                ui.label("  Ctrl+O - Quick switch");
                ui.label("  Ctrl+Shift+O - Open vault");
                ui.label("  Ctrl+N - New note");
                ui.label("  Ctrl+F - Find in note");
                ui.label("  Ctrl+Shift+F - Search vault");
                ui.label("  Ctrl+P - Command palette");
                ui.label("  Ctrl+G - Toggle graph");
                ui.label("  Ctrl+W - Close note");
                ui.label("  Ctrl+Tab - Next note");
                ui.label("  Ctrl+Shift+P - Export document");
                ui.label("  Ctrl+Shift+B - Export vault bundle");

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.command_palette_open = false;
                    }
                    if ui.button("Export Document").clicked() {
                        if let Some(doc) = &self.open_document {
                            self.export_state = Some(ExportState::Document {
                                doc_path: doc.clone(),
                                output: String::new(),
                            });
                        }
                        self.command_palette_open = false;
                    }
                });
            });
    }
}

impl eframe::App for EditorApp {
    #[expect(
        clippy::too_many_lines,
        reason = "single eframe update entry point wires the whole UI together"
    )]
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Intercept window close when any tab has unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.stash_active_tab();
            if self.has_unsaved_tabs() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_action = Some(PendingAction::Quit);
            }
        }

        // Keep the window title in sync with the open document.
        let title = self.window_title();
        if title != self.last_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_window_title = title;
        }

        // Keyboard shortcuts (handled before panels so they take priority).
        self.handle_keyboard(ctx);

        // Follow the OS theme when the user picked System.
        let want_dark = self.settings.theme.is_dark(ctx);
        if want_dark != self.theme_is_dark {
            self.theme_is_dark = want_dark;
            self.settings.theme.apply(ctx);
        }

        // Auto-save check.
        self.maybe_auto_save();

        // Apply filesystem changes detected in the background.
        self.poll_watcher();

        // Top menu bar.
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Note").clicked() {
                        self.new_note();
                        ui.close_menu();
                    }
                    if ui.button("Quick Switch...\tCtrl+O").clicked() {
                        self.open_quick_switcher();
                        ui.close_menu();
                    }
                    if ui.button("Open Vault...\tCtrl+Shift+O").clicked() {
                        self.vault_open_dialog_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        self.save_document();
                        ui.close_menu();
                    }
                    if ui.button("Close Note").clicked() {
                        self.request_close_document();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.open_document.is_some(),
                            egui::Button::new("File Recovery..."),
                        )
                        .clicked()
                    {
                        self.open_recovery_dialog();
                        ui.close_menu();
                    }
                    ui.menu_button("Export", |ui| {
                        if ui.button("Export Document as HTML...").clicked() {
                            if let Some(doc) = &self.open_document {
                                self.export_state = Some(ExportState::Document {
                                    doc_path: doc.clone(),
                                    output: String::new(),
                                });
                            }
                            ui.close_menu();
                        }
                        if ui.button("Export Vault Bundle...").clicked() {
                            self.export_state = Some(ExportState::Vault {
                                output: String::new(),
                            });
                            ui.close_menu();
                        }
                    });
                    if ui.button("Quit").clicked() {
                        self.stash_active_tab();
                        if self.has_unsaved_tabs() {
                            self.pending_action = Some(PendingAction::Quit);
                        } else {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Preferences...").clicked() {
                        self.settings_open = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    use crate::theme::ThemeMode;
                    let pane = self.workspace.focused;
                    let can_back = self.nav.get(pane).is_some_and(NavHistory::can_go_back);
                    let can_forward = self.nav.get(pane).is_some_and(NavHistory::can_go_forward);
                    if ui
                        .add_enabled(can_back, egui::Button::new("Back\tAlt+Left"))
                        .clicked()
                    {
                        self.navigate(false);
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(can_forward, egui::Button::new("Forward\tAlt+Right"))
                        .clicked()
                    {
                        self.navigate(true);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Find in Note\tCtrl+F").clicked() {
                        self.open_find_bar();
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(self.search_open, "Search Vault\tCtrl+Shift+F")
                        .clicked()
                    {
                        self.search_open = !self.search_open;
                        if self.search_open {
                            self.panels.search.focus();
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    for (label, mode) in [
                        ("System", ThemeMode::System),
                        ("Light", ThemeMode::Light),
                        ("Dark", ThemeMode::Dark),
                    ] {
                        if ui
                            .selectable_label(self.settings.theme.mode == mode, label)
                            .clicked()
                        {
                            self.settings.theme.mode = mode;
                            self.settings.theme.apply(ctx);
                            self.theme_is_dark = self.settings.theme.is_dark(ctx);
                            ui.close_menu();
                        }
                    }
                });
            });

            ui.separator();

            // Toolbar: identity, Editor/Graph tabs, and quick actions.
            let palette = palette(ctx);
            ui.horizontal(|ui| {
                let vault_name = self
                    .vault
                    .as_ref()
                    .and_then(|vault| vault.root.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("No vault");
                ui.label(egui::RichText::new(vault_name).strong().size(15.0));
                if let Some(doc) = &self.open_document {
                    let name = doc
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("untitled");
                    ui.label(
                        egui::RichText::new(format!("/ {name}"))
                            .size(12.0)
                            .color(palette.text_muted),
                    );
                }
                if self.dirty {
                    badge(ui, "Unsaved", palette.warning);
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(4.0);
                let active_tab = usize::from(self.panels.active == PanelId::Graph);
                if let Some(tab) = tab_bar(ui, active_tab, &["Editor", "Graph"]) {
                    self.panels.active = if tab == 1 {
                        PanelId::Graph
                    } else {
                        PanelId::EditorSplit
                    };
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "Settings", "Preferences").clicked() {
                        self.settings_open = true;
                    }
                    if icon_button(ui, "Search", "Search notes (Ctrl+F)").clicked() {
                        self.search_open = !self.search_open;
                        if self.search_open {
                            self.panels.search.focus();
                        }
                    }
                });
            });
            ui.add_space(2.0);
        });

        // Conflict banner: a file changed on disk while the buffer was dirty.
        let mut dismiss_notice = false;
        if let Some(notice) = self.external_change.clone() {
            let palette = palette(ctx);
            TopBottomPanel::top("external_change_banner").show(ctx, |ui| {
                toast(ui, &notice, palette.warning, &mut dismiss_notice);
            });
        }
        if dismiss_notice {
            self.external_change = None;
        }

        // Persistent status bar: vault, note, save state, and graph stats.
        {
            let palette = palette(ctx);
            let mut left = Vec::new();
            left.push(
                self.vault
                    .as_ref()
                    .and_then(|vault| vault.root.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("No vault")
                    .to_string(),
            );
            if let Some(doc) = &self.open_document {
                left.push(doc.display().to_string());
                left.push(format!(
                    "{} words",
                    self.document_content.split_whitespace().count()
                ));
            }
            if self.open_document.is_some() {
                left.push(if self.dirty { "Unsaved" } else { "Saved" }.to_string());
            }
            let right = vec![
                format!("{} nodes", self.graph.node_count()),
                format!("{} links", self.graph.edge_count()),
                "Ctrl+P commands".to_string(),
            ];
            TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                status_bar(ui, &palette, &left, &right);
            });
        }

        // Bottom search panel.
        if self.search_open {
            let mut close_search = false;
            TopBottomPanel::bottom("search_panel")
                .resizable(true)
                .default_height(220.0)
                .show(ctx, |ui| {
                    close_search = self.panels.search.show(
                        ui,
                        self.search_index.as_ref(),
                        &mut self.event_bus,
                    );
                });
            if close_search {
                self.search_open = false;
                self.panels.search.invalidate();
            }
        }

        // Bottom backlinks panel for the open note.
        if self.open_document.is_some() {
            TopBottomPanel::bottom("backlinks_panel")
                .resizable(true)
                .default_height(76.0)
                .show(ctx, |ui| {
                    self.panels.backlinks.show(
                        ui,
                        &self.graph,
                        self.open_document.as_deref(),
                        &mut self.event_bus,
                    );
                });
        }

        // Left side panel: file tree.
        SidePanel::left("file_tree").show(ctx, |ui| {
            self.panels.file_tree.show(
                ui,
                self.vault.as_mut(),
                self.open_document.as_deref(),
                self.dirty,
                &mut self.event_bus,
            );
        });

        // Central panel: editor split or graph view depending on active panel.
        let mut graph_moved = false;
        let mut tab_action: Option<TabStripAction> = None;
        let mut editor_metrics: Option<EditorMetrics> = None;
        let breadcrumbs = self.breadcrumbs();
        // The editor is about to render this document. If a tab action moves
        // the workspace during the same frame, the metrics belong to the old
        // note and must not be written to the new tab.
        let rendered_document = self.open_document.clone();
        CentralPanel::default().show(ctx, |ui| {
            if self.panels.active == PanelId::Graph {
                graph_moved = self.panels.graph.show(
                    ui,
                    &mut self.graph,
                    &mut self.layout_store,
                    &mut self.event_bus,
                    self.open_document.as_deref(),
                );
            } else {
                tab_action = document_tabs(ui, self.workspace.focused_pane(), self.dirty);
                self.render_find_bar(ui);
                ui.separator();
                editor_metrics = Some(self.panels.editor_split.show(
                    ui,
                    &self.open_document,
                    &mut self.document_content,
                    &mut self.dirty,
                    ctx,
                    &mut self.editor_split_ratio,
                    self.settings.font_size,
                    &breadcrumbs,
                    &mut self.restore_cursor,
                    &mut self.restore_scroll,
                    &mut self.pending_selection,
                ));
            }
        });
        if let Some(action) = tab_action {
            self.apply_tab_action(action);
        }
        if let Some(metrics) = editor_metrics
            && self.open_document == rendered_document
        {
            self.editor_metrics = Some(metrics);
            self.record_nav_position();
        }
        if self.focus_editor
            && let Some(path) = &self.open_document
        {
            let id = crate::widgets::MarkdownEditor::text_id(path);
            ctx.memory_mut(|memory| memory.request_focus(id));
            self.focus_editor = false;
        }
        if graph_moved {
            self.save_layout();
        }

        // Dispatch cross-panel events.
        self.process_events(ctx);

        // Render the export dialog modal if one is open.
        self.render_export_dialog(ctx);

        // Render the preferences dialog if open.
        if self.settings_open {
            self.render_settings_dialog(ctx);
        }

        // Render the open vault dialog if open.
        if self.vault_open_dialog_open {
            self.render_vault_open_dialog(ctx);
        }

        // Render the quick switcher if open.
        self.render_quick_switcher(ctx);

        // Render the command palette if open.
        if self.command_palette_open {
            self.render_command_palette(ctx);
        }

        // Render the file-recovery dialog if open.
        self.render_recovery_dialog(ctx);

        // Render the unsaved-changes confirmation dialog if pending.
        self.render_unsaved_dialog(ctx);

        // Keep auto-save ticking without forcing continuous repaints.
        let repaint_in = if self.dirty && self.settings.auto_save_interval_secs > 0 {
            let elapsed = self.last_auto_save.elapsed().as_secs();
            self.settings
                .auto_save_interval_secs
                .saturating_sub(elapsed)
                .max(1)
        } else {
            30
        };
        ctx.request_repaint_after(std::time::Duration::from_secs(repaint_in));
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        self.state.vault_path = self.vault.as_ref().map(|vault| vault.root.clone());
        self.state.open_document = self.open_document.clone();
        self.state.active_panel = self.panels.active_panel();
        self.state.editor_split_ratio = self.editor_split_ratio;
        self.state.save(storage);
        self.settings.save(storage);
        self.save_layout();
        self.save_workspace();
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(30)
    }

    fn on_exit(&mut self, _ctx: Option<&eframe::glow::Context>) {
        self.save_layout();
        self.save_workspace();
    }
}

/// State for the export dialog modal.
#[derive(Debug, Clone)]
pub enum ExportState {
    /// Export a single document as HTML.
    Document { doc_path: PathBuf, output: String },
    /// Export the whole vault as a markdown bundle.
    Vault { output: String },
}

/// Buttons the user pressed in the find bar.
///
/// One flag per control: each is independent, so an enum per button would
/// only add conversion noise.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag mirrors one independent control in the bar"
)]
#[derive(Debug, Default, Clone, Copy)]
struct FindBarAction {
    changed: bool,
    close: bool,
    next: bool,
    previous: bool,
    replace: bool,
    replace_all: bool,
}

/// Action waiting on unsaved-changes confirmation.
#[derive(Debug, Clone, Copy)]
enum PendingAction {
    /// Close the current document.
    CloseDocument,
    /// Close the application.
    Quit,
}

/// Normalize `path` to a vault-relative path.
///
/// The graph, the search index, and watcher events all key on paths relative
/// to the vault root, so the open document must match, otherwise backlink and
/// graph lookups miss. Absolute paths under `root` are stripped; anything else
/// is returned unchanged.
fn vault_relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Whether `path` is a normal, non-hidden markdown path relative to the vault
/// root. Used before creating a note from an unresolved graph link.
fn is_safe_note_path(path: &Path) -> bool {
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                if name.to_string_lossy().starts_with('.') {
                    return false;
                }
                has_normal_component = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }

    has_normal_component
        && path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// Build a non-colliding path `<dir>/<stem>[ n].<ext>` inside `vault`.
fn unique_child_path(vault: &Vault, dir: &Path, stem: &str, ext: &str) -> PathBuf {
    for index in 0.. {
        let name = if index == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} {index}.{ext}")
        };
        let candidate = dir.join(name);
        if !vault.resolve(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!("the loop always returns before exhausting usize")
}

/// Character index of a byte offset, clamped to the string length.
fn char_index(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    text[..byte].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_default_is_sane() {
        let state = AppState::default();
        assert!(state.vault_path.is_none());
        assert!(state.open_document.is_none());
        assert_eq!(state.active_panel, PanelId::FileTree);
        assert!((state.editor_split_ratio - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn app_state_round_trips_through_json() {
        let state = AppState {
            vault_path: Some(PathBuf::from("/vault")),
            open_document: Some(PathBuf::from("notes/a.md")),
            window_rect: Some([10.0, 20.0, 800.0, 600.0]),
            active_panel: PanelId::Graph,
            editor_split_ratio: 0.7,
        };
        let json = serde_json::to_string(&state).expect("serialize app state");
        let loaded: AppState = serde_json::from_str(&json).expect("deserialize app state");
        assert_eq!(loaded.vault_path, state.vault_path);
        assert_eq!(loaded.open_document, state.open_document);
        assert_eq!(loaded.active_panel, state.active_panel);
        assert!((loaded.editor_split_ratio - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn vault_relative_strips_only_the_vault_root() {
        let root = Path::new("/vault");
        assert_eq!(
            vault_relative(root, Path::new("/vault/notes/a.md")),
            PathBuf::from("notes/a.md")
        );
        assert_eq!(
            vault_relative(root, Path::new("notes/a.md")),
            PathBuf::from("notes/a.md")
        );
        assert_eq!(
            vault_relative(root, Path::new("/elsewhere/a.md")),
            PathBuf::from("/elsewhere/a.md")
        );
    }

    #[test]
    fn safe_note_paths_reject_escape_and_hidden_paths() {
        assert!(is_safe_note_path(Path::new("Missing Note.md")));
        assert!(is_safe_note_path(Path::new("sub/alpha.md")));
        assert!(is_safe_note_path(Path::new("./sub/alpha.markdown")));

        assert!(!is_safe_note_path(Path::new("../outside.md")));
        assert!(!is_safe_note_path(Path::new("/etc/passwd.md")));
        assert!(!is_safe_note_path(Path::new(".hidden/note.md")));
        assert!(!is_safe_note_path(Path::new("sub/.note.md")));
        assert!(!is_safe_note_path(Path::new("note.txt")));
        assert!(!is_safe_note_path(Path::new("")));
    }

    #[test]
    fn unique_child_path_avoids_existing_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = Vault::open(dir.path()).expect("open vault");
        vault
            .write(Path::new("Untitled.md"), "")
            .expect("first note");
        vault
            .write(Path::new("Untitled 1.md"), "")
            .expect("second note");

        let path = unique_child_path(&vault, Path::new(""), "Untitled", "md");
        assert_eq!(path, PathBuf::from("Untitled 2.md"));
    }
}
