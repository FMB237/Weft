//! Main eframe application.
//!
//! Handles app lifecycle: persistence (save/load vault state, window
// position, layout), theme, panels, and wiring between the GUI and
//! editor-core.

use std::path::{Path, PathBuf};

use editor_core::vault::{Vault};
use editor_core::document::Document;
use editor_core::graph::LinkGraph;
use editor_core::layout_store::LayoutStore;
use editor_core::export::{export_document, export_vault_bundle};
use editor_core::parser::parse_frontmatter;

use egui::{CentralPanel, Key, Modifiers, SidePanel, TopBottomPanel, Slider, DragValue};
use eframe::Storage;

use crate::event_bus::{Event, EventBus};
use crate::panels::{Panels, PanelId};
use crate::settings::{Settings, SettingsPanel};
use crate::theme::Theme;
use editor_core::plugins::{PluginEvent, PluginManager};

/// Persisted application state saved between sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AppState {
    /// Last opened vault path.
    pub vault_path: Option<PathBuf>,
    /// Window size/position hints.
    pub window_rect: Option<[f32; 4]>,
    /// Last active panel.
    pub active_panel: PanelId,
    /// Theme settings.
    pub theme: Theme,
}

impl AppState {
    const STORAGE_KEY: &'static str = "editor_app_state";

    pub fn load(storage: Option<&dyn Storage>) -> Self {
        storage.and_then(|s| s.get_string(Self::STORAGE_KEY)).and_then(|s| {
            serde_json::from_str::<AppState>(&s).ok()
        }).unwrap_or_default()
    }

    pub fn save(&self, storage: &mut dyn Storage) {
        if let Ok(json) = serde_json::to_string(self) {
            storage.set_string(Self::STORAGE_KEY, json);
        }
    }
}

/// The main editor application.
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
}

impl EditorApp {
    /// Create a new app instance, loading persisted state from storage.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::load(cc.storage);
        cc.egui_ctx.set_visuals(state.theme.visuals());

        let mut panels = Panels::new();
        panels.set_active(state.active_panel);

