//! Theme system for the editor GUI.
//!
//! A small design-token layer on top of egui: one [`Palette`] per light/dark
//! mode, plus shared spacing, radii, text styles and widget visuals. Panels and
//! custom paint code read tokens from [`palette`] so the whole app stays
//! visually consistent and theme changes take effect immediately.

use serde::{Deserialize, Serialize};

/// Available visual themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    /// Human-readable label for menus and settings.
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }
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
            accent_color: [99, 102, 241, 255], // indigo-500
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

    /// Whether this theme resolves to dark mode in `ctx`.
    pub fn is_dark(&self, ctx: &egui::Context) -> bool {
        match self.mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => ctx
                .system_theme()
                .is_none_or(|theme| theme == egui::Theme::Dark),
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

    /// Resolved tokens for the current theme mode.
    pub fn palette(&self, ctx: &egui::Context) -> Palette {
        if self.is_dark(ctx) {
            Palette::dark(self.accent())
        } else {
            Palette::light(self.accent())
        }
    }

    /// Apply this theme's visuals, spacing, and text styles to `ctx`.
    pub fn apply(&self, ctx: &egui::Context) {
        let palette = self.palette(ctx);
        let mut style = (*ctx.style()).clone();
        style.visuals = visuals(&palette);
        apply_spacing(&mut style.spacing);
        style.text_styles = text_styles();
        ctx.set_style(style);
    }

    /// Surface color for panels/cards.
    pub fn surface(&self) -> egui::Color32 {
        match self.mode {
            ThemeMode::Light => Palette::light(self.accent()).surface,
            ThemeMode::Dark | ThemeMode::System => Palette::dark(self.accent()).surface,
        }
    }

    /// Border color for subtle borders.
    pub fn border(&self) -> egui::Color32 {
        match self.mode {
            ThemeMode::Light => Palette::light(self.accent()).border,
            ThemeMode::Dark | ThemeMode::System => Palette::dark(self.accent()).border,
        }
    }
}

/// Resolved color tokens for one light/dark mode.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// App background / panel fill.
    pub bg: egui::Color32,
    /// Cards, inputs and other raised-but-quiet surfaces.
    pub surface: egui::Color32,
    /// Hovered/selected surfaces and popups.
    pub surface_raised: egui::Color32,
    pub border: egui::Color32,
    pub text: egui::Color32,
    pub text_muted: egui::Color32,
    pub accent: egui::Color32,
    pub accent_hover: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,
    pub selection: egui::Color32,
    /// Graph node fill.
    pub graph_node: egui::Color32,
    /// Graph edge stroke.
    pub graph_edge: egui::Color32,
    /// Unresolved-link placeholder fill.
    pub graph_placeholder: egui::Color32,
}

impl Palette {
    pub fn dark(accent: egui::Color32) -> Self {
        Self {
            bg: egui::Color32::from_rgb(14, 14, 17),
            surface: egui::Color32::from_rgb(21, 21, 26),
            surface_raised: egui::Color32::from_rgb(28, 28, 35),
            border: egui::Color32::from_rgb(42, 42, 51),
            text: egui::Color32::from_rgb(231, 231, 236),
            text_muted: egui::Color32::from_rgb(154, 154, 166),
            accent,
            accent_hover: egui::Color32::from_rgb(129, 140, 248),
            success: egui::Color32::from_rgb(34, 197, 94),
            warning: egui::Color32::from_rgb(245, 158, 11),
            danger: egui::Color32::from_rgb(239, 68, 68),
            selection: egui::Color32::from_rgba_unmultiplied(99, 102, 241, 90),
            graph_node: accent,
            graph_edge: egui::Color32::from_rgb(72, 72, 86),
            graph_placeholder: egui::Color32::from_rgb(58, 58, 68),
        }
    }

    pub fn light(accent: egui::Color32) -> Self {
        Self {
            bg: egui::Color32::from_rgb(247, 247, 249),
            surface: egui::Color32::WHITE,
            surface_raised: egui::Color32::WHITE,
            border: egui::Color32::from_rgb(228, 228, 233),
            text: egui::Color32::from_rgb(24, 24, 27),
            text_muted: egui::Color32::from_rgb(107, 107, 118),
            accent,
            accent_hover: egui::Color32::from_rgb(99, 102, 241),
            success: egui::Color32::from_rgb(22, 163, 74),
            warning: egui::Color32::from_rgb(217, 119, 6),
            danger: egui::Color32::from_rgb(220, 38, 38),
            selection: egui::Color32::from_rgba_unmultiplied(79, 70, 229, 55),
            graph_node: accent,
            graph_edge: egui::Color32::from_rgb(203, 203, 211),
            graph_placeholder: egui::Color32::from_rgb(224, 224, 230),
        }
    }
}

/// Read the resolved palette from the active egui style.
pub fn palette(ctx: &egui::Context) -> Palette {
    let visuals = &ctx.style().visuals;
    // The hover stroke holds the accent; `selection.stroke` is the readable
    // text/focus color on top of the accent-tinted selection fill.
    let accent = visuals.widgets.hovered.bg_stroke.color;
    if visuals.dark_mode {
        Palette::dark(accent)
    } else {
        Palette::light(accent)
    }
}

