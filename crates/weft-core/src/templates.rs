//! Template management for notes.
//!
//! Templates live in a configurable folder (default `.weft/templates/`) and
//! support placeholders: `{{date}}`, `{{time}}`, `{{title}}`, `{{cursor}}`.

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for templates, loaded from `vault.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplatesConfig {
    /// Folder (relative to vault root) where templates live.
    #[serde(default)]
    pub folder: String,
}

impl Default for TemplatesConfig {
    fn default() -> Self {
        Self {
            folder: ".weft/templates".to_string(),
        }
    }
}

impl TemplatesConfig {
    /// Load templates config from vault.toml, or return defaults.
    pub fn load(vault_root: &Path) -> Self {
        let vault_toml = vault_root.join("vault.toml");
        if !vault_toml.exists() {
            return Self::default();
        }
        let Ok(content) = std::fs::read_to_string(&vault_toml) else {
            return Self::default();
        };
        let value: toml::Value = match content.parse() {
            Ok(v) => v,
            Err(_) => return Self::default(),
        };
        value
            .get("templates")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default()
    }
}

/// List all template files in the templates folder.
pub fn list_templates(vault_root: &Path, config: &TemplatesConfig) -> Result<Vec<PathBuf>> {
    let folder = vault_root.join(&config.folder);
    if !folder.exists() {
        return Ok(Vec::new());
    }
    let mut templates = Vec::new();
    for entry in std::fs::read_dir(&folder)
        .with_context(|| format!("read template dir {}", folder.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            templates.push(path);
        }
    }
    templates.sort();
    Ok(templates)
}

/// Get template names (without extension) for display.
pub fn template_names(vault_root: &Path, config: &TemplatesConfig) -> Result<Vec<String>> {
    let templates = list_templates(vault_root, config)?;
    Ok(templates
        .into_iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str().map(String::from)))
        .collect())
}

/// Read a template by name (without extension).
pub fn read_template(
    vault_root: &Path,
    config: &TemplatesConfig,
    name: &str,
) -> Result<Option<String>> {
    let folder = vault_root.join(&config.folder);
    let path = folder.join(format!("{name}.md"));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read template {}", path.display()))?;
    Ok(Some(content))
}

/// Create a new note from a template.
///
/// Returns the rendered content with placeholders replaced.
pub fn create_from_template(template_content: &str, title: &str) -> String {
    let now = Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let time_str = now.format("%H:%M").to_string();

    template_content
        .replace("{{date}}", &date_str)
        .replace("{{time}}", &time_str)
        .replace("{{title}}", title)
        .replace("{{cursor}}", "")
}

/// Create a template file.
pub fn create_template(
    vault_root: &Path,
    config: &TemplatesConfig,
    name: &str,
    content: &str,
) -> Result<PathBuf> {
    let folder = vault_root.join(&config.folder);
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("create template folder {}", folder.display()))?;
    let path = folder.join(format!("{name}.md"));
    std::fs::write(&path, content).with_context(|| format!("write template {}", path.display()))?;
    Ok(path)
}

/// Placeholders supported in templates.
pub const PLACEHOLDERS: &[&str] = &["{{date}}", "{{time}}", "{{title}}", "{{cursor}}"];

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn templates_config_defaults() {
        let config = TemplatesConfig::default();
        assert_eq!(config.folder, ".weft/templates");
    }

    #[test]
    fn create_from_template_replaces_placeholders() {
        let template = "# {{title}}\n\nCreated: {{date}} {{time}}\n\n{{cursor}}";
        let result = create_from_template(template, "My Note");
        assert!(result.contains("My Note"));
        assert!(result.contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));
        assert!(!result.contains("{{date}}"));
        assert!(!result.contains("{{time}}"));
        assert!(!result.contains("{{title}}"));
        assert!(!result.contains("{{cursor}}"));
    }

    #[test]
    fn template_placeholders_are_known() {
        assert!(PLACEHOLDERS.contains(&"{{date}}"));
        assert!(PLACEHOLDERS.contains(&"{{time}}"));
        assert!(PLACEHOLDERS.contains(&"{{title}}"));
        assert!(PLACEHOLDERS.contains(&"{{cursor}}"));
    }

    #[test]
    fn create_and_read_template() {
        let dir = tempdir().unwrap();
        let config = TemplatesConfig::default();

        let path =
            create_template(dir.path(), &config, "meeting", "# Meeting\n\n{{date}}").unwrap();
        assert!(path.exists());

        let content = read_template(dir.path(), &config, "meeting")
            .unwrap()
            .unwrap();
        assert_eq!(content, "# Meeting\n\n{{date}}");
    }

    #[test]
    fn list_templates_returns_sorted() {
        let dir = tempdir().unwrap();
        let config = TemplatesConfig::default();

        create_template(dir.path(), &config, "aaa", "").unwrap();
        create_template(dir.path(), &config, "bbb", "").unwrap();
        create_template(dir.path(), &config, "ccc", "").unwrap();

        let names = template_names(dir.path(), &config).unwrap();
        assert_eq!(names, vec!["aaa", "bbb", "ccc"]);
    }
}
