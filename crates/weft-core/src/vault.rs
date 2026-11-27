use crate::daily::DailyConfig;
use crate::graph_updater::AliasMap;
use crate::parser;
use crate::properties::PropertySchema;
use crate::templates::TemplatesConfig;
use crate::views::ViewRegistry;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A vault is a directory of markdown files with associated config.
#[derive(Debug)]
pub struct Vault {
    pub root: PathBuf,
    pub config: VaultConfig,
    /// Property schema (loaded from vault.toml and inferred from notes).
    property_schema: PropertySchema,
    /// View registry (loaded from .weft/views.toml).
    view_registry: ViewRegistry,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VaultConfig {
    pub name: String,
    pub extensions: Vec<String>,
    /// Property definitions from vault.toml [properties] section.
    #[serde(default)]
    pub properties: PropertySchema,
    /// Daily note configuration from vault.toml [daily] section.
    #[serde(default)]
    pub daily: DailyConfig,
    /// Template configuration from vault.toml [templates] section.
    #[serde(default)]
    pub templates: TemplatesConfig,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            extensions: vec!["md".into(), "markdown".into()],
            properties: PropertySchema::default(),
            daily: DailyConfig::default(),
            templates: TemplatesConfig::default(),
        }
    }
}

impl Vault {
    /// Open or create a vault at the given root path.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)
            .with_context(|| format!("create vault dir {}", root.display()))?;

        let config_path = root.join("vault.toml");
        let config = if config_path.exists() {
            let data = std::fs::read_to_string(&config_path)?;
            toml::from_str(&data).context("parse vault config")?
        } else {
            VaultConfig::default()
        };

        // Load property schema from config
        let property_schema = config.properties.clone();

        // Load view registry from .weft/views.toml
        let view_registry = Self::load_view_registry(&root)?;

