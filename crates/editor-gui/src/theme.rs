//! Theme system for the editor GUI.
//!
//! Supports dark/light mode switching with beautiful, modern color palettes
// and persistence via egui's `eframe::Storage`.

use serde::{Deserialize, Serialize};

/// Available visual themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

/// Application theme settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent_color: [u8; 4],
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub fn new() -> Self {
        Self {
            mode: ThemeMode::System,
            accent_color: [99, 102, 241, 255], // indigo-500 - modern default
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent_color: [99, 102, 241, 255],
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            accent_color: [79, 70, 229, 255], // indigo-600 for light mode
        }
    }

    /// Returns the egui visual style for this theme.
    pub fn visuals(&self) -> egui::Visuals {
        match self.mode {
            ThemeMode::Dark | ThemeMode::System => egui::Visuals::dark(),
            ThemeMode::Light => egui::Visuals::light(),
        }
    }

    /// Accent color as `egui::Color32`.
    pub fn accent(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            self.accent_color[0],
            self.accent_color[1],
            self.accent_color[2],
            self.accent_color[3],
        )
    }

    /// Surface color for panels/cards.
    pub fn surface(&self) -> egui::Color32 {
        match self.mode {
            ThemeMode::Dark | ThemeMode::System => egui::Color32::from_rgb(17, 17, 19),
            ThemeMode::Light => egui::Color32::from_rgb(255, 255, 255),
        }
    }

    /// Border color for subtle borders.
    pub fn border(&self) -> egui::Color32 {
        match self.mode {
            ThemeMode::Dark | ThemeMode::System => egui::Color32::from_rgb(39, 39, 42),
            ThemeMode::Light => egui::Color32::from_rgb(229, 231, 235),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_default() {
        let theme = Theme::default();
        assert_eq!(theme.mode, ThemeMode::System);
    }

    #[test]
    fn theme_dark_and_light() {
        let dark = Theme::dark();
        assert_eq!(dark.mode, ThemeMode::Dark);
        let light = Theme::light();
        assert_eq!(light.mode, ThemeMode::Light);
    }

    #[test]
    fn theme_serialization() {
        let theme = Theme {
            mode: ThemeMode::Dark,
            accent_color: [255, 0, 0, 255],
        };
        let json = serde_json::to_string(&theme).unwrap();
        let loaded: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.mode, ThemeMode::Dark);
    }
}
