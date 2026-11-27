//! Task extraction and management from markdown files.
//!
//! Scans notes for `- [ ]` / `- [x]` items and extracts metadata:
//! note path, line number, text, due date, priority, tags.

use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A task extracted from a markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Vault-relative path of the note containing this task.
    pub note_path: PathBuf,
    /// Line number (1-based) where the task appears.
    pub line: usize,
    /// The task text (without the checkbox marker).
    pub text: String,
    /// Whether the task is completed.
    pub completed: bool,
    /// Optional due date parsed from the text (e.g., `@due(2024-01-15)` or `📅 2024-01-15`).
    pub due: Option<NaiveDate>,
    /// Priority parsed from the text (e.g., `@high`, `@medium`, `@low` or `!!!`, `!!`, `!`).
    pub priority: Option<Priority>,
    /// Tags associated with the task (from `#tag` in the text or note's frontmatter).
    pub tags: Vec<String>,
    /// The raw line content for context.
    pub raw_line: String,
}

/// Task priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl Priority {
    /// Parse priority from text markers.
    pub fn from_text(text: &str) -> Option<Self> {
        // Check for @priority tags
        if text.contains("@critical") {
            return Some(Self::Critical);
        }
        if text.contains("@high") {
            return Some(Self::High);
        }
        if text.contains("@medium") {
            return Some(Self::Medium);
        }
        if text.contains("@low") {
            return Some(Self::Low);
        }
        // Check for exclamation marks (!!! > !! > !)
        let excl_count = text.chars().filter(|&c| c == '!').count();
        match excl_count {
            0 => None,
            1 => Some(Self::Low),
            2 => Some(Self::Medium),
            _ => Some(Self::High), // 3+ treated as high/critical
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }

    /// Emoji for UI display.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Low => "▼",
            Self::Medium => "▲",
            Self::High => "🔴",
            Self::Critical => "🔥",
        }
    }
}

/// Extract all tasks from a markdown file.
pub fn extract_tasks(note_path: &Path, content: &str) -> Vec<Task> {
    let mut tasks = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        if let Some(task) = parse_task_line(note_path, line_num + 1, line) {
            tasks.push(task);
        }
    }
    tasks
}

/// Parse a single line for a task checkbox.
fn parse_task_line(note_path: &Path, line_num: usize, line: &str) -> Option<Task> {
    let trimmed = line.trim_start();
    // Match `- [ ]` or `- [x]` or `* [ ]` or `* [x]`
    let checkbox_end = if trimmed.starts_with("- [") || trimmed.starts_with("* [") {
        trimmed.find("] ")?
    } else {
        return None;
    };

    let checkbox = &trimmed[..=checkbox_end];
    let completed = checkbox.contains('x') || checkbox.contains('X');
    let text = trimmed[checkbox_end + 2..].trim().to_string();

    if text.is_empty() {
        return None;
    }

    let due = extract_due_date(&text);
    let priority = Priority::from_text(&text);
    let tags = extract_tags(&text);

    Some(Task {
        note_path: note_path.to_path_buf(),
        line: line_num,
        text,
        completed,
        due,
        priority,
        tags,
        raw_line: line.to_string(),
    })
}

/// Extract due date from task text.
/// Supports: `@due(YYYY-MM-DD)`, `📅 YYYY-MM-DD`, `@due(YYYY-MM-DD HH:MM)`.
fn extract_due_date(text: &str) -> Option<NaiveDate> {
    // @due(YYYY-MM-DD) or @due(YYYY-MM-DD HH:MM)
    if let Some(start) = text.find("@due(") {
        let end = text[start..].find(')')? + start;
        let inside = &text[start + 5..end];
        let date_part = inside.split_whitespace().next()?;
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok();
    }
    // 📅 YYYY-MM-DD
    if let Some(start) = text.find("📅") {
        let after = &text[start + "📅".len()..].trim();
        let date_part = after.split_whitespace().next()?;
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok();
    }
    // 🗓 YYYY-MM-DD
    if let Some(start) = text.find("🗓") {
        let after = &text[start + "🗓".len()..].trim();
        let date_part = after.split_whitespace().next()?;
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok();
    }
    None
}

/// Extract tags from task text (`#tag` format).
fn extract_tags(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for word in text.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#')
            && !tag.is_empty()
        {
            tags.push(tag.to_string());
        }
    }
    tags
}

/// Bucket for grouping tasks in the Tasks view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskBucket {
    /// Overdue or due today.
    Today,
    /// Due in the future.
    Upcoming,
    /// No due date.
    Inbox,
    /// Completed.
    Done,
}

impl TaskBucket {
    /// Determine which bucket a task belongs to.
    pub fn for_task(task: &Task, today: NaiveDate) -> Self {
        if task.completed {
            return Self::Done;
        }
        match task.due {
            Some(due) if due <= today => Self::Today,
            Some(_) => Self::Upcoming,
            None => Self::Inbox,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Upcoming => "Upcoming",
            Self::Inbox => "Inbox",
            Self::Done => "Done",
        }
    }

    /// Emoji for UI display.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Today => "🔴",
            Self::Upcoming => "📅",
            Self::Inbox => "📥",
            Self::Done => "✅",
        }
    }
}

