//! Properties panel for editing note frontmatter.
//!
//! Provides a form-style editor for typed properties. The panel never writes
//! to the vault itself: it edits the live buffer text and hands the new text
//! back to the app, so unsaved edits in the editor are not clobbered and the
//! note is saved through the normal path.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use egui::{CollapsingHeader, Grid, Id, Response, RichText, TextEdit, Ui, WidgetText};
use weft_core::document::Frontmatter;
use weft_core::parser::{parse_frontmatter_block, split_frontmatter};
use weft_core::properties::{
    PropertyDef, PropertyMap, PropertySchema, PropertyValue, frontmatter_to_properties,
};

/// Properties panel state.
#[derive(Debug)]
pub struct PropertiesPanel {
    /// Whether the panel is expanded.
    pub expanded: bool,
}

impl PropertiesPanel {
    #[must_use]
    pub fn new() -> Self {
        Self { expanded: true }
    }

    /// Show the properties for the note in `text` (the live buffer).
    ///
    /// Returns the rewritten note text when the user changed a property, so
    /// the caller can put it in the buffer and mark it dirty.
    pub fn show(&mut self, ui: &mut Ui, schema: &PropertySchema, text: &str) -> Option<String> {
        let (block, _) = split_frontmatter(text);
        let frontmatter = block.map(parse_frontmatter_block).unwrap_or_default();
        let mut props = frontmatter_to_properties(&frontmatter, Some(schema));
        let mut changed = false;

        CollapsingHeader::new(RichText::new("Properties").strong())
            .id_salt(Id::new("properties_header"))
            .default_open(self.expanded)
            .show(ui, |ui| {
                self.expanded = ui.is_visible();
                Grid::new("properties_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for def in schema.properties.values() {
                            changed |= show_property_row(ui, def, &mut props);
                        }

                        let mut unknown: Vec<String> = props
                            .keys()
                            .filter(|key| !schema.properties.contains_key(*key))
                            .cloned()
                            .collect();
                        unknown.sort();
                        for key in unknown {
                            if let Some(value) = props.get_mut(&key) {
                                changed |= show_unknown_row(ui, &key, value);
                            }
                        }
                    });
            });

        changed.then(|| rebuild(text, &props))
    }
}

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Render one row for a schema property.
fn show_property_row(ui: &mut Ui, def: &PropertyDef, props: &mut PropertyMap) -> bool {
    let value = props
        .entry(def.name.clone())
        .or_insert_with(|| def.prop_type.default_value());

    ui.label(WidgetText::RichText(
        RichText::new(&def.name)
            .strong()
            .color(ui.visuals().strong_text_color()),
    ));
    let response = show_value_editor(ui, value, &def.options);
    ui.end_row();
    response.changed()
}

/// Render one row for a property the schema does not know.
fn show_unknown_row(ui: &mut Ui, key: &str, value: &mut PropertyValue) -> bool {
    ui.label(WidgetText::RichText(
        RichText::new(key)
            .italics()
            .color(ui.visuals().weak_text_color()),
    ));
    let response = show_value_editor(ui, value, &[]);
    ui.end_row();
    response.changed()
}

/// Render the editor that matches the value's type.
fn show_value_editor(ui: &mut Ui, value: &mut PropertyValue, options: &[String]) -> Response {
    match value {
        PropertyValue::Text(text) | PropertyValue::Link(text) => {
            ui.add(TextEdit::singleline(text).desired_width(200.0))
        }
        PropertyValue::Number(number) => ui.add(egui::DragValue::new(number).speed(0.1)),
        PropertyValue::Date(date) => show_date_editor(ui, date),
        PropertyValue::Bool(flag) => ui.checkbox(flag, ""),
        PropertyValue::Select(current) => show_select(ui, current, options),
        PropertyValue::MultiSelect(selected) => show_multiselect(ui, selected, options),
        PropertyValue::Tags(tags) => show_tags_editor(ui, tags),
    }
}

/// A `YYYY-MM-DD` text field that clears the date when emptied.
fn show_date_editor(ui: &mut Ui, date: &mut Option<DateTime<FixedOffset>>) -> Response {
    let mut text = date
        .map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let response = ui.add(TextEdit::singleline(&mut text).desired_width(120.0));
    if response.changed() {
        *date = if text.trim().is_empty() {
            None
        } else {
            NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
                .ok()
                .map(|naive| naive.and_time(NaiveTime::MIN).and_utc().fixed_offset())
        };
    }
    response
}

/// A combo box over the allowed options.
fn show_select(ui: &mut Ui, current: &mut String, options: &[String]) -> Response {
    let before = current.clone();
    let mut response = egui::ComboBox::new(ui.id().with("select"), "")
        .selected_text(current.as_str())
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(current, option.clone(), option);
            }
            if current.is_empty() && !options.is_empty() {
                ui.separator();
                ui.label("(no selection)");
            }
        })
        .response;
    if *current != before {
        response.mark_changed();
    }
    response
}

/// A combo box whose options toggle membership in the list.
fn show_multiselect(ui: &mut Ui, selected: &mut Vec<String>, options: &[String]) -> Response {
    let before = selected.clone();
    let mut response = egui::ComboBox::new(ui.id().with("multiselect"), "")
        .selected_text(if selected.is_empty() {
            "(none)".to_string()
        } else {
            selected.join(", ")
        })
        .show_ui(ui, |ui| {
            for option in options {
                let mut is_selected = selected.contains(option);
                if ui.selectable_label(is_selected, option).clicked() {
                    is_selected = !is_selected;
                    if is_selected {
                        selected.push(option.clone());
                    } else {
                        selected.retain(|value| value != option);
                    }
                }
            }
        })
        .response;
    if *selected != before {
        response.mark_changed();
    }
    response
}

/// A comma-separated tag field.
fn show_tags_editor(ui: &mut Ui, tags: &mut Vec<String>) -> Response {
    let mut text = tags.join(", ");
    let response = ui.add(TextEdit::singleline(&mut text).desired_width(200.0));
    if response.changed() {
        *tags = text
            .split(',')
            .map(|tag| tag.trim().trim_start_matches('#').to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
    }
    response
}

/// Rewrite the note with the edited properties, preserving typed keys and
/// unknown keys alike.
fn rebuild(original: &str, props: &PropertyMap) -> String {
    let (block, body) = split_frontmatter(original);
    let frontmatter: Frontmatter = block.map(parse_frontmatter_block).unwrap_or_default();
    let mut table = frontmatter.to_toml_table();

    for (key, value) in props {
        if value.is_empty() {
            table.remove(key);
            continue;
        }
        match (key.as_str(), value) {
            ("title", PropertyValue::Text(title)) => {
                table.insert(key.clone(), toml::Value::String(title.clone()));
            }
            ("tags", PropertyValue::Tags(tags)) => {
                table.insert(
                    key.clone(),
                    toml::Value::Array(tags.iter().cloned().map(toml::Value::String).collect()),
                );
            }
            ("created" | "modified", PropertyValue::Date(Some(date))) => {
                table.insert(
                    key.clone(),
                    toml::Value::String(date.format("%Y-%m-%d").to_string()),
                );
            }
            _ => {
                table.insert(key.clone(), value.to_toml_value());
            }
        }
    }

    let header = if table.is_empty() {
        String::new()
    } else {
        toml::to_string(&table).map_or_else(|_| String::new(), |toml| format!("---\n{toml}---\n"))
    };
    format!("{header}{body}")
}
