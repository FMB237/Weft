//! Daily note management.
//!
//! Handles creation, opening, and configuration of daily notes.
//! Configuration comes from `vault.toml` under `[daily]`.

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for daily notes, loaded from `vault.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DailyConfig {
    /// Folder (relative to vault root) where daily notes live.
    #[serde(default = "default_folder")]
    pub folder: String,
    /// Filename format with strftime placeholders.
    /// Default: `%Y-%m-%d.md` (e.g., `2024-01-15.md`).
    #[serde(default = "default_format")]
    pub format: String,
    /// Template file (relative to vault root) to use for new daily notes.
    /// Optional; if set, the template is rendered with date/title placeholders.
    pub template: Option<String>,
    /// Whether to open the daily note automatically on vault open.
    pub open_on_startup: bool,
}

fn default_folder() -> String {
    "daily".to_string()
}
fn default_format() -> String {
    "%Y-%m-%d.md".to_string()
}

impl Default for DailyConfig {
    fn default() -> Self {
        Self {
            folder: default_folder(),
            format: default_format(),
            template: None,
            open_on_startup: false,
        }
    }
}

impl DailyConfig {
    /// Load daily config from vault.toml, or return defaults.
    pub fn load(vault_root: &Path) -> Self {
        let vault_toml = vault_root.join("vault.toml");
        if !vault_toml.exists() {
            return Self::default();
        }
        let Ok(content) = std::fs::read_to_string(&vault_toml) else {
            return Self::default();
        };
        let Ok(value) = content.parse::<toml::Value>() else {
            return Self::default();
        };
        value
            .get("daily")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default()
    }
}

/// Resolve the path for a daily note on a given date.
pub fn daily_note_path(vault_root: &Path, config: &DailyConfig, date: NaiveDate) -> PathBuf {
    let folder = vault_root.join(&config.folder);
    let filename = date.format(&config.format).to_string();
    folder.join(filename)
}

/// Get today's daily note path, creating parent directories if needed.
pub fn todays_daily_note_path(vault_root: &Path, config: &DailyConfig) -> PathBuf {
    let date = Local::now().date_naive();
    let path = daily_note_path(vault_root, config, date);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    path
}

/// Create a new daily note from template (if configured) or empty.
pub fn create_daily_note(
    vault_root: &Path,
    config: &DailyConfig,
    date: NaiveDate,
) -> Result<PathBuf> {
    let path = daily_note_path(vault_root, config, date);
    if path.exists() {
        return Ok(path);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create daily folder {}", parent.display()))?;
    }

    let content = if let Some(template_path) = &config.template {
        let full_template = vault_root.join(template_path);
        if full_template.exists() {
            let template = std::fs::read_to_string(&full_template)
                .with_context(|| format!("read template {}", full_template.display()))?;
            render_daily_template(&template, date)
        } else {
            default_daily_content(date)
        }
    } else {
        default_daily_content(date)
    };

    std::fs::write(&path, content)
        .with_context(|| format!("write daily note {}", path.display()))?;
    Ok(path)
}

/// Render a template for a daily note.
fn render_daily_template(template: &str, date: NaiveDate) -> String {
    let title = date.format("%A, %B %d, %Y").to_string();
    let date_str = date.format("%Y-%m-%d").to_string();
    let time_str = Local::now().format("%H:%M").to_string();

    template
        .replace("{{date}}", &date_str)
        .replace("{{time}}", &time_str)
        .replace("{{title}}", &title)
        .replace("{{cursor}}", "")
}

/// Default content for a daily note when no template is configured.
fn default_daily_content(date: NaiveDate) -> String {
    let title = date.format("%A, %B %d, %Y").to_string();
    format!("# {title}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn daily_config_defaults() {
        let config = DailyConfig::default();
        assert_eq!(config.folder, "daily");
        assert_eq!(config.format, "%Y-%m-%d.md");
        assert!(config.template.is_none());
        assert!(!config.open_on_startup);
    }

    #[test]
    fn daily_note_path_formats_correctly() {
        let vault = PathBuf::from("/tmp/vault");
        let config = DailyConfig {
            folder: "journal".to_string(),
            format: "%Y-%m-%d.md".to_string(),
            template: None,
            open_on_startup: false,
        };
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let path = daily_note_path(&vault, &config, date);
        assert_eq!(path, PathBuf::from("/tmp/vault/journal/2024-01-15.md"));
    }

    #[test]
    fn daily_note_path_custom_format() {
        let vault = PathBuf::from("/tmp/vault");
        let config = DailyConfig {
            folder: "daily".to_string(),
            format: "daily-%Y-%m-%d.md".to_string(),
            template: None,
            open_on_startup: false,
        };
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let path = daily_note_path(&vault, &config, date);
        assert_eq!(path, PathBuf::from("/tmp/vault/daily/daily-2024-01-15.md"));
    }

    #[test]
    fn render_daily_template_replaces_placeholders() {
        let template = "# {{title}}\n\nDate: {{date}}\nTime: {{time}}\n\n{{cursor}}";
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let rendered = render_daily_template(template, date);
        assert!(rendered.contains("Monday, January 15, 2024"));
        assert!(rendered.contains("2024-01-15"));
        assert!(!rendered.contains("{{cursor}}")); // replaced with empty
    }

    #[test]
    fn create_daily_note_creates_file() {
        let dir = tempdir().unwrap();
        let config = DailyConfig::default();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let path = create_daily_note(dir.path(), &config, date).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Monday, January 15, 2024"));
    }

    #[test]
    fn create_daily_note_with_template() {
        let dir = tempdir().unwrap();
        let template_dir = dir.path().join("templates");
        std::fs::create_dir_all(&template_dir).unwrap();
        let template_path = template_dir.join("daily.md");
        std::fs::write(&template_path, "# {{title}}\n\n{{date}}").unwrap();

        let config = DailyConfig {
            folder: "daily".to_string(),
            format: "%Y-%m-%d.md".to_string(),
            template: Some("templates/daily.md".to_string()),
            open_on_startup: false,
        };
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let path = create_daily_note(dir.path(), &config, date).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Monday, January 15, 2024"));
        assert!(content.contains("2024-01-15"));
    }
}