/// Install the bundled Inter and `JetBrains Mono` fonts.
///
/// Called once at startup. Falls back cleanly if the font list cannot be
/// extended, so a missing glyph family never prevents the app from running.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter.ttf")),
    );
    fonts.font_data.insert(
        "jetbrains_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono.ttf")),
    );

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "inter".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.insert(0, "jetbrains_mono".to_owned());
    }

    ctx.set_fonts(fonts);
}

/// Shared corner radii.
pub mod radius {
    pub const SM: f32 = 5.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
}

/// Shared spacing constants used by custom widgets.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 18.0;
    pub const XL: f32 = 28.0;
}

fn apply_spacing(spacing: &mut egui::Spacing) {
    spacing.item_spacing = egui::vec2(8.0, 6.0);
    spacing.button_padding = egui::vec2(10.0, 5.0);
    spacing.interact_size = egui::vec2(28.0, 28.0);
    spacing.indent = 18.0;
    spacing.window_margin = egui::Margin::same(12.0);
    spacing.menu_margin = egui::Margin::same(6.0);
    spacing.scroll.bar_width = 8.0;
    spacing.scroll.floating = true;
    spacing.scroll.bar_inner_margin = 4.0;
}

fn text_styles() -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
    use egui::{FontFamily, FontId, TextStyle};
    [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(19.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Name("SectionHeader".into()),
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Name("Caption".into()),
            FontId::new(12.0, FontFamily::Proportional),
        ),
    ]
    .into()
}

fn visuals(palette: &Palette) -> egui::Visuals {
    let mut visuals = if palette.bg.r() < 128 {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let rounding = egui::Rounding::same(radius::SM);
    let hover_fill = blend(palette.surface_raised, palette.accent, 0.16);
    let active_fill = blend(palette.surface_raised, palette.accent, 0.55);

    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.surface_raised;
    visuals.extreme_bg_color = palette.surface;
    visuals.faint_bg_color = palette.surface;
    visuals.code_bg_color = palette.surface_raised;
    visuals.window_stroke = egui::Stroke::new(1.0, palette.border);
    visuals.window_rounding = egui::Rounding::same(radius::LG);
    visuals.menu_rounding = egui::Rounding::same(radius::MD);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 24.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(70),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 16.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(60),
    };
    visuals.hyperlink_color = palette.accent_hover;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.danger;
    visuals.selection = egui::style::Selection {
        bg_fill: palette.selection,
        stroke: egui::Stroke::new(
            1.0,
            if palette.bg.r() < 128 {
                egui::Color32::from_rgb(240, 240, 245)
            } else {
                egui::Color32::from_rgb(24, 24, 27)
            },
        ),
    };
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.button_frame = true;
    visuals.striped = false;

    visuals.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: palette.surface,
        weak_bg_fill: palette.surface,
        bg_stroke: egui::Stroke::new(1.0, palette.border),
        rounding,
        fg_stroke: egui::Stroke::new(1.0, palette.text),
        expansion: 0.0,
    };
    visuals.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: palette.surface,
        weak_bg_fill: palette.surface,
        bg_stroke: egui::Stroke::new(1.0, palette.border),
        rounding,
        fg_stroke: egui::Stroke::new(1.0, palette.text),
        expansion: 0.0,
    };
    visuals.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: hover_fill,
        weak_bg_fill: hover_fill,
        bg_stroke: egui::Stroke::new(1.0, palette.accent),
        rounding,
        fg_stroke: egui::Stroke::new(1.0, palette.text),
        expansion: 0.0,
    };
    visuals.widgets.active = egui::style::WidgetVisuals {
        bg_fill: active_fill,
        weak_bg_fill: active_fill,
        bg_stroke: egui::Stroke::new(1.0, palette.accent),
        rounding,
        // `active.fg_stroke` is also egui's `strong_text_color`, so it must
        // stay readable as body text rather than becoming button-white.
        fg_stroke: egui::Stroke::new(1.0, palette.text),
        expansion: 0.0,
    };
    visuals.widgets.open = visuals.widgets.hovered;

    visuals
}

/// Linearly blend `from` toward `to` by `t` in the range 0..=1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "channels are clamped to 0..=255 before the cast"
)]
pub fn blend(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let lerp = |a: u8, b: u8| -> u8 {
        let value = f32::from(a) + (f32::from(b) - f32::from(a)) * t;
        value.clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgb(
        lerp(from.r(), to.r()),
        lerp(from.g(), to.g()),
        lerp(from.b(), to.b()),
    )
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

    #[test]
    fn palettes_have_readable_contrast() {
        for palette in [
            Palette::dark(Theme::new().accent()),
            Palette::light(Theme::new().accent()),
        ] {
            let contrast = luminance_contrast(palette.text, palette.bg);
            assert!(contrast >= 4.5, "text contrast was {contrast:.2}");
        }
    }

    #[test]
    fn blend_moves_between_colors() {
        let black = egui::Color32::BLACK;
        let white = egui::Color32::WHITE;
        assert_eq!(blend(black, white, 0.0), black);
        assert_eq!(blend(black, white, 1.0), white);
        assert_eq!(
            blend(black, white, 0.5),
            egui::Color32::from_rgb(127, 127, 127)
        );
    }

    fn luminance_contrast(a: egui::Color32, b: egui::Color32) -> f64 {
        let la = relative_luminance(a);
        let lb = relative_luminance(b);
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn relative_luminance(color: egui::Color32) -> f64 {
        let channel = |value: u8| -> f64 {
            let value = f64::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
}
