//! Shared chrome widgets: cards, section headers, tabs, empty states, badges,
//! toasts, and the bottom status bar.
//!
//! These build on the tokens in [`crate::theme`] so every panel has the same
//! spacing, radii, and state colors.

use crate::theme::{Palette, palette, radius, space};
use egui::{Color32, Frame, Margin, Response, RichText, Stroke, Ui};

/// A quiet uppercase-ish label that introduces a group of controls.
pub fn section_header(ui: &mut Ui, text: &str) {
    let palette = palette(ui.ctx());
    ui.add_space(space::SM);
    ui.label(
        RichText::new(text.to_uppercase())
            .text_style(egui::TextStyle::Name("SectionHeader".into()))
            .color(palette.text_muted),
    );
    ui.add_space(space::XS);
}

/// A raised card with a subtle border.
pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let palette = palette(ui.ctx());
    Frame::none()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .rounding(radius::MD)
        .inner_margin(Margin::same(space::MD))
        .show(ui, add_contents)
}

/// A compact icon/label button with a tooltip.
pub fn icon_button(ui: &mut Ui, label: &str, tooltip: &str) -> Response {
    let palette = palette(ui.ctx());
    let response = ui.add(
        egui::Button::new(RichText::new(label).color(palette.text_muted))
            .frame(false)
            .min_size(egui::vec2(28.0, 28.0)),
    );
    response.on_hover_text(tooltip)
}

/// A one-line horizontal tab bar. Returns `Some(index)` when a tab is clicked.
pub fn tab_bar(ui: &mut Ui, active: usize, labels: &[&str]) -> Option<usize> {
    let palette = palette(ui.ctx());
    let mut clicked = None;

    ui.horizontal(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let selected = index == active;
            let color = if selected {
                palette.text
            } else {
                palette.text_muted
            };
            let response = ui.add(
                egui::Button::new(RichText::new(*label).color(color).strong())
                    .frame(false)
                    .min_size(egui::vec2(0.0, 28.0)),
            );
            if selected {
                let underline = egui::Rect::from_min_max(
                    egui::pos2(response.rect.left(), response.rect.bottom() - 2.0),
                    egui::pos2(response.rect.right(), response.rect.bottom()),
                );
                ui.painter()
                    .rect_filled(underline, radius::SM, palette.accent);
            }
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });

    clicked
}

/// Centered empty-state message with a title and a muted hint.
pub fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    let palette = palette(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space(space::XL);
        ui.label(RichText::new(title).size(16.0).strong().color(palette.text));
        ui.add_space(space::XS);
        ui.label(RichText::new(hint).color(palette.text_muted));
        ui.add_space(space::XL);
    });
}

/// A small pill-shaped colored label.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    // Accent-tinted text needs a lighter tone on dark surfaces to stay readable.
    let text_color = if ui.ctx().style().visuals.dark_mode {
        crate::theme::blend(color, Color32::WHITE, 0.35)
    } else {
        color
    };
    Frame::none()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .rounding(radius::SM)
        .inner_margin(Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(text_color));
        })
        .response
}

/// A dismissible-looking notice strip.
pub fn toast(ui: &mut Ui, text: &str, color: Color32, dismiss: &mut bool) {
    Frame::none()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .rounding(radius::SM)
        .inner_margin(Margin::symmetric(space::SM, space::XS))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(text).color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "✕", "Dismiss").clicked() {
                        *dismiss = true;
                    }
                });
            });
        });
}

/// Render the bottom status bar: left segments, flex space, right segments.
pub fn status_bar(ui: &mut Ui, palette: &Palette, left: &[String], right: &[String]) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        for (index, segment) in left.iter().enumerate() {
            if index > 0 {
                ui.label(RichText::new("·").color(palette.border));
            }
            ui.label(RichText::new(segment).size(11.0).color(palette.text_muted));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for segment in right.iter().rev() {
                ui.label(RichText::new(segment).size(11.0).color(palette.text_muted));
            }
        });
    });
    ui.add_space(2.0);
}
