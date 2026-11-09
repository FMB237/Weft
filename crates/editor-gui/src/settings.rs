//! Settings / preferences system.
//!
//! Persists user preferences (theme, font size, auto-save interval, …) across
//! sessions using egui's [`eframe::Storage`]. Changes are applied live so the
// UI reacts immediately without a restart.

use serde::{Deserialize, Serialize};

use crate::theme::{Theme, ThemeMode};

/// Storage key for the persisted settings.
const STORAGE_KEY: &str = "editor_settings";

/// Application-wide user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Visual theme.
    pub theme: Theme,
    /// Editor font size in points.
    pub font_size: f32,
    /// Auto-save interval in seconds (0 disables auto-save).
    pub auto_save_interval_secs: u64,
    /// Whether to show line numbers in the editor (reserved for future use).
    pub show_line_numbers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            font_size: 15.0,
            auto_save_interval_secs: 0,
            show_line_numbers: false,
        }
    }
}

impl Settings {
    /// Load settings from egui storage, falling back to defaults.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        storage
            .and_then(|s| s.get_string(STORAGE_KEY))
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default()
    }

    /// Save settings to egui storage.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(json) = serde_json::to_string(self) {
            storage.set_string(STORAGE_KEY, json);
        }
    }
}

/// Preferences panel rendered inside a modal window.
#[derive(Debug, Default)]
pub struct SettingsPanel {
    /// Whether a setting has changed and needs to be applied this frame.
    pub changed: bool,
}

impl SettingsPanel {
    /// Show the settings panel inside `ui`. Returns `true` when any setting
    /// was modified by the user this call.
    pub fn show(&mut self, ui: &mut egui::Ui, settings: &mut Settings) -> bool {
        self.changed = false;

        ui.heading("Preferences");
        ui.separator();

        // Theme.
        ui.label("Theme");
        egui::ComboBox::from_id_salt("settings_theme")
            .selected_text(format!("{:?}", settings.theme.mode))
            .show_ui(ui, |ui| {
                for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
                    if ui
                        .selectable_label(settings.theme.mode == mode, format!("{mode:?}"))
                        .clicked()
                        && settings.theme.mode != mode
                    {
                        settings.theme.mode = mode;
                        self.changed = true;
                    }
                }
            });
        ui.separator();

        // Font size.
        ui.label("Editor font size (pt)");
        let mut font_size = settings.font_size;
        ui.add(
            egui::DragValue::new(&mut font_size)
                .range(8.0..=48.0)
                .speed(0.5),
        );
        if (font_size - settings.font_size).abs() > f32::EPSILON {
            settings.font_size = font_size;
            self.changed = true;
        }
        ui.separator();

        // Auto-save interval.
        ui.label("Auto-save interval (seconds, 0 = off)");
        let mut interval = settings.auto_save_interval_secs;
        ui.add(egui::DragValue::new(&mut interval).range(0..=3600).speed(1));
        if interval != settings.auto_save_interval_secs {
            settings.auto_save_interval_secs = interval;
            self.changed = true;
        }
        ui.separator();

        ui.checkbox(
            &mut settings.show_line_numbers,
            "Show line numbers (experimental)",
        );

        self.changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default() {
        let s = Settings::default();
        assert_eq!(s.theme.mode, ThemeMode::System);
        assert!((s.font_size - 15.0).abs() < f32::EPSILON);
        assert_eq!(s.auto_save_interval_secs, 0);
        assert!(!s.show_line_numbers);
    }
}