        Ok(Self {
            root,
            config,
            property_schema,
            view_registry,
        })
    }

    /// Save vault config to disk.
    pub fn save_config(&self) -> Result<()> {
        let data = toml::to_string_pretty(&self.config)?;
        let config_path = self.root.join("vault.toml");
        crate::file_ops::write_atomic(&config_path, &data)?;
        Ok(())
    }

    /// Save view registry to disk.
    pub fn save_view_registry(&self) -> Result<()> {
        let weft_dir = self.root.join(".weft");
        std::fs::create_dir_all(&weft_dir)?;
        let views_path = weft_dir.join("views.toml");
        let data = toml::to_string_pretty(&self.view_registry)?;
        crate::file_ops::write_atomic(&views_path, &data)?;
        Ok(())
    }

    fn load_view_registry(root: &Path) -> Result<ViewRegistry> {
        let views_path = root.join(".weft").join("views.toml");
        if views_path.exists() {
            let data = std::fs::read_to_string(&views_path)?;
            Ok(toml::from_str(&data).context("parse views registry")?)
        } else {
            Ok(ViewRegistry::new())
        }
    }

    /// Get the property schema for this vault.
    pub fn property_schema(&self) -> &PropertySchema {
        &self.property_schema
    }

    /// Get the view registry for this vault.
    pub fn view_registry(&self) -> &ViewRegistry {
        &self.view_registry
    }

    /// Get a mutable reference to the view registry.
    pub fn view_registry_mut(&mut self) -> &mut ViewRegistry {
        &mut self.view_registry
    }

    /// Get a mutable reference to the property schema.
    pub fn property_schema_mut(&mut self) -> &mut PropertySchema {
        &mut self.property_schema
    }

    /// Get the daily note configuration.
    pub fn daily_config(&self) -> &DailyConfig {
        &self.config.daily
    }

    /// Get the templates configuration.
    pub fn templates_config(&self) -> &TemplatesConfig {
        &self.config.templates
    }

    /// List all markdown files in the vault recursively, relative to the vault root.
    pub fn list_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        Self::walk_dir(&self.root, &self.root, &self.config.extensions, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn walk_dir(
        root: &Path,
        dir: &Path,
        extensions: &[String],
        files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if Self::is_hidden_dir(root, &path) {
                    continue;
                }
                Self::walk_dir(root, &path, extensions, files)?;
            } else if path.is_file()
                && let Some(ext) = path.extension()
                && extensions.contains(&ext.to_string_lossy().to_string())
            {
                files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }

    /// List all directories in the vault recursively, relative to the vault
    /// root. Hidden bookkeeping directories (`.editor`, `.git`, and any other
    /// dot-directory) are excluded.
    pub fn list_dirs(&self) -> Result<Vec<PathBuf>> {
        let mut dirs = Vec::new();
        Self::walk_dirs(&self.root, &self.root, &mut dirs)?;
        dirs.sort();
        Ok(dirs)
    }

    fn walk_dirs(root: &Path, dir: &Path, dirs: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && !Self::is_hidden_dir(root, &path) {
                dirs.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                Self::walk_dirs(root, &path, dirs)?;
            }
        }
        Ok(())
    }

    fn is_hidden_dir(root: &Path, path: &Path) -> bool {
        let rel = path.strip_prefix(root).unwrap_or(path);
        rel.components()
            .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    }

    /// Create a directory (and any missing parents) relative to the vault root.
    pub fn create_dir(&self, path: &Path) -> Result<()> {
        let full = self.root.join(path);
        std::fs::create_dir_all(&full)
            .with_context(|| format!("create directory {}", full.display()))
    }

    /// Rename a file or directory relative to the vault root.
    pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let from_full = self.root.join(from);
        let to_full = self.root.join(to);
        if let Some(parent) = to_full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from_full, &to_full)
            .with_context(|| format!("rename {} to {}", from_full.display(), to_full.display()))
    }

    /// Build an index of note aliases (from frontmatter `aliases:`) to file paths.
    /// Returns a map from alias (lowercase) to the relative file path.
    pub fn build_alias_index(&self) -> Result<AliasMap> {
        let mut alias_map = HashMap::new();
        let files = self.list_files()?;
        for file in files {
            if let Ok(content) = self.read(&file) {
                let frontmatter = parser::parse_frontmatter(&content);
                for alias in frontmatter.aliases {
                    alias_map.insert(alias.to_lowercase(), file.clone());
                }
            }
        }
        Ok(alias_map)
    }

    /// Recursively delete a directory relative to the vault root.
    pub fn delete_dir(&self, path: &Path) -> Result<()> {
        let full = self.root.join(path);
        std::fs::remove_dir_all(&full)
            .with_context(|| format!("delete directory {}", full.display()))
    }

    /// Read a file's contents relative to the vault root.
    pub fn read(&self, path: &Path) -> Result<String> {
        let full = self.root.join(path);
        std::fs::read_to_string(&full).with_context(|| format!("read {}", full.display()))
    }

    /// Write content to a file relative to the vault root.
    ///
    /// The write is atomic: a crash mid-save leaves the previous contents
    /// intact instead of a truncated note.
    pub fn write(&self, path: &Path, content: &str) -> Result<()> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::file_ops::write_atomic(&full, content)
            .with_context(|| format!("write {}", full.display()))
    }

    /// Move a file or directory to the trash instead of deleting it.
    ///
    /// Uses the platform trash when available and `<vault>/.weft/trash/`
    /// otherwise. Returns where the entry went.
    pub fn trash(&self, path: &Path) -> Result<crate::trash::TrashLocation> {
        crate::trash::trash_entry(&self.root, path)
    }

    /// Delete a file relative to the vault root.
    pub fn delete(&self, path: &Path) -> Result<()> {
        let full = self.root.join(path);
        std::fs::remove_file(&full).with_context(|| format!("delete {}", full.display()))
    }

    /// Resolve a relative path to an absolute path within the vault.
    pub fn resolve(&self, path: &Path) -> PathBuf {
        self.root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn vault_open_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        assert_eq!(vault.list_files().unwrap().len(), 0);

        // Create a markdown file
        let mut f = std::fs::File::create(dir.path().join("note.md")).unwrap();
        f.write_all(b"# hello").unwrap();

        let files = vault.list_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap() == "note.md");
    }

    #[test]
    fn vault_lists_nested_dirs_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.create_dir(Path::new("projects/alpha")).unwrap();
        vault
            .write(Path::new("projects/alpha/note.md"), "# Alpha")
            .unwrap();
        std::fs::create_dir_all(dir.path().join(".editor")).unwrap();

        let dirs = vault.list_dirs().unwrap();
        assert_eq!(
            dirs,
            vec![PathBuf::from("projects"), PathBuf::from("projects/alpha")]
        );
        assert!(dirs.iter().all(|d| d != Path::new(".editor")));
    }

    #[test]
    fn vault_rename_and_delete_dir() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.create_dir(Path::new("old/child")).unwrap();
        vault.write(Path::new("old/child/note.md"), "hi").unwrap();

        vault
            .rename(Path::new("old/child"), Path::new("new/child"))
            .unwrap();
        assert!(dir.path().join("new/child/note.md").exists());
        assert!(!dir.path().join("old/child").exists());

        vault.delete_dir(Path::new("new")).unwrap();
        assert!(!dir.path().join("new").exists());
    }
}
