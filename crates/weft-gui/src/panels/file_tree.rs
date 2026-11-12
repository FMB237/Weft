//! File explorer panel with a nested, collapsible tree.
//!
//! Lists the markdown files and directories in the currently open vault,
//! supports basic file operations through a context menu, and keeps the
//! active note highlighted and scrolled into view. Only the visible rows are
//! rendered (virtual scrolling), so large vaults stay performant.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::event_bus::{Event, EventBus};
use crate::theme::palette;
use crate::widgets::chrome::{empty_state, section_header};
use egui::{Id, RichText, ScrollArea, Ui, Vec2};
use weft_core::vault::Vault;

/// Height of a single file-tree row in points.
const ITEM_HEIGHT: f32 = 20.0;
/// Horizontal indentation per directory level in points.
const INDENT: f32 = 12.0;

/// One visible row in the flattened tree.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeRow {
    path: PathBuf,
    name: String,
    is_dir: bool,
    depth: usize,
}

/// A pending inline rename.
#[derive(Debug, Clone)]
struct RenameState {
    original: PathBuf,
    value: String,
}

#[derive(Debug)]
pub struct FileTreePanel {
    /// Directories currently expanded.
    expanded: HashSet<PathBuf>,
    /// Path passed in by the app as the currently open document.
    active: Option<PathBuf>,
    /// Previous active path, used to trigger expand/scroll on change.
    last_active: Option<PathBuf>,
    /// Set when the active row should be scrolled into view.
    scroll_to_active: bool,
    /// One-shot scroll offset applied on the next frame.
    pending_scroll: Option<f32>,
    /// Persisted vertical scroll offset between frames.
    scroll_offset: f32,
    /// Rename dialog state, if open.
    rename: Option<RenameState>,
    /// Path waiting for delete confirmation, if any.
    confirm_delete: Option<PathBuf>,
    /// Small status message (e.g. why an action was blocked).
    notice: Option<String>,
}

