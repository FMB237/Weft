//! Main eframe application.
//!
//! Handles app lifecycle: persistence (save/load vault state, window
// position, layout), theme, panels, and wiring between the GUI and
//! weft-core.

use std::path::{Component, Path, PathBuf};

use weft_core::export::{export_document, export_vault_bundle};
use weft_core::graph::LinkGraph;
use weft_core::graph_updater::GraphUpdater;
use weft_core::index::SearchIndex;
use weft_core::layout_store::LayoutStore;
use weft_core::vault::Vault;
use weft_core::watcher::{WatchEvent, WatchHandle};

use eframe::Storage;
use egui::{CentralPanel, Key, SidePanel, TopBottomPanel};

use crate::event_bus::{Event, EventBus};
use crate::panels::{PanelId, Panels};
use crate::settings::{Settings, SettingsPanel};
use crate::theme::palette;
use crate::widgets::chrome::{badge, icon_button, status_bar, tab_bar, toast};
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
        storage
            .and_then(|s| s.get_string(Self::STORAGE_KEY))
            .and_then(|s| serde_json::from_str::<AppState>(&s).ok())
            .unwrap_or_default()
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
            command_palette_open: false,
            command_palette_input: String::new(),
            pending_action: None,
            last_window_title: String::new(),
            search_index: None,
            watcher: None,
            search_open: false,
            external_change: None,
            theme_is_dark,
        };

        // Re-open the last vault and document, if they still exist.
        if let Some(vault_path) = app.state.vault_path.clone()
            && vault_path.exists()
        {
            app.open_vault(&vault_path);
        }
        if let Some(doc) = app.state.open_document.clone()
            && app.vault.is_some()
        {
            app.open_document(&doc);
        }

        app
    }

    /// Open a vault at the given path.
    pub(crate) fn open_vault(&mut self, path: &Path) {
        match Vault::open(path) {
            Ok(vault) => {
                self.state.vault_path = Some(path.to_path_buf());
                self.vault = Some(vault);
                self.layout_store = self
                    .layout_path()
                    .and_then(|p| LayoutStore::load_from_file(&p).ok())
                    .unwrap_or_default();
                self.open_document = None;
                self.document_content.clear();
                self.dirty = false;
                self.last_auto_save = std::time::Instant::now();
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

    /// Open a document for editing.
    pub(crate) fn open_document(&mut self, path: &Path) {
        let Some(vault) = &self.vault else { return };
        match vault.read(path) {
            Ok(content) => {
                self.open_document = Some(path.to_path_buf());
                self.document_content = content;
                self.dirty = false;
                self.last_auto_save = std::time::Instant::now();
                let _ = self
                    .plugins
                    .dispatch(&PluginEvent::DocumentOpened(path.to_path_buf()));
            }
            Err(e) => {
                eprintln!("Failed to read document {}: {e}", path.display());
            }
        }
    }

    /// Open a document, asking the user first when the current one is dirty.
    pub(crate) fn request_open_document(&mut self, path: PathBuf) {
        if self.dirty && self.open_document.as_deref() != Some(path.as_path()) {
            self.pending_action = Some(PendingAction::Open(path));
        } else {
            self.open_document(&path);
        }
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

    /// Drop the in-memory document buffer without touching the file on disk.
    fn close_document_now(&mut self) {
        self.open_document = None;
        self.document_content.clear();
        self.dirty = false;
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

        if vault.write(&path, &self.document_content).is_ok() {
            self.dirty = false;
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
            self.request_open_document(path.to_path_buf());
            return;
        }

        if vault.write(path, "").is_ok() {
            if let Some(index) = &mut self.search_index {
                let _ = index.add_document(path, "");
            }
            self.rebuild_graph();
            self.request_open_document(path.to_path_buf());
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
            self.request_open_document(path);
        }
    }

    /// Open the next markdown file in the vault, wrapping around.
    pub(crate) fn cycle_note(&mut self) {
        let Some(vault) = &self.vault else { return };
        let Ok(files) = vault.list_files() else {
            return;
        };
        if files.is_empty() {
            return;
        }
        let current = self
            .open_document
            .as_ref()
            .and_then(|open| files.iter().position(|f| f == open));
        let next = current.map_or(0, |index| (index + 1) % files.len());
        self.request_open_document(files[next].clone());
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
                    self.request_open_document(path);
                }
                Event::CreateNote(path) => {
                    self.create_note_at(&path);
                }
                Event::GraphChanged | Event::VaultChanged => {
                    self.rebuild_graph();
                }
                Event::EntryRenamed(from, to) => {
                    if let Some(open) = self.open_document.clone() {
                        if open == from {
                            self.open_document = Some(to.clone());
                        } else if let Ok(relative) = open.strip_prefix(&from) {
                            self.open_document = Some(to.join(relative));
                        }
                    }
                    self.rebuild_search_index();
                    self.panels.search.invalidate();
                    self.rebuild_graph();
                }
                Event::EntryDeleted(path) => {
                    let deleted_open = self
                        .open_document
                        .as_deref()
                        .is_some_and(|open| open == path || open.starts_with(&path));
                    if deleted_open {
                        self.close_document_now();
                    }
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
                let deleted_open = self
                    .open_document
                    .as_deref()
                    .is_some_and(|open| open == path || open.starts_with(&path));
                if deleted_open {
                    if self.dirty {
                        self.external_change = Some(format!(
                            "{} was deleted on disk; your buffer was kept.",
                            path.display()
                        ));
                    } else {
                        self.close_document_now();
                    }
                }
                self.panels.search.invalidate();
            }
            WatchEvent::Renamed(from, to) => {
                self.rebuild_search_index();
                self.panels.search.invalidate();
                if let Some(open) = self.open_document.clone() {
                    if open == from {
                        self.open_document = Some(to);
                    } else if let Ok(relative) = open.strip_prefix(&from) {
                        self.open_document = Some(to.join(relative));
                    }
                }
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
        let command = modifiers.mac_cmd || modifiers.ctrl;
        if !command {
            return;
        }

        if modifiers.shift {
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
            return;
        }

        if input.key_pressed(Key::S) {
            self.save_document();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::O) {
            self.vault_open_dialog_open = true;
            ctx.request_repaint();
        }
        if input.key_pressed(Key::N) {
            self.new_note();
            ctx.request_repaint();
        }
        if input.key_pressed(Key::F) {
            self.search_open = !self.search_open;
            if self.search_open {
                self.panels.search.focus();
            }
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
        if input.key_pressed(Key::Tab) {
            self.cycle_note();
            ctx.request_repaint();
        }
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

    /// Resolve a pending action after the user chooses save/discard/cancel.
    fn finish_pending_action(&mut self, ctx: &egui::Context, save: bool) {
        let Some(action) = self.pending_action.take() else {
            return;
        };
        if save {
            self.save_document();
        }
        match action {
            PendingAction::Open(path) => self.open_document(&path),
            PendingAction::CloseDocument => self.close_document_now(),
            PendingAction::Quit => {
                self.dirty = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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
                ui.label("  Ctrl+O - Open vault");
                ui.label("  Ctrl+N - New note");
                ui.label("  Ctrl+F - Search notes");
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
        // Intercept window close when there are unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_action = Some(PendingAction::Quit);
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
                    if ui.button("Open Vault...").clicked() {
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
                        if self.dirty {
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
                    if ui
                        .selectable_label(self.search_open, "Search\tCtrl+F")
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
                self.panels.editor_split.show(
                    ui,
                    &self.open_document,
                    &mut self.document_content,
                    &mut self.dirty,
                    ctx,
                    &mut self.editor_split_ratio,
                    self.settings.font_size,
                );
            }
        });
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

        // Render the command palette if open.
        if self.command_palette_open {
            self.render_command_palette(ctx);
        }

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
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(30)
    }

    fn on_exit(&mut self, _ctx: Option<&eframe::glow::Context>) {
        self.save_layout();
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

/// Action waiting on unsaved-changes confirmation.
#[derive(Debug, Clone)]
enum PendingAction {
    /// Open another document after saving or discarding.
    Open(PathBuf),
    /// Close the current document.
    CloseDocument,
    /// Close the application.
    Quit,
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
