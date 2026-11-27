//! Timeline panel for viewing notes on a horizontal time axis.
//!
//! Shows notes distributed along a horizontal time axis by date range.

use crate::event_bus::{Event, EventBus};
use crate::widgets::chrome::section_header;
use chrono::{Local, NaiveDate};
use egui::{Align, Layout, RichText, Ui};
use std::collections::HashMap;
use weft_core::daily::daily_note_path;
use weft_core::vault::Vault;

#[allow(
    clippy::collapsible_if,
    clippy::needless_borrow,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Timeline panel uses safe unwraps and casts for date calculations"
)]
/// Timeline panel state.
#[derive(Debug, Default)]
pub struct TimelinePanel {
    /// Start date of the timeline.
    start_date: NaiveDate,
    /// End date of the timeline.
    end_date: NaiveDate,
    /// Cached notes by date.
    notes_by_date: HashMap<NaiveDate, Vec<std::path::PathBuf>>,
    /// Whether we need to refresh the notes.
    needs_refresh: bool,
}

impl TimelinePanel {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            start_date: today - chrono::Duration::days(30),
            end_date: today + chrono::Duration::days(30),
            notes_by_date: HashMap::new(),
            needs_refresh: true,
        }
    }

    /// Show the timeline panel.
    pub fn show(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        if self.needs_refresh {
            self.refresh_notes(vault);
        }

        ui.vertical(|ui| {
            // Header with range controls
            ui.horizontal(|ui| {
                section_header(ui, "Timeline");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Zoom In").clicked() {
                        self.zoom_in();
                        self.needs_refresh = true;
                    }
                    if ui.button("Zoom Out").clicked() {
                        self.zoom_out();
                        self.needs_refresh = true;
                    }
                    if ui.button("Today").clicked() {
                        let today = chrono::Local::now().date_naive();
                        self.start_date = today - chrono::Duration::days(30);
                        self.end_date = today + chrono::Duration::days(30);
                        self.needs_refresh = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Timeline
            self.show_timeline(ui, vault, event_bus);
        });
    }

    fn zoom_in(&mut self) {
        let mid = self.start_date + (self.end_date - self.start_date) / 2;
        let half_range = (self.end_date - self.start_date) / 4;
        self.start_date = mid - half_range;
        self.end_date = mid + half_range;
    }

    fn zoom_out(&mut self) {
        let mid = self.start_date + (self.end_date - self.start_date) / 2;
        let half_range = (self.end_date - self.start_date) / 2;
        self.start_date = mid - half_range * 2;
        self.end_date = mid + half_range * 2;
    }

    #[allow(clippy::collapsible_if)]
    fn refresh_notes(&mut self, vault: &Vault) {
        let config = vault.daily_config().clone();
        self.notes_by_date.clear();

        if let Ok(files) = vault.list_files() {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let file_clone = file.clone();
                    // Try to extract date from filename (daily note format)
                    let stem = file.file_stem().and_then(|s| s.to_str());
                    if let Some(stem) = stem {
                        if let Ok(date) =
                            NaiveDate::parse_from_str(stem, &config.format.replace(".md", ""))
                        {
                            self.notes_by_date
                                .entry(date)
                                .or_default()
                                .push(file_clone.clone());
                        }
                    }
                    // Also check frontmatter for date property
                    if let Some(fm_end) = content.find("---\n").or_else(|| content.find("---\r\n"))
                    {
                        let fm_content = &content[4..fm_end];
                        for line in fm_content.lines() {
                            if let Some(date_str) = line.strip_prefix("date:") {
                                let date_str = date_str.trim();
                                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                                    self.notes_by_date
                                        .entry(date)
                                        .or_default()
                                        .push(file_clone.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::collapsible_if, clippy::too_many_lines)]
    fn show_timeline(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        let range = self.end_date - self.start_date;
        let total_days = range.num_days();

        if total_days <= 0 {
            ui.weak("Invalid date range");
            return;
        }

        // Calculate scale: pixels per day
        let available_width = ui.available_width().max(600.0);
        let pixels_per_day = (available_width / total_days as f32).clamp(2.0, 50.0);

        // Show date range header
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} → {}",
                    self.start_date.format("%Y-%m-%d"),
                    self.end_date.format("%Y-%m-%d")
                ))
                .weak()
                .small(),
            );
        });
        ui.add_space(4.0);

        // Timeline bar
        let total_width = total_days as f32 * pixels_per_day;
        let (rect, response) = ui.allocate_at_least(
            egui::vec2(total_width.max(available_width), 60.0),
            egui::Sense::click_and_drag(),
        );

        // Draw timeline bar
        ui.painter()
            .rect_filled(rect.shrink(4.0), 4.0, ui.visuals().faint_bg_color);

        // Draw date markers
        let marker_interval = if total_days <= 30 {
            1
        } else if total_days <= 90 {
            7
        } else {
            30
        };

        for day_offset in (0..=total_days).step_by(marker_interval) {
            let date = self.start_date + chrono::Duration::days(day_offset);
            let x = rect.left() + day_offset as f32 * pixels_per_day;

            // Draw tick
            ui.painter().line_segment(
                [
                    egui::pos2(x, rect.top() + 20.0),
                    egui::pos2(x, rect.bottom() - 20.0),
                ],
                ui.visuals().widgets.noninteractive.bg_stroke,
            );

            // Date label
            ui.painter().text(
                egui::pos2(x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                date.format("%m/%d").to_string(),
                egui::FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
        }

        // Draw notes on timeline - collect click/hover info first
        let mut clicked_date: Option<NaiveDate> = None;
        let mut hovered_date: Option<NaiveDate> = None;

        for date in self.notes_by_date.keys() {
            if *date >= self.start_date && *date <= self.end_date {
                let day_offset = (*date - self.start_date).num_days();
                let x = rect.left() + day_offset as f32 * pixels_per_day;

                // Draw note marker
                ui.painter().circle_filled(
                    egui::pos2(x, rect.center().y),
                    6.0,
                    egui::Color32::from_rgb(99, 102, 241),
                );

                // Check hover
                if let Some(hover_pos) = response.hover_pos() {
                    if (hover_pos.x - x).abs() < 10.0 {
                        hovered_date = Some(*date);
                    }
                }

                // Check click
                if response.clicked() {
                    if let Some(click_pos) = response.interact_pointer_pos() {
                        if (click_pos.x - x).abs() < 10.0 {
                            clicked_date = Some(*date);
                        }
                    }
                }
            }
        }

        // Handle click
        if let Some(clicked_date) = clicked_date {
            let path = daily_note_path(&vault.root, vault.daily_config(), clicked_date);
            event_bus.publish(Event::NodeOpened(path));
        }

        // Handle drag to pan
        let _dragged = response.dragged();
        let drag_delta = response.drag_delta().x;
        #[allow(clippy::cast_possible_truncation)]
        let days_delta = (drag_delta / pixels_per_day).round() as i64;
        self.start_date -= chrono::Duration::days(days_delta);
        self.end_date -= chrono::Duration::days(days_delta);
        self.needs_refresh = true;

        // Handle hover tooltip (must be last use of response)
        if let Some(hovered_date) = hovered_date {
            if let Some(notes) = self.notes_by_date.get(&hovered_date) {
                let note_text = notes
                    .iter()
                    .map(|p| p.file_stem().and_then(|s| s.to_str()).unwrap_or("note"))
                    .collect::<Vec<_>>()
                    .join(", ");
                response.on_hover_text(format!(
                    "{} notes: {}",
                    hovered_date.format("%Y-%m-%d"),
                    note_text
                ));
            }
        }
    }
}