impl FileTreePanel {
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            active: None,
            last_active: None,
            scroll_to_active: false,
            pending_scroll: None,
            scroll_offset: 0.0,
            rename: None,
            confirm_delete: None,
            notice: None,
        }
    }

    /// Show the file tree, populated from the open vault.
    #[expect(
        clippy::too_many_lines,
        reason = "the tree panel renders rows, context menus, and dialogs in one place"
    )]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        vault: Option<&mut Vault>,
        active: Option<&Path>,
        dirty: bool,
        event_bus: &mut EventBus,
    ) {
        let palette = palette(ui.ctx());
        section_header(ui, "Files");

        let Some(vault) = vault else {
            empty_state(
                ui,
                "No vault open",
                "Use File > Open Vault to choose a folder of notes.",
            );
            return;
        };

        ui.label(
            RichText::new(vault.config.name.clone())
                .size(12.0)
                .color(palette.text_muted),
        );

        let (Ok(files), Ok(dirs)) = (vault.list_files(), vault.list_dirs()) else {
            ui.label("Failed to list vault files.");
            return;
        };

        self.sync_active(active);

        let rows = build_rows(&files, &dirs, &self.expanded);
        let mut action: Option<TreeAction> = None;
        let mut clicked_node: Option<PathBuf> = None;
        let mut toggled_dir: Option<PathBuf> = None;

        ui.horizontal(|ui| {
            if ui.button("+ Note").on_hover_text("Create a note").clicked() {
                action = Some(TreeAction::NewNote(PathBuf::new()));
            }
            if ui
                .button("+ Folder")
                .on_hover_text("Create a folder")
                .clicked()
            {
                action = Some(TreeAction::NewFolder(PathBuf::new()));
            }
        });
        ui.separator();

        if let Some(notice) = &self.notice {
            ui.colored_label(palette.warning, notice);
        }

        if rows.is_empty() {
            empty_state(
                ui,
                "This vault is empty",
                "Create your first note with the + Note button.",
            );
        } else {
            if self.scroll_to_active {
                if let Some(path) = &self.active
                    && let Some(index) = rows.iter().position(|row| row.path == *path)
                {
                    self.pending_scroll =
                        Some((index as f32 * ITEM_HEIGHT - ui.available_height() * 0.5).max(0.0));
                }
                self.scroll_to_active = false;
            }

            let prev_offset = self.pending_scroll.take().unwrap_or(self.scroll_offset);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "scroll offsets are non-negative screen coordinates"
            )]
            let visible_start = (prev_offset / ITEM_HEIGHT).floor() as usize;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "scroll offsets are non-negative screen coordinates"
            )]
            let mut visible_end =
                ((prev_offset + ui.available_height()) / ITEM_HEIGHT).ceil() as usize + 1;
            visible_end = visible_end.min(rows.len()).max(visible_start);

            let total_height = ITEM_HEIGHT * rows.len() as f32;
            let mut scroll = ScrollArea::vertical()
                .id_salt(Id::new("file_tree_scroll"))
                .auto_shrink([false, false]);
            if prev_offset > 0.0 {
                scroll = scroll.vertical_scroll_offset(prev_offset);
            }

            let output = scroll.min_scrolled_height(total_height).show(ui, |ui| {
                if visible_start > 0 {
                    ui.add_space(visible_start as f32 * ITEM_HEIGHT);
                }

                let inner_width = ui.available_width();
                for row in rows.iter().take(visible_end).skip(visible_start) {
                    let selected = self.active.as_deref() == Some(row.path.as_path());
                    ui.allocate_ui(Vec2::new(inner_width, ITEM_HEIGHT), |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(row.depth as f32 * INDENT);
                            let marker = if row.is_dir {
                                if self.expanded.contains(&row.path) {
                                    "▾"
                                } else {
                                    "▸"
                                }
                            } else {
                                " "
                            };
                            let response =
                                ui.selectable_label(selected, format!("{marker} {}", row.name));
                            if response.clicked() {
                                if row.is_dir {
                                    toggled_dir = Some(row.path.clone());
                                } else {
                                    clicked_node = Some(row.path.clone());
                                }
                            }

                            let target_dir = if row.is_dir {
                                row.path.clone()
                            } else {
                                parent_dir(&row.path)
                            };
                            response.context_menu(|ui| {
                                if ui.button("New Note").clicked() {
                                    action = Some(TreeAction::NewNote(target_dir.clone()));
                                    ui.close_menu();
                                }
                                if ui.button("New Folder").clicked() {
                                    action = Some(TreeAction::NewFolder(target_dir.clone()));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Rename").clicked() {
                                    action = Some(TreeAction::Rename(row.path.clone()));
                                    ui.close_menu();
                                }
                                if ui.button("Delete").clicked() {
                                    action = Some(TreeAction::Delete(row.path.clone()));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Reveal path").clicked() {
                                    action = Some(TreeAction::Reveal(row.path.clone()));
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                }

                let below = rows.len().saturating_sub(visible_end);
                if below > 0 {
                    ui.add_space(below as f32 * ITEM_HEIGHT);
                }
            });

            self.scroll_offset = output.state.offset.y;
        }

        if let Some(dir) = toggled_dir
            && !self.expanded.remove(&dir)
        {
            self.expanded.insert(dir);
        }
        if let Some(path) = clicked_node {
            self.active = Some(path.clone());
            event_bus.publish(Event::NodeSelected(path));
        }
        if let Some(action) = action {
            self.apply_action(action, vault, active, dirty, event_bus);
        }

        self.show_rename_dialog(ui.ctx(), vault, event_bus);
        self.show_delete_dialog(ui.ctx(), vault, event_bus);
    }

    /// Keep the panel's active row in sync with the app's open document.
    fn sync_active(&mut self, active: Option<&Path>) {
        let active = active.map(Path::to_path_buf);
        if active == self.last_active {
            self.active = active;
            return;
        }

        self.last_active = active.clone();
        self.active = active.clone();
        if let Some(path) = active {
            let mut current = path.parent();
            while let Some(dir) = current {
                if !dir.as_os_str().is_empty() {
                    self.expanded.insert(dir.to_path_buf());
                }
                current = dir.parent();
            }
            self.scroll_to_active = true;
        }
    }

    fn apply_action(
        &mut self,
        action: TreeAction,
        vault: &Vault,
        active: Option<&Path>,
        dirty: bool,
        event_bus: &mut EventBus,
    ) {
        self.notice = None;
        match action {
            TreeAction::NewNote(dir) => {
                let path = unique_path(vault, &dir, "Untitled", "md");
                match vault.write(&path, "") {
                    Ok(()) => {
                        self.expanded.extend(ancestors(&path));
                        event_bus.publish(Event::VaultChanged);
                        event_bus.publish(Event::NodeOpened(path));
                    }
                    Err(e) => self.notice = Some(format!("Could not create note: {e}")),
                }
            }
            TreeAction::NewFolder(dir) => {
                let path = unique_path(vault, &dir, "New Folder", "");
                match vault.create_dir(&path) {
                    Ok(()) => {
                        self.expanded.insert(path.clone());
                        event_bus.publish(Event::VaultChanged);
                    }
                    Err(e) => self.notice = Some(format!("Could not create folder: {e}")),
                }
            }
            TreeAction::Rename(path) => {
                if active == Some(path.as_path()) && dirty {
                    self.notice =
                        Some("Save or discard changes before renaming the open note.".into());
                    return;
                }
                let value = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();
                self.rename = Some(RenameState {
                    original: path,
                    value,
                });
            }
            TreeAction::Delete(path) => {
                if active == Some(path.as_path()) && dirty {
                    self.notice =
                        Some("Save or discard changes before deleting the open note.".into());
                    return;
                }
                self.confirm_delete = Some(path);
            }
            TreeAction::Reveal(path) => reveal_in_file_manager(vault, &path),
        }
    }

    fn show_rename_dialog(&mut self, ctx: &egui::Context, vault: &Vault, event_bus: &mut EventBus) {
        let Some(state) = &mut self.rename else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new("Rename")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("New name:");
                ui.text_edit_singleline(&mut state.value);
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.rename = None;
        } else if submit {
            let Some(state) = self.rename.take() else {
                return;
            };
            let new_name = state.value.trim();
            if new_name.is_empty() {
                self.notice = Some("Name cannot be empty.".into());
                return;
            }
            let mut target = parent_dir(&state.original).join(new_name);
            if state.original.extension().is_some() && target.extension().is_none() {
                target.set_extension("md");
            }
            if target != state.original && vault.resolve(&target).exists() {
                self.notice = Some("An entry with that name already exists.".into());
                return;
            }
            match vault.rename(&state.original, &target) {
                Ok(()) => {
                    self.expanded.extend(ancestors(&target));
                    event_bus.publish(Event::EntryRenamed(state.original, target));
                }
                Err(e) => self.notice = Some(format!("Rename failed: {e}")),
            }
        }
    }

    fn show_delete_dialog(&mut self, ctx: &egui::Context, vault: &Vault, event_bus: &mut EventBus) {
        let Some(path) = &self.confirm_delete else {
            return;
        };
        let is_dir = vault.resolve(path).is_dir();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("entry");
        let mut confirmed = false;
        let mut cancel = false;
        egui::Window::new("Delete")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Delete {name}{}?",
                    if is_dir {
                        " and everything inside it"
                    } else {
                        ""
                    }
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.confirm_delete = None;
        } else if confirmed {
            let Some(path) = self.confirm_delete.take() else {
                return;
            };
            let result = if is_dir {
                vault.delete_dir(&path)
            } else {
                vault.delete(&path)
            };
            match result {
                Ok(()) => event_bus.publish(Event::EntryDeleted(path)),
                Err(e) => self.notice = Some(format!("Delete failed: {e}")),
            }
        }
    }
}