        Self {
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
            settings: Settings::load(cc.storage),
            settings_open: false,
            settings_panel: SettingsPanel::default(),
            plugins: PluginManager::new(),
            vault_open_dialog_open: false,
            vault_open_path: String::new(),
            editor_split_ratio: 0.5,
            command_palette_open: false,
            command_palette_input: String::new(),
        }
    }

    /// Open a vault at the given path.
    pub(crate) fn open_vault(&mut self, path: &Path) {
        match Vault::open(path) {
            Ok(vault) => {
                self.state.vault_path = Some(path.to_path_buf());
                self.vault = Some(vault);
                self.rebuild_graph();
                let _ = self.plugins.dispatch(&PluginEvent::VaultOpened(path.to_path_buf()));
            }
            Err(e) => {
                eprintln!("Failed to open vault at {:?}: {}", path, e);
            }
        }
    }

    /// Rebuild the link graph from the current vault's files.
    pub(crate) fn rebuild_graph(&mut self) {
        let Some(vault) = &self.vault else { return };
        let Ok(files) = vault.list_files() else { return };

        self.graph = LinkGraph::new();
        for file in &files {
            if let Ok(content) = vault.read(file) {
                let frontmatter = parse_frontmatter(&content);
                let title = frontmatter
                    .title
                    .clone()
                    .unwrap_or_else(|| {
                        file.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("untitled")
                            .to_string()
                    });
                self.graph.add_node(file.clone(), title);
            }
        }

        // Apply stored positions to the graph.
        self.layout_store.apply_to_graph(&mut self.graph);
    }

    /// Open a document for editing.
    pub(crate) fn open_document(&mut self, path: PathBuf) {
        let Some(vault) = &self.vault else { return };
        match vault.read(&path) {
            Ok(content) => {
                self.open_document = Some(path.clone());
                self.document_content = content;
                self.dirty = false;
                let _ = self.plugins.dispatch(&PluginEvent::DocumentOpened(path.clone()));
            }
            Err(e) => {
                eprintln!("Failed to read document {:?}: {}", path, e);
            }
        }
    }

    /// Save the current document to the vault.
    pub(crate) fn save_document(&mut self) {
        let Some(vault) = &mut self.vault else { return };
        let Some(path) = &self.open_document else { return };

        let doc = Document::new(path.clone(), self.document_content.clone());
        let content = doc.content.clone();
        // Re-attach frontmatter if present.
        let full_content = if doc.frontmatter.title.is_some() || !doc.frontmatter.tags.is_empty() {
            // Simple round-trip: serialize frontmatter back.
            let mut out = String::new();
            out.push_str("---\n");
            out.push_str(&toml::to_string(&doc.frontmatter).unwrap_or_default());
            out.push_str("---\n");
            out.push_str(&self.document_content);
            out
        } else {
            self.document_content.clone()
        };

        if vault.write(path, &full_content).is_ok() {
            self.dirty = false;
            self.event_bus.publish(Event::DocumentSaved(path.clone()));
            let _ = self.plugins.dispatch(&PluginEvent::DocumentSaved(path.clone()));
            self.rebuild_graph();
        }
    }

    /// Auto-save the current document if dirty and enough time has passed.
    pub(crate) fn maybe_auto_save(&mut self) {
        if !self.dirty {
            return;
        }
        if self.last_auto_save.elapsed().as_secs() < 30 {
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
                Event::NodeSelected(path) => {
                    self.open_document = Some(path);
                }
                Event::NodeOpened(path) => {
                    self.open_document = Some(path);
                }
                Event::GraphChanged => {
                    self.rebuild_graph();
                }
                Event::DocumentSaved(_path) => {
                    // Document already saved by the panel; mark not dirty.
                    self.dirty = false;
                }
                Event::VaultChanged => {
                    self.rebuild_graph();
                }
                Event::ThemeChanged => {
                    ctx.set_visuals(self.state.theme.visuals());
                }
            }
        }
    }

    /// Handle global keyboard shortcuts.
    pub(crate) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());
        let modifiers = input.modifiers;

        if modifiers.mac_cmd || modifiers.ctrl {
            if input.key_pressed(Key::S) {
                self.save_document();
                ctx.request_repaint();
            }
            if input.key_pressed(Key::O) {
                self.vault_open_dialog_open = true;
                ctx.request_repaint();
            }
            if input.key_pressed(Key::P) {
                // Toggle command palette
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
        }
    }

    /// Render the export dialog modal if an export is pending.
    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        let mut export = match std::mem::take(&mut self.export_state) {
            Some(e) => e,
            None => return,
        };
        let doc_path = match &export {
            ExportState::Document { doc_path, .. } => Some(doc_path.clone()),
            ExportState::Vault { .. } => None,
        };
        let mut output = match &export {
            ExportState::Document { output, .. } => output.clone(),
            ExportState::Vault { output } => output.clone(),
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
            let result = match doc_path {
                Some(ref p) => export_document(self.vault.as_ref().unwrap(), p, Path::new(&output)).map(|_| ()),
                None => export_vault_bundle(self.vault.as_ref().unwrap(), Path::new(&output)).map(|_| ()),
            };
            match result {
                Ok(()) => { self.export_state = None; }
                Err(e) => {
                    self.export_state = Some(match doc_path {
                        Some(p) => ExportState::Document { doc_path: p, output },
                        None => ExportState::Vault { output },
                    });
                    eprintln!("Export failed: {e}");
                }
            }
        } else {
            // Window closed or reshow: keep the dialog open with any edits.
            self.export_state = Some(match doc_path {
                Some(p) => ExportState::Document { doc_path: p, output },
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
                    ctx.set_visuals(self.settings.theme.visuals());
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
                    if ui.button("Cancel").clicked() {
                    }
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
                ui.label("  Ctrl+P - Command palette");
                ui.label("  Ctrl+P - Toggle graph");
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Keyboard shortcuts (handled before panels so they take priority).
        self.handle_keyboard(ctx);

        // Auto-save check.
        self.maybe_auto_save();

        // Top menu bar.
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Vault...").clicked() {
                        self.vault_open_dialog_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        self.save_document();
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
                            self.export_state = Some(ExportState::Vault { output: String::new() });
                            ui.close_menu();
                        }
                    });
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Redo").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences...").clicked() {
                        self.settings_open = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                    ui.close_menu();
                });
            });
        });

        // Left side panel: file tree.
        SidePanel::left("file_tree").show(ctx, |ui| {
            self.panels.file_tree
                .show(ui, self.vault.as_mut(), &mut self.event_bus);
        });

        // Central panel: editor split or graph view depending on active panel.
        CentralPanel::default().show(ctx, |ui| {
            if self.panels.active == PanelId::Graph {
                self.panels
                    .graph
                    .show(ui, &self.graph, &mut self.layout_store, &mut self.event_bus, self.open_document.as_deref());
            } else {
                self.panels.editor_split.show(
                    ui,
                    &self.open_document,
                    &mut self.document_content,
                    &mut self.dirty,
                    ctx,
                    &mut self.editor_split_ratio,
                );
            }
        });

        // Right side panel: preview.
        SidePanel::right("preview").show(ctx, |ui| {
            self.panels.preview.show(ui, &self.open_document, &self.document_content);
        });

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

        // Persist state on exit.
        ctx.request_repaint_after(std::time::Duration::from_secs(30));
    }

    fn on_exit(&mut self, _ctx: Option<&eframe::glow::Context>) {
        self.state.theme = self.panels.theme.clone();
        self.state.active_panel = self.panels.active_panel();
        self.state.window_rect = None; // egui handles window rect
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