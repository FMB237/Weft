//! Views panel for table, cards, and board views over notes.
//!
//! Uses the core query engine to filter, sort, and group notes by properties.

use crate::event_bus::{Event, EventBus};
use egui::{
    Align, CollapsingHeader, ComboBox, Id, Layout, RichText, ScrollArea, Separator, TextEdit, Ui,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use weft_core::properties::PropertyValue;
use weft_core::query::{GroupBy, Query, SortDirection, SortKey};
use weft_core::vault::Vault;

/// View layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewLayout {
    #[default]
    Table,
    Cards,
    Board,
}

/// Views panel state.
#[derive(Debug)]
pub struct ViewsPanel {
    /// Current view layout.
    pub layout: ViewLayout,
    /// Current filter text for the filter bar.
    pub filter_text: String,
    /// Whether the filter bar is expanded.
    pub filter_expanded: bool,
    /// Current sort column and direction.
    pub sort: Option<(String, SortDirection)>,
    /// Current group-by property.
    pub group_by: Option<String>,
    /// Selected view from registry.
    pub selected_view_id: Option<String>,
    /// Currently dragged note path (for board view drag & drop).
    pub dragged_note: Option<String>,
    /// Source group key of the dragged note.
    pub dragged_from_group: Option<String>,
    /// Dialog state for saving a new view.
    pub save_view_dialog_open: bool,
    /// Name input for save view dialog.
    pub save_view_name: String,
    /// Source folder for the new view (if any).
    pub save_view_source_folder: Option<String>,
}

impl ViewsPanel {
    pub fn new() -> Self {
        Self {
            layout: ViewLayout::Table,
            filter_text: String::new(),
            filter_expanded: false,
            sort: None,
            group_by: None,
            selected_view_id: None,
            dragged_note: None,
            dragged_from_group: None,
            save_view_dialog_open: false,
            save_view_name: String::new(),
            save_view_source_folder: None,
        }
    }

    /// Show the views panel.
    pub fn show(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        // Build notes map for query
        let notes_map = self.build_notes_map(vault);

        // Build query from current state
        let query = self.build_query(vault);
        let results = query.evaluate(&notes_map);

        // View toolbar
        self.show_toolbar(ui, vault, Some(&results));

        ui.add_space(8.0);

        // Filter bar
        self.show_filter_bar(ui);

        ui.add_space(4.0);

        // Results count
        ui.label(
            RichText::new(format!(
                "{} of {} notes",
                results.total_matched,
                notes_map.len()
            ))
            .weak(),
        );

        ui.add_space(4.0);

        // Render view based on layout
        ScrollArea::vertical().show(ui, |ui| match self.layout {
            ViewLayout::Table => self.show_table_view(ui, vault, event_bus, &results),
            ViewLayout::Cards => self.show_cards_view(ui, vault, event_bus, &results),
            ViewLayout::Board => self.show_board_view(ui, vault, event_bus, &results),
        });
    }