impl Default for FileTreePanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Flatten the directory tree into the rows that should be visible.
fn build_rows(files: &[PathBuf], dirs: &[PathBuf], expanded: &HashSet<PathBuf>) -> Vec<TreeRow> {
    let mut children: HashMap<PathBuf, Vec<(PathBuf, String, bool)>> = HashMap::new();
    for dir in dirs {
        let parent = parent_dir(dir);
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        children
            .entry(parent)
            .or_default()
            .push((dir.clone(), name, true));
    }
    for file in files {
        let parent = parent_dir(file);
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        children
            .entry(parent)
            .or_default()
            .push((file.clone(), name, false));
    }
    for entries in children.values_mut() {
        entries.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
    }

    let mut rows = Vec::new();
    append_rows(Path::new(""), 0, &children, expanded, &mut rows);
    rows
}

fn append_rows(
    parent: &Path,
    depth: usize,
    children: &HashMap<PathBuf, Vec<(PathBuf, String, bool)>>,
    expanded: &HashSet<PathBuf>,
    rows: &mut Vec<TreeRow>,
) {
    let Some(entries) = children.get(parent) else {
        return;
    };
    for (path, name, is_dir) in entries {
        rows.push(TreeRow {
            path: path.clone(),
            name: name.clone(),
            is_dir: *is_dir,
            depth,
        });
        if *is_dir && expanded.contains(path) {
            append_rows(path, depth + 1, children, expanded, rows);
        }
    }
}

