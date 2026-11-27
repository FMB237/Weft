//! Calendar panel for viewing notes by date.
//!
//! Shows a month grid with notes for each day. Click a day to open its daily
//! note or filter notes by that date.

use crate::event_bus::{Event, EventBus};
use chrono::{Datelike, Local, NaiveDate};
use egui::{Align, Direction, Grid, Layout, RichText, Ui};
use std::collections::HashMap;
use weft_core::daily::daily_note_path;
use weft_core::vault::Vault;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_borrow,
    reason = "Calendar panel uses safe unwraps and casts for date calculations"
)]
/// Calendar panel state.
#[derive(Debug, Default)]
pub struct CalendarPanel {
    /// Current month being displayed.
    current_month: NaiveDate,
    /// Cached notes by date.
    notes_by_date: HashMap<NaiveDate, Vec<std::path::PathBuf>>,
    /// Whether we need to refresh the notes.
    needs_refresh: bool,
}

impl CalendarPanel {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            current_month: NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                .expect("valid date"),
            notes_by_date: HashMap::new(),
            needs_refresh: true,
        }
    }

    /// Show the calendar panel.
    pub fn show(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        if self.needs_refresh {
            self.refresh_notes(vault);
        }

        ui.vertical(|ui| {
            // Header with month navigation
            ui.horizontal(|ui| {
                if ui.button("◀").on_hover_text("Previous month").clicked() {
                    self.current_month = self.prev_month();
                    self.needs_refresh = true;
                }
                ui.with_layout(
                    Layout::centered_and_justified(Direction::LeftToRight),
                    |ui| {
                        ui.label(
                            RichText::new(self.current_month.format("%B %Y").to_string())
                                .strong()
                                .size(18.0),
                        );
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("▶").on_hover_text("Next month").clicked() {
                        self.current_month = self.next_month();
                        self.needs_refresh = true;
                    }
                    if ui.button("Today").clicked() {
                        let today = chrono::Local::now().date_naive();
                        self.current_month =
                            NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                                .expect("valid date");
                        self.needs_refresh = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Month grid
            self.show_month_grid(ui, vault, event_bus);
        });
    }

    fn prev_month(&self) -> NaiveDate {
        let year = self.current_month.year();
        let month = self.current_month.month();
        if month == 1 {
            NaiveDate::from_ymd_opt(year - 1, 12, 1).expect("valid date")
        } else {
            NaiveDate::from_ymd_opt(year, month - 1, 1).expect("valid date")
        }
    }

    fn next_month(&self) -> NaiveDate {
        let year = self.current_month.year();
        let month = self.current_month.month();
        if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid date")
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1).expect("valid date")
        }
    }

    fn refresh_notes(&mut self, vault: &Vault) {
        let config = vault.daily_config().clone();
        self.notes_by_date.clear();

        if let Ok(files) = vault.list_files() {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let file_clone = file.clone();
                    // Try to extract date from filename (daily note format)
                    if let Some(stem) = file.file_stem().and_then(|s| s.to_str())
                        && let Ok(date) =
                            NaiveDate::parse_from_str(stem, &config.format.replace(".md", ""))
                    {
                        self.notes_by_date.entry(date).or_default().push(file_clone);
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
                                        .push(file.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn show_month_grid(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        let first_day = self.current_month;
        let _days_in_month = self.days_in_month();
        let first_weekday = first_day.weekday().num_days_from_monday() as usize;

        let total_cells = first_weekday + self.days_in_month();
        let weeks = total_cells.div_ceil(7);

        Grid::new("calendar_grid")
            .striped(false)
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                // Weekday headers
                for (i, day) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                    .iter()
                    .enumerate()
                {
                    ui.label(RichText::new(*day).strong().small());
                    if i == 6 {
                        ui.end_row();
                    }
                }

                let mut day = 1;
                for week in 0..weeks {
                    for weekday in 0..7 {
                        let cell_index = week * 7 + weekday;
                        if cell_index < first_weekday || day > self.days_in_month() {
                            ui.label("");
                        } else {
                            let date = NaiveDate::from_ymd_opt(
                                self.current_month.year(),
                                self.current_month.month(),
                                u32::try_from(day).expect("day fits in u32"),
                            )
                            .expect("valid date");
                            let is_today = date == chrono::Local::now().date_naive();
                            let _notes = self.notes_by_date.get(&date).map_or(0, Vec::len);

                            let (rect, cell_response) =
                                ui.allocate_at_least(egui::vec2(80.0, 80.0), egui::Sense::click());

                            let rect = rect.shrink(4.0);

                            // Cell background
                            if is_today {
                                ui.painter().rect_filled(
                                    rect,
                                    4.0,
                                    egui::Color32::from_rgb(99, 102, 241).linear_multiply(0.2),
                                );
                            } else {
                                ui.painter()
                                    .rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
                            }
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                ui.visuals().widgets.noninteractive.bg_stroke,
                            );

                            let content_rect = rect.shrink(4.0);
                            let mut content_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(content_rect));

                            content_ui.vertical(|ui| {
                                // Day number
                                let day_text = if date == chrono::Local::now().date_naive() {
                                    RichText::new(day.to_string())
                                        .strong()
                                        .color(egui::Color32::from_rgb(99, 102, 241))
                                } else {
                                    RichText::new(day.to_string()).strong()
                                };
                                ui.label(day_text);

                                // Note count
                                let notes_today = self
                                    .notes_by_date
                                    .get(
                                        &chrono::NaiveDate::from_ymd_opt(
                                            self.current_month.year(),
                                            self.current_month.month(),
                                            u32::try_from(day).expect("day fits in u32"),
                                        )
                                        .expect("valid date"),
                                    )
                                    .map_or(0, Vec::len);
                                if notes_today > 0 {
                                    ui.label(RichText::new(format!("📝 {notes_today}")).small());
                                }
                            });

                            if cell_response.clicked() {
                                let date = NaiveDate::from_ymd_opt(
                                    self.current_month.year(),
                                    self.current_month.month(),
                                    u32::try_from(day).expect("day fits in u32"),
                                )
                                .expect("valid date");
                                let path = daily_note_path(&vault.root, vault.daily_config(), date);
                                event_bus.publish(Event::NodeOpened(path));
                            }
                        }
                        day += 1;
                    }
                    if day > self.days_in_month() {
                        ui.end_row();
                        break;
                    }
                    ui.end_row();
                }
            });
    }

    fn days_in_month(&self) -> usize {
        let year = self.current_month.year();
        let month = self.current_month.month();
        let next_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid date")
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1).expect("valid date")
        };
        let first_day = NaiveDate::from_ymd_opt(year, month, 1).expect("valid date");
        usize::try_from((next_month - first_day).num_days()).expect("days in month fits in usize")
    }
}