    fn build_notes_map(&self, vault: &Vault) -> HashMap<String, HashMap<String, PropertyValue>> {
        let _ = self;
        let mut notes_map = HashMap::new();

        if let Ok(files) = vault.list_files() {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let frontmatter = weft_core::parser::parse_frontmatter(&content);
                    let schema = vault.property_schema();
                    let props = weft_core::properties::frontmatter_to_properties(
                        &frontmatter,
                        Some(schema),
                    );
                    notes_map.insert(file.to_string_lossy().to_string(), props);
                }
            }
        }

        notes_map
    }

    fn build_query(&self, vault: &Vault) -> Query {
        // Start with selected view's query if one is selected
        let mut query = if let Some(view_id) = &self.selected_view_id {
            if let Some(view) = vault.view_registry().views.get(view_id) {
                view.query.clone()
            } else {
                Query::new()
            }
        } else {
            Query::new()
        };

        // Apply filter from filter_text (overrides view filter if both exist)
        if let Some(filter) = {
            let trimmed = self.filter_text.trim();
            if trimmed.is_empty() {
                None
            } else {
                weft_core::query::parse_simple_filter(trimmed)
            }
        } {
            query.filter = weft_core::query::FilterGroup {
                all: true,
                conditions: vec![filter],
                groups: vec![],
            };
        }

        // Apply sort (overrides view sort)
        if let Some((prop, dir)) = &self.sort {
            query.sort = vec![SortKey {
                property: prop.clone(),
                direction: *dir,
            }];
        }

        // Apply group by (overrides view group_by)
        if let Some(prop) = &self.group_by {
            query.group_by = Some(GroupBy {
                property: prop.clone(),
                show_empty: false,
                limit: 0,
            });
        }

        query
    }

    fn show_toolbar(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        results: Option<&weft_core::query::QueryResult>,
    ) {
        ui.horizontal(|ui| {
            // Layout selector
            ComboBox::new("view_layout", "")
                .selected_text(match self.layout {
                    ViewLayout::Table => "📋 Table",
                    ViewLayout::Cards => "🃏 Cards",
                    ViewLayout::Board => "📊 Board",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.layout, ViewLayout::Table, "📋 Table");
                    ui.selectable_value(&mut self.layout, ViewLayout::Cards, "🃏 Cards");
                    ui.selectable_value(&mut self.layout, ViewLayout::Board, "📊 Board");
                });

            ui.separator();

            // View selector from registry
            let registry = vault.view_registry();
            let view_names: Vec<_> = registry.views.values().map(|v| v.name.clone()).collect();
            let view_ids: Vec<_> = registry.views.keys().cloned().collect();

            let current_name = self
                .selected_view_id
                .as_ref()
                .and_then(|id| registry.views.get(id))
                .map(|v| v.name.clone())
                .map_or("Select view...".to_string(), |s| s);

            ComboBox::new("view_selector", "")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_view_id, None, "Select view...");
                    for (id, name) in view_ids.iter().zip(view_names.iter()) {
                        ui.selectable_value(&mut self.selected_view_id, Some(id.clone()), name);
                    }
                    ui.separator();
                    if ui.button("Save current as view...").clicked() {
                        self.open_save_view_dialog(vault);
                    }
                });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // CSV export for table view
                if self.layout == ViewLayout::Table
                    && let Some(results) = results
                    && ui.button("📥 Export CSV").clicked()
                {
                    self.export_csv(vault, results);
                }
                if ui.button("➕ New View").clicked() {
                    self.open_save_view_dialog(vault);
                }
            });
        });
    }

    /// Open the "Save as view" dialog.
    fn open_save_view_dialog(&mut self, _vault: &Vault) {
        self.save_view_dialog_open = true;
        self.save_view_name.clear();
        self.save_view_source_folder = None;

        // If a folder is selected in the current tab, use it as the source folder
        // For now, just leave it as None (global view)
    }

    fn export_csv(&self, vault: &Vault, results: &weft_core::query::QueryResult) {
        let columns = self.get_visible_columns(vault);
        let _schema = vault.property_schema();

        // Build CSV header
        let mut csv = String::new();
        csv.push_str("Title");
        for col in &columns {
            csv.push(',');
            csv.push_str(&escape_csv(col));
        }
        csv.push('\n');

        // Build CSV rows
        for (path, props) in &results.notes {
            let title = props
                .get("title")
                .and_then(|v| v.as_text())
                .cloned()
                .unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Untitled")
                        .to_string()
                });
            csv.push_str(&escape_csv(&title));
            for col in &columns {
                csv.push(',');
                let value = props.get(col);
                let display = value.map_or_else(String::new, PropertyValue::display);
                csv.push_str(&escape_csv(&display));
            }
            csv.push('\n');
        }

        // Save to file
        let vault_root = &vault.root;
        let export_path = vault_root.join(".weft").join("exports");
        let _ = fs::create_dir_all(&export_path);
        let file_name = format!(
            "export-{}.csv",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        let file_path = export_path.join(file_name);
        let _ = fs::write(&file_path, csv);

        // TODO: Show notification that export was saved
    }

    /// Render the "Save as view" modal dialog.
    pub fn show_save_view_dialog(&mut self, ctx: &egui::Context, vault: &mut Vault) {
        if !self.save_view_dialog_open {
            return;
        }

        let mut open = self.save_view_dialog_open;
        let mut should_close = false;

        // Clone the fields we need for the closure to avoid borrow issues
        let save_view_name = &mut self.save_view_name;
        let save_view_source_folder = &mut self.save_view_source_folder;

        egui::Window::new("Save Current as View")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label("Save the current filter, sort, group, and layout settings as a named view.");
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(TextEdit::singleline(save_view_name).desired_width(250.0));
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Scope:");
                        ComboBox::new("view_scope", "")
                            .selected_text(
                                save_view_source_folder
                                    .as_ref()
                                    .map_or("Global (all notes)".to_string(), |s| format!("Folder: {s}")),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    save_view_source_folder,
                                    None,
                                    "Global (all notes)",
                                );
                                // TODO: Add folder options from vault
                            });
                    });

                    ui.add_space(16.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                        if ui.add_enabled(!save_view_name.trim().is_empty(), egui::Button::new("Save"))
                            .clicked()
                        {
                            // We can't call save_view here due to borrow checker
                            // Just mark to close and save after the window
                            should_close = true;
                        }
                    });
                });
            });

        if should_close && !self.save_view_name.trim().is_empty() {
            self.save_view(vault);
        }
        if should_close {
            self.save_view_dialog_open = false;
        } else {
            self.save_view_dialog_open = open;
        }
    }

    /// Save the current view state to the registry.
    fn save_view(&mut self, vault: &mut Vault) -> bool {
        let name = self.save_view_name.trim();
        if name.is_empty() {
            return false;
        }

        let query = self.build_query(vault);
        let columns = if self.layout == ViewLayout::Table {
            self.get_visible_columns(vault)
        } else {
            Vec::new()
        };

        let view_id = uuid::Uuid::new_v4().to_string();
        let view = weft_core::views::View {
            id: view_id.clone(),
            name: name.to_string(),
            query,
            columns,
            show_path: true,
            icon: None,
        };

        vault
            .view_registry_mut()
            .views
            .insert(view_id.clone(), view);
        let _ = vault.save_view_registry();
        self.selected_view_id = Some(view_id);
        true
    }

    fn show_filter_bar(&mut self, ui: &mut Ui) {
        CollapsingHeader::new("🔍 Filter")
            .id_salt(Id::new("filter_header"))
            .default_open(self.filter_expanded)
            .show(ui, |ui| {
                self.filter_expanded = ui.is_visible();
                ui.horizontal(|ui| {
                    ui.label("Query:");
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.filter_text)
                            .hint_text("status:done tag:meeting priority:>5")
                            .desired_width(300.0),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        // Trigger re-query
                    }
                    if ui.button("Clear").clicked() {
                        self.filter_text.clear();
                    }
                });
                ui.weak(
                    "Syntax: prop:value, prop:\"text\", prop:>date, prop:<date, -prop:value, #tag",
                );
            });
    }

    fn show_table_view(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        event_bus: &mut EventBus,
        results: &weft_core::query::QueryResult,
    ) {
        use egui_extras::{Column, TableBuilder};

        let columns = self.get_visible_columns(vault);
        let schema = vault.property_schema();

        // Build table with all columns upfront
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(40.0).at_least(30.0).at_most(60.0)) // checkbox/select
            .column(Column::initial(200.0).at_least(150.0)); // title

        for _ in &columns {
            table = table.column(Column::initial(120.0).at_least(80.0));
        }

        // Use the TableBuilder::header and body pattern correctly
        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.weak("#");
                });
                header.col(|ui| {
                    ui.strong("Title");
                });
                for col in &columns {
                    header.col(|ui| {
                        let label = if self.sort.as_ref()
                            == Some(&(col.clone(), SortDirection::Asc))
                        {
                            format!("▲ {col}")
                        } else if self.sort.as_ref() == Some(&(col.clone(), SortDirection::Desc)) {
                            format!("▼ {col}")
                        } else {
                            col.clone()
                        };
                        if ui.button(label).clicked() {
                            self.sort = Some((
                                col.clone(),
                                if self.sort.as_ref() == Some(&(col.clone(), SortDirection::Asc)) {
                                    SortDirection::Desc
                                } else {
                                    SortDirection::Asc
                                },
                            ));
                        }
                    });
                }
            })
            .body(|mut body| {
                for (idx, (path, props)) in results.notes.iter().enumerate() {
                    let path_clone = path.clone();
                    body.row(24.0, |mut row| {
                        row.col(|ui| {
                            ui.weak(format!("{}", idx + 1));
                        });
                        row.col(|ui| {
                            // Extract title from properties or use path stem
                            let title = props
                                .get("title")
                                .and_then(|v| v.as_text())
                                .cloned()
                                .unwrap_or_else(|| {
                                    std::path::Path::new(path)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("Untitled")
                                        .to_string()
                                });
                            if ui.link(title).clicked() {
                                event_bus
                                    .publish(Event::NodeOpened(std::path::PathBuf::from(path)));
                            }
                        });
                        for col in &columns {
                            let prop_def = schema.properties.get(col);
                            let prop_type = prop_def.map(|d| d.prop_type);
                            row.col(|ui| {
                                let value = props.get(col);
                                self.show_editable_property_cell(
                                    ui,
                                    vault,
                                    &path_clone,
                                    col,
                                    value,
                                    prop_type,
                                    event_bus,
                                );
                            });
                        }
                    });
                }
            });
    }

    fn show_editable_property_cell(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        note_path: &str,
        property_name: &str,
        value: Option<&PropertyValue>,
        prop_type: Option<weft_core::properties::PropertyType>,
        event_bus: &mut EventBus,
    ) {
        let current_display = value.map_or_else(String::new, PropertyValue::display);
        let mut edit_text = current_display.clone();
        let response = ui.add(
            TextEdit::singleline(&mut edit_text)
                .desired_width(f32::INFINITY)
                .frame(false),
        );

        // If edited and lost focus (Enter or click away), save
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if edit_text != current_display {
                self.save_single_property(
                    vault,
                    note_path,
                    property_name,
                    &edit_text,
                    prop_type,
                    event_bus,
                );
            }
        } else if response.changed() && !response.has_focus() {
            // Also save on click away if changed
            self.save_single_property(
                vault,
                note_path,
                property_name,
                &edit_text,
                prop_type,
                event_bus,
            );
        }
    }

    fn save_single_property(
        &self,
        vault: &Vault,
        note_path: &str,
        property_name: &str,
        edit_text: &str,
        prop_type: Option<weft_core::properties::PropertyType>,
        event_bus: &mut EventBus,
    ) {
        let _ = self;
        // Parse the new value based on property type
        let new_value = match prop_type {
            Some(weft_core::properties::PropertyType::Number) => {
                if let Ok(n) = edit_text.parse::<f64>() {
                    weft_core::properties::PropertyValue::Number(n)
                } else {
                    return; // Invalid number, don't save
                }
            }
            Some(weft_core::properties::PropertyType::Bool) => {
                let lower = edit_text.to_lowercase();
                if lower == "true" || lower == "yes" || lower == "1" {
                    weft_core::properties::PropertyValue::Bool(true)
                } else if lower == "false" || lower == "no" || lower == "0" {
                    weft_core::properties::PropertyValue::Bool(false)
                } else {
                    return; // Invalid bool, don't save
                }
            }
            Some(weft_core::properties::PropertyType::Date) => {
                if edit_text.is_empty() {
                    weft_core::properties::PropertyValue::Date(None)
                } else if let Ok(naive) = chrono::NaiveDate::parse_from_str(edit_text, "%Y-%m-%d") {
                    let fixed = naive
                        .and_hms_opt(0, 0, 0)
                        .expect("midnight is always valid")
                        .and_utc()
                        .into();
                    weft_core::properties::PropertyValue::Date(Some(fixed))
                } else {
                    return; // Invalid date, don't save
                }
            }
            Some(weft_core::properties::PropertyType::Select) => {
                weft_core::properties::PropertyValue::Select(edit_text.to_string())
            }
            Some(weft_core::properties::PropertyType::MultiSelect) => {
                let items: Vec<String> = edit_text
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                weft_core::properties::PropertyValue::MultiSelect(items)
            }
            Some(weft_core::properties::PropertyType::Link) => {
                weft_core::properties::PropertyValue::Link(edit_text.to_string())
            }
            Some(weft_core::properties::PropertyType::Tags) => {
                let items: Vec<String> = edit_text
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                weft_core::properties::PropertyValue::Tags(items)
            }
            _ => weft_core::properties::PropertyValue::Text(edit_text.to_string()),
        };

        // Read current content
        let Ok(content) = vault.read(Path::new(note_path)) else {
            return;
        };

        // Parse frontmatter and body
        let (frontmatter_block, body) = weft_core::parser::split_frontmatter(&content);
        let frontmatter = frontmatter_block
            .map(weft_core::parser::parse_frontmatter_block)
            .unwrap_or_default();
        let mut fm = frontmatter;

        // Update the property
        if new_value.is_empty() {
            fm.other.remove(property_name);
        } else {
            fm.other
                .insert(property_name.to_string(), new_value.to_toml_value());
        }

        // Reconstruct content
        let new_frontmatter = if fm.is_empty() {
            String::new()
        } else {
            format!("---\n{}---\n", toml::to_string(&fm).unwrap_or_default())
        };
        let new_content = format!("{new_frontmatter}{body}");

        // Write atomically
        let _ = vault.write(Path::new(note_path), &new_content);

        // Notify app of change via event bus
        event_bus.publish(Event::DocumentSaved(Path::new(note_path).to_path_buf()));
    }

    fn show_cards_view(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        event_bus: &mut EventBus,
        results: &weft_core::query::QueryResult,
    ) {
        let card_width = 280.0;
        let spacing = 12.0;

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            ui.spacing_mut().item_spacing.y = spacing;

            for (path, props) in &results.notes {
                let (card_rect, card_response) =
                    ui.allocate_at_least(egui::vec2(card_width, 160.0), egui::Sense::click());

                let rect = card_rect.shrink(4.0);

                // Card background
                ui.painter()
                    .rect_filled(rect, 8.0, ui.visuals().faint_bg_color);
                ui.painter()
                    .rect_stroke(rect, 8.0, ui.visuals().widgets.noninteractive.bg_stroke);

                // Card content - use the parent ui with a restricted clip rect via new_child
                let content_rect = rect.shrink(8.0);
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));

                content_ui.vertical(|ui| {
                    // Title
                    let title = props
                        .get("title")
                        .and_then(|v| v.as_text())
                        .cloned()
                        .unwrap_or_else(|| {
                            std::path::Path::new(path)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Untitled")
                                .to_string()
                        });
                    ui.add(
                        egui::Label::new(RichText::new(title).strong())
                            .wrap()
                            .truncate(),
                    );

                    ui.add_space(8.0);

                    // Properties preview
                    let card_props = self.get_card_properties(vault, props);
                    for (key, value) in card_props.iter().take(3) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(key).weak().small());
                            ui.label(RichText::new(value).small());
                        });
                    }

                    ui.add_space(8.0);

                    // Tags
                    if let Some(PropertyValue::Tags(tags)) = props.get("tags") {
                        ui.horizontal_wrapped(|ui| {
                            for tag in tags.iter().take(5) {
                                let tag_rect = ui.allocate_at_least(
                                    egui::vec2(40.0, 18.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    tag_rect.1.rect,
                                    4.0,
                                    ui.visuals().selection.bg_fill,
                                );
                                ui.painter().text(
                                    tag_rect.1.rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("#{tag}"),
                                    egui::FontId::proportional(11.0),
                                    ui.visuals().selection.stroke.color,
                                );
                            }
                        });
                    }
                });

                if card_response.clicked() {
                    event_bus.publish(Event::NodeOpened(std::path::PathBuf::from(path)));
                }
            }
        });
    }

    fn show_board_view(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        event_bus: &mut EventBus,
        results: &weft_core::query::QueryResult,
    ) {
        // Group by the group_by property (should be a select/multi-select)
        let group_prop = self
            .group_by
            .clone()
            .unwrap_or_else(|| "status".to_string());

        // Get groups from results
        let groups = &results.groups;

        let Some(groups) = groups else {
            ui.centered_and_justified(|ui| {
                ui.weak("No groups found. Set a group-by property in the view settings.");
            });
            return;
        };

        if groups.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No groups found. Set a group-by property in the view settings.");
            });
            return;
        }

        // Track drop payload and target
        let mut dropped_payload: Option<BoardDragPayload> = None;
        let mut drop_target_group: Option<String> = None;

        // Use a frame for drop zones
        let frame = egui::Frame::none();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;

            for group in groups {
                let col_width = 280.0;
                let group_key = group.key.clone();

                // Make the entire column a drop target
                let (_, drop_payload) = ui.dnd_drop_zone::<BoardDragPayload, _>(frame, |ui| {
                    ui.allocate_ui(egui::vec2(col_width, ui.available_height()), |ui| {
                        // Column header
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&group_key);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.weak(format!("{} cards", group.notes.len()));
                                });
                            });
                            ui.add(Separator::default());

                            // Cards in this column
                            ScrollArea::vertical().show(ui, |ui| {
                                for (path, props) in &group.notes {
                                    let path_clone = path.clone();
                                    let (card_rect, _card_response) = ui.allocate_at_least(
                                        egui::vec2(col_width - 16.0, 100.0),
                                        egui::Sense::click_and_drag(),
                                    );

                                    let rect = card_rect.shrink(4.0);

                                    // Highlight if this is the dragged card
                                    let is_dragged = self.dragged_note.as_ref() == Some(path);
                                    let bg_color = if is_dragged {
                                        ui.visuals().selection.bg_fill
                                    } else {
                                        ui.visuals().faint_bg_color
                                    };
                                    ui.painter().rect_filled(rect, 6.0, bg_color);
                                    ui.painter().rect_stroke(
                                        rect,
                                        6.0,
                                        ui.visuals().widgets.noninteractive.bg_stroke,
                                    );

                                    // Make card a drag source
                                    let drag_payload = BoardDragPayload {
                                        note_path: path_clone.clone(),
                                        from_group: group_key.clone(),
                                    };

                                    let drag_source_response = ui.dnd_drag_source(
                                        Id::new("board_card").with(path),
                                        drag_payload,
                                        |ui| {
                                            let content_rect = rect.shrink(8.0);
                                            let mut content_ui = ui.new_child(
                                                egui::UiBuilder::new().max_rect(content_rect),
                                            );

                                            content_ui.vertical(|ui| {
                                                let title = props
                                                    .get("title")
                                                    .and_then(|v| v.as_text())
                                                    .cloned()
                                                    .unwrap_or_else(|| {
                                                        std::path::Path::new(path)
                                                            .file_stem()
                                                            .and_then(|s| s.to_str())
                                                            .unwrap_or("Untitled")
                                                            .to_string()
                                                    });
                                                if ui.link(RichText::new(title).strong()).clicked()
                                                {
                                                    event_bus.publish(Event::NodeOpened(
                                                        std::path::PathBuf::from(path),
                                                    ));
                                                }
                                                ui.add_space(4.0);
                                                // Show a couple properties
                                                let card_props =
                                                    self.get_card_properties(vault, props);
                                                for (key, value) in card_props.iter().take(2) {
                                                    ui.horizontal(|ui| {
                                                        ui.label(RichText::new(key).weak().small());
                                                        ui.label(RichText::new(value).small());
                                                    });
                                                }
                                            });
                                        },
                                    );

                                    // Check if drag was released (dropped) on this drop zone
                                    if let Some(payload) = drag_source_response
                                        .response
                                        .dnd_release_payload::<BoardDragPayload>(
                                    ) {
                                        dropped_payload = Some(payload.as_ref().clone());
                                        drop_target_group = Some(group_key.clone());
                                    }
                                }
                            });
                        });
                    });
                });

                if let Some(payload) = drop_payload {
                    dropped_payload = Some(payload.as_ref().clone());
                    drop_target_group = Some(group_key.clone());
                }
            }
        });

        // Handle the drop after rendering to avoid borrow issues
        if let (Some(payload), Some(target_group)) = (dropped_payload, drop_target_group) {
            // Only update if dropped on a different group
            if payload.from_group != target_group {
                self.move_note_to_group(
                    vault,
                    &payload.note_path,
                    &group_prop,
                    &target_group,
                    event_bus,
                );
            }
            // Clear drag state
            self.dragged_note = None;
            self.dragged_from_group = None;
        }
    }

    /// Move a note to a different group by updating its property value.
    fn move_note_to_group(
        &mut self,
        vault: &Vault,
        note_path: &str,
        group_prop: &str,
        new_group_value: &str,
        event_bus: &mut EventBus,
    ) {
        use weft_core::properties::PropertyValue;
        let _ = self;

        // Read current content
        let Ok(content) = vault.read(Path::new(note_path)) else {
            return;
        };

        // Parse frontmatter and body
        let (frontmatter_block, body) = weft_core::parser::split_frontmatter(&content);
        let frontmatter = frontmatter_block
            .map(weft_core::parser::parse_frontmatter_block)
            .unwrap_or_default();
        let mut fm = frontmatter;

        // Determine the property type from schema to set the right value
        let schema = vault.property_schema();
        let new_value = if let Some(def) = schema.properties.get(group_prop) {
            match def.prop_type {
                weft_core::properties::PropertyType::Select => {
                    PropertyValue::Select(new_group_value.to_string())
                }
                weft_core::properties::PropertyType::MultiSelect => {
                    PropertyValue::MultiSelect(vec![new_group_value.to_string()])
                }
                weft_core::properties::PropertyType::Text => {
                    PropertyValue::Text(new_group_value.to_string())
                }
                _ => PropertyValue::Text(new_group_value.to_string()),
            }
        } else {
            PropertyValue::Text(new_group_value.to_string())
        };

        if new_value.is_empty() {
            fm.other.remove(group_prop);
        } else {
            fm.other
                .insert(group_prop.to_string(), new_value.to_toml_value());
        }

        // Reconstruct content
        let new_frontmatter = if fm.is_empty() {
            String::new()
        } else {
            format!("---\n{}---\n", toml::to_string(&fm).unwrap_or_default())
        };
        let new_content = format!("{new_frontmatter}{body}");

        // Write atomically
        let _ = vault.write(Path::new(note_path), &new_content);

        // Notify app of change via event bus
        event_bus.publish(Event::DocumentSaved(Path::new(note_path).to_path_buf()));
    }

    fn get_visible_columns(&self, vault: &Vault) -> Vec<String> {
        // Use selected view's columns if available
        if let (Some(_view_id), Some(view)) = (
            self.selected_view_id.as_ref(),
            self.selected_view_id
                .as_ref()
                .and_then(|id| vault.view_registry().views.get(id)),
        ) {
            if !view.columns.is_empty() {
                return view.columns.clone();
            }
        }
        // Default to schema-defined properties that are not hidden
        let schema = vault.property_schema();
        schema
            .properties
            .values()
            .filter(|d| !d.name.starts_with('_'))
            .map(|d| d.name.clone())
            .collect()
    }

    fn get_card_properties(
        &self,
        vault: &Vault,
        props: &HashMap<String, PropertyValue>,
    ) -> Vec<(String, String)> {
        let columns = self.get_visible_columns(vault);
        let schema = vault.property_schema();
        columns
            .iter()
            .filter_map(|col| {
                schema.properties.get(col).and_then(|def| {
                    props
                        .get(&def.name)
                        .map(|v| (def.name.clone(), v.display()))
                })
            })
            .collect()
    }
}

impl Default for ViewsPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Payload for board view drag and drop.
#[derive(Clone)]
struct BoardDragPayload {
    note_path: String,
    from_group: String,
}

// Extension trait for PropertyValue to get text easily
trait PropertyValueExt {
    fn as_text(&self) -> Option<&String>;
}

impl PropertyValueExt for PropertyValue {
    fn as_text(&self) -> Option<&String> {
        match self {
            PropertyValue::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// Escape a string for CSV output.
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let mut escaped = String::with_capacity(s.len() + 2);
        escaped.push('"');
        for ch in s.chars() {
            if ch == '"' {
                escaped.push_str("\"\"");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('"');
        escaped
    } else {
        s.to_string()
    }
}