fn parent_dir(path: &Path) -> PathBuf {
    path.parent().unwrap_or(Path::new("")).to_path_buf()
}

fn ancestors(path: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut current = path.parent();
    while let Some(dir) = current {
        if !dir.as_os_str().is_empty() {
            result.push(dir.to_path_buf());
        }
        current = dir.parent();
    }
    result
}

/// Build a non-colliding path inside `dir`.
fn unique_path(vault: &Vault, dir: &Path, stem: &str, ext: &str) -> PathBuf {
    for index in 0.. {
        let name = if ext.is_empty() {
            if index == 0 {
                stem.to_string()
            } else {
                format!("{stem} {index}")
            }
        } else if index == 0 {
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

fn reveal_in_file_manager(vault: &Vault, path: &Path) {
    let full = vault.resolve(path);
    let target = if full.is_dir() {
        full
    } else {
        full.parent().unwrap_or(&full).to_path_buf()
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(target);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(target);
        command
    };
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(target);
        command
    };
    let _ = command.spawn();
}

/// Actions triggered from the tree context menu.
enum TreeAction {
    NewNote(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
    Reveal(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_tree_default() {
        let _panel = FileTreePanel::new();
    }

    #[test]
    fn build_rows_orders_directories_first_and_hides_collapsed_children() {
        let files = vec![
            PathBuf::from("b.md"),
            PathBuf::from("projects/alpha/note.md"),
        ];
        let dirs = vec![PathBuf::from("projects"), PathBuf::from("projects/alpha")];
        let expanded = HashSet::new();

        let rows = build_rows(&files, &dirs, &expanded);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, PathBuf::from("projects"));
        assert!(rows[0].is_dir);
        assert_eq!(rows[1].path, PathBuf::from("b.md"));
    }

    #[test]
    fn build_rows_includes_expanded_descendants_with_depth() {
        let files = vec![PathBuf::from("projects/alpha/note.md")];
        let dirs = vec![PathBuf::from("projects"), PathBuf::from("projects/alpha")];
        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::from("projects"));
        expanded.insert(PathBuf::from("projects/alpha"));

        let rows = build_rows(&files, &dirs, &expanded);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[2].name, "note.md");
    }

    #[test]
    fn visible_range_is_clamped() {
        let files_len = 100;
        let offset = 500.0;
        let viewport = 200.0;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let start = (offset / ITEM_HEIGHT).floor() as usize;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scroll offsets are non-negative screen coordinates"
        )]
        let mut end = ((offset + viewport) / ITEM_HEIGHT).ceil() as usize + 1;
        end = end.min(files_len).max(start);
        assert!(start < files_len);
        assert!(end <= files_len);
        assert!(end >= start);
    }
}
