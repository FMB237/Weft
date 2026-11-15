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

/// One user action chosen in the document tab strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStripAction {
    /// Make a tab visible.
    Activate(usize),
    /// Close a tab.
    Close(usize),
    /// Move a tab into the other pane.
    MoveToOtherPane(usize),
    /// Close every tab except the active one.
    CloseOthers,
    /// Close every tab to the right of the active one.
    CloseToRight,
    /// Reopen the most recently closed tab.
    ReopenClosed,
    /// Create a note in a new tab.
    NewTab,
}

/// A strip of document tabs with a dirty marker, close buttons, and a
/// right-click menu. Returns the action the user picked, if any.
pub fn document_tabs(
    ui: &mut Ui,
    pane_index: usize,
    pane: &weft_core::workspace::Pane,
    active_dirty: bool,
    can_move: bool,
) -> Option<TabStripAction> {
    let palette = palette(ui.ctx());
    let mut action = None;

    ui.horizontal(|ui| {
        egui::ScrollArea::horizontal()
            .id_salt(("document_tab_strip", pane_index))
            .max_height(30.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in pane.tabs.iter().enumerate() {
                        let selected = index == pane.active;
                        let mut title = tab.title();
                        if tab.is_dirty() || (selected && active_dirty) {
                            title.push_str(" •");
                        }
                        let color = if selected {
                            palette.text
                        } else {
                            palette.text_muted
                        };
                        let response = ui.add(
                            egui::Button::new(RichText::new(title).color(color))
                                .frame(selected)
                                .min_size(egui::vec2(0.0, 24.0)),
                        );
                        if response.clicked() {
                            action = Some(TabStripAction::Activate(index));
                        }
                        if response.middle_clicked() {
                            action = Some(TabStripAction::Close(index));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Close").clicked() {
                                action = Some(TabStripAction::Close(index));
                                ui.close_menu();
                            }
                            if ui.button("Close others").clicked() {
                                action = Some(TabStripAction::CloseOthers);
                                ui.close_menu();
                            }
                            if ui.button("Close to the right").clicked() {
                                action = Some(TabStripAction::CloseToRight);
                                ui.close_menu();
                            }
                            if ui.button("Reopen closed tab").clicked() {
                                action = Some(TabStripAction::ReopenClosed);
                                ui.close_menu();
                            }
                            if can_move && ui.button("Move to other pane").clicked() {
                                action = Some(TabStripAction::MoveToOtherPane(index));
                                ui.close_menu();
                            }
                        });
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("×").color(palette.text_muted).size(13.0),
                                )
                                .frame(false)
                                .min_size(egui::vec2(18.0, 24.0)),
                            )
                            .on_hover_text("Close tab (Ctrl+W)")
                            .clicked()
                        {
                            action = Some(TabStripAction::Close(index));
                        }
                    }
                });
            });

        if icon_button(ui, "+", "New note (Ctrl+T)").clicked() {
            action = Some(TabStripAction::NewTab);
        }
    });

    action
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