/// Group tasks into buckets.
pub fn bucket_tasks(tasks: Vec<Task>, today: NaiveDate) -> Vec<(TaskBucket, Vec<Task>)> {
    let mut buckets: Vec<(TaskBucket, Vec<Task>)> = vec![
        (TaskBucket::Today, Vec::new()),
        (TaskBucket::Upcoming, Vec::new()),
        (TaskBucket::Inbox, Vec::new()),
        (TaskBucket::Done, Vec::new()),
    ];

    for task in tasks {
        let bucket = TaskBucket::for_task(&task, today);
        if let Some((_, vec)) = buckets.iter_mut().find(|(b, _)| *b == bucket) {
            vec.push(task);
        }
    }

    // Sort within each bucket: Today/Upcoming by due date, then priority desc
    for (bucket, vec) in &mut buckets {
        match bucket {
            TaskBucket::Today | TaskBucket::Upcoming => {
                vec.sort_by(|a, b| {
                    a.due
                        .cmp(&b.due)
                        .then_with(|| b.priority.cmp(&a.priority))
                        .then_with(|| a.text.cmp(&b.text))
                });
            }
            TaskBucket::Inbox => {
                vec.sort_by(|a, b| {
                    b.priority
                        .cmp(&a.priority)
                        .then_with(|| a.text.cmp(&b.text))
                });
            }
            TaskBucket::Done => {
                vec.sort_by(|a, b| b.text.cmp(&a.text));
            }
        }
    }

    buckets
}

/// Toggle task completion in the source file.
pub fn toggle_task_completion(note_path: &Path, line: usize) -> Result<()> {
    let content = std::fs::read_to_string(note_path)?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    if line == 0 || line > lines.len() {
        return Ok(());
    }

    let idx = line - 1;
    let l = &mut lines[idx];

    // Toggle `- [ ]` <-> `- [x]` or `* [ ]` <-> `* [x]`
    if l.contains("[ ]") {
        *l = l.replacen("[ ]", "[x]", 1);
    } else if l.contains("[x]") || l.contains("[X]") {
        *l = l.replacen("[x]", "[ ]", 1).replacen("[X]", "[ ]", 1);
    }

    std::fs::write(note_path, lines.join("\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile;

    #[test]
    fn parse_simple_task() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(&path, "- [ ] Buy milk\n- [x] Done task\n");
        assert_eq!(tasks.len(), 2);
        assert!(!tasks[0].completed);
        assert!(tasks[1].completed);
    }

    #[test]
    fn parse_task_with_due_at_due() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(&path, "- [ ] Task @due(2024-01-15)\n");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].due,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
    }

    #[test]
    fn parse_task_with_due_calendar() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(&path, "- [ ] Task 📅 2024-01-15\n");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].due,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
    }

    #[test]
    fn parse_task_with_priority() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(
            &path,
            "- [ ] High priority !!!\n- [ ] Medium !!\n- [ ] Low !\n",
        );
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].priority, Some(Priority::High));
        assert_eq!(tasks[1].priority, Some(Priority::Medium));
        assert_eq!(tasks[2].priority, Some(Priority::Low));
    }

    #[test]
    fn parse_task_with_at_priority() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(
            &path,
            "- [ ] Critical @critical\n- [ ] High @high\n- [ ] Medium @medium\n- [ ] Low @low\n",
        );
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].priority, Some(Priority::Critical));
        assert_eq!(tasks[1].priority, Some(Priority::High));
        assert_eq!(tasks[2].priority, Some(Priority::Medium));
        assert_eq!(tasks[3].priority, Some(Priority::Low));
    }

    #[test]
    fn parse_task_with_tags() {
        let path = PathBuf::from("note.md");
        let tasks = extract_tasks(&path, "- [ ] Task #work #urgent\n");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].tags, vec!["work", "urgent"]);
    }

    #[test]
    fn bucket_today() {
        let today = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let tasks = vec![
            Task {
                note_path: PathBuf::new(),
                line: 1,
                text: "t".into(),
                completed: false,
                due: Some(today),
                priority: None,
                tags: vec![],
                raw_line: String::new(),
            },
            Task {
                note_path: PathBuf::new(),
                line: 2,
                text: "t".into(),
                completed: false,
                due: Some(today - chrono::Duration::days(1)),
                priority: None,
                tags: vec![],
                raw_line: String::new(),
            },
            Task {
                note_path: PathBuf::new(),
                line: 3,
                text: "t".into(),
                completed: false,
                due: Some(today + chrono::Duration::days(1)),
                priority: None,
                tags: vec![],
                raw_line: String::new(),
            },
            Task {
                note_path: PathBuf::new(),
                line: 4,
                text: "t".into(),
                completed: false,
                due: None,
                priority: None,
                tags: vec![],
                raw_line: String::new(),
            },
            Task {
                note_path: PathBuf::new(),
                line: 5,
                text: "t".into(),
                completed: true,
                due: Some(today),
                priority: None,
                tags: vec![],
                raw_line: String::new(),
            },
        ];
        let buckets = bucket_tasks(tasks, today);
        let today_bucket = buckets
            .iter()
            .find(|(b, _)| *b == TaskBucket::Today)
            .unwrap();
        let upcoming_bucket = buckets
            .iter()
            .find(|(b, _)| *b == TaskBucket::Upcoming)
            .unwrap();
        let inbox_bucket = buckets
            .iter()
            .find(|(b, _)| *b == TaskBucket::Inbox)
            .unwrap();
        let done_bucket = buckets
            .iter()
            .find(|(b, _)| *b == TaskBucket::Done)
            .unwrap();

        assert_eq!(today_bucket.1.len(), 2); // due today + overdue
        assert_eq!(upcoming_bucket.1.len(), 1);
        assert_eq!(inbox_bucket.1.len(), 1);
        assert_eq!(done_bucket.1.len(), 1);
    }

    #[test]
    fn toggle_task_completion_works() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.md");
        std::fs::write(&file, "- [ ] Task 1\n- [x] Task 2\n").unwrap();

        toggle_task_completion(&file, 1).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("- [x] Task 1"));

        toggle_task_completion(&file, 2).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("- [ ] Task 2"));
    }
}
