//! Tasks panel for viewing and managing tasks across the vault.
//!
//! Shows tasks grouped into Today, Upcoming, Inbox, and Done buckets.
//! Supports toggling completion which writes back to the source note.

use crate::event_bus::{Event, EventBus};
use crate::widgets::chrome::section_header;
use egui::{Align, CollapsingHeader, Layout, RichText, ScrollArea, Ui};
use std::collections::HashMap;
use weft_core::tasks::{Task, TaskBucket, bucket_tasks, extract_tasks, toggle_task_completion};
use weft_core::vault::Vault;

/// Tasks panel state.
#[derive(Debug, Default)]
pub struct TasksPanel {
    /// Whether each bucket is expanded (keyed by bucket label).
    expanded: HashMap<String, bool>,
    /// Cached tasks for the current view.
    cached_tasks: Vec<Task>,
    /// Whether we need to refresh the task list.
    needs_refresh: bool,
}

impl TasksPanel {
    pub fn new() -> Self {
        let mut expanded = HashMap::new();
        expanded.insert("Today".to_string(), true);
        expanded.insert("Upcoming".to_string(), true);
        expanded.insert("Inbox".to_string(), true);
        expanded.insert("Done".to_string(), true);
        Self {
            expanded,
            cached_tasks: Vec::new(),
            needs_refresh: true,
        }
    }

    /// Show the tasks panel.
    pub fn show(&mut self, ui: &mut Ui, vault: &Vault, event_bus: &mut EventBus) {
        // Refresh tasks if needed
        if self.needs_refresh {
            self.refresh_tasks(vault);
        }

        let today = chrono::Local::now().date_naive();
        let buckets = bucket_tasks(self.cached_tasks.clone(), today);

        ui.vertical(|ui| {
            // Header with refresh button
            ui.horizontal(|ui| {
                section_header(ui, "Tasks");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("🔄 Refresh").clicked() {
                        self.needs_refresh = true;
                    }
                });
            });
            ui.add_space(8.0);

            // Summary counts
            let total_today = buckets
                .iter()
                .find(|(b, _)| *b == weft_core::tasks::TaskBucket::Today)
                .map_or(0, |(_, v)| v.len());
            let total_upcoming = buckets
                .iter()
                .find(|(b, _)| *b == weft_core::tasks::TaskBucket::Upcoming)
                .map_or(0, |(_, v)| v.len());
            let total_inbox = buckets
                .iter()
                .find(|(b, _)| *b == weft_core::tasks::TaskBucket::Inbox)
                .map_or(0, |(_, v)| v.len());
            let total_done = buckets
                .iter()
                .find(|(b, _)| *b == weft_core::tasks::TaskBucket::Done)
                .map_or(0, |(_, v)| v.len());

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("🔴 Today: {total_today}"))
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("📅 Upcoming: {total_upcoming}"))
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("📥 Inbox: {total_inbox}"))
                        .strong()
                        .color(ui.visuals().text_color()),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("✅ Done: {total_done}"))
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Buckets
            for (bucket, tasks) in buckets {
                self.show_bucket(ui, vault, event_bus, bucket, tasks);
            }
        });
    }

    fn refresh_tasks(&mut self, vault: &Vault) {
        let mut all_tasks = Vec::new();
        if let Ok(files) = vault.list_files() {
            for file in files {
                if let Ok(content) = vault.read(&file) {
                    let tasks = extract_tasks(&file, &content);
                    all_tasks.extend(tasks);
                }
            }
        }
        self.cached_tasks = all_tasks;
        self.needs_refresh = false;
    }

    fn bucket_label(bucket: TaskBucket) -> String {
        match bucket {
            weft_core::tasks::TaskBucket::Today => "Today",
            weft_core::tasks::TaskBucket::Upcoming => "Upcoming",
            weft_core::tasks::TaskBucket::Inbox => "Inbox",
            weft_core::tasks::TaskBucket::Done => "Done",
        }
        .to_string()
    }

    fn bucket_emoji(bucket: TaskBucket) -> &'static str {
        match bucket {
            weft_core::tasks::TaskBucket::Today => "🔴",
            weft_core::tasks::TaskBucket::Upcoming => "📅",
            weft_core::tasks::TaskBucket::Inbox => "📥",
            weft_core::tasks::TaskBucket::Done => "✅",
        }
    }

    fn show_bucket(
        &mut self,
        ui: &mut Ui,
        vault: &Vault,
        event_bus: &mut EventBus,
        bucket: TaskBucket,
        tasks: Vec<weft_core::tasks::Task>,
    ) {
        let bucket_key = Self::bucket_label(bucket);
        let expanded = self.expanded.get(&bucket_key).copied().unwrap_or(true);
        let bucket_label = format!(
            "{} {} ({})",
            Self::bucket_emoji(bucket),
            Self::bucket_label(bucket),
            tasks.len()
        );

        CollapsingHeader::new(RichText::new(bucket_label).strong())
            .id_salt(format!("tasks_bucket_{bucket_key}"))
            .default_open(expanded)
            .show(ui, |ui| {
                self.expanded.insert(bucket_key, ui.is_visible());
                if tasks.is_empty() {
                    ui.weak("No tasks in this bucket");
                    return;
                }

                ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for task in tasks {
                        self.show_task(ui, vault, event_bus, &task);
                    }
                });
            });
    }

    fn show_task(
        &mut self,
        ui: &mut Ui,
        _vault: &Vault,
        event_bus: &mut EventBus,
        task: &weft_core::tasks::Task,
    ) {
        let is_done = task.completed;
        let mut new_completed = is_done;

        ui.horizontal(|ui| {
            // Checkbox
            if ui.checkbox(&mut new_completed, "").changed() {
                if let Err(e) = toggle_task_completion(&task.note_path, task.line) {
                    eprintln!("Failed to toggle task: {e}");
                } else {
                    // Refresh to update buckets
                    self.needs_refresh = true;
                    event_bus.publish(Event::DocumentSaved(task.note_path.clone()));
                }
            }

            // Task text with priority indicator
            let priority_emoji = task.priority.map_or("", |p| p.emoji());
            let text = if is_done {
                RichText::new(&task.text)
                    .strikethrough()
                    .color(ui.visuals().weak_text_color())
            } else {
                RichText::new(&task.text).strong()
            };
            ui.label(text);

            if !priority_emoji.is_empty() {
                ui.label(RichText::new(priority_emoji).small());
            }

            // Due date
            if let Some(due) = task.due {
                let today = chrono::Local::now().date_naive();
                let due_color = if due <= today {
                    ui.visuals().warn_fg_color
                } else {
                    ui.visuals().weak_text_color()
                };
                let due_text = RichText::new(format!("📅 {}", due.format("%Y-%m-%d")))
                    .color(due_color)
                    .small();
                ui.label(due_text);
            }

            // Tags
            for tag in &task.tags {
                ui.label(
                    RichText::new(format!("#{tag}"))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }

            // Source note link
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("📄").on_hover_text("Open note").clicked() {
                    event_bus.publish(crate::event_bus::Event::NodeOpened(task.note_path.clone()));
                }
            });
        });

        ui.add_space(4.0);
    }
}
