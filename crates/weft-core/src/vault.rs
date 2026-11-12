use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// A vault is a directory of markdown files with associated config.
#[derive(Debug)]
pub struct Vault {
    pub root: PathBuf,
    pub config: VaultConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VaultConfig {
    pub name: String,
    pub extensions: Vec<String>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            extensions: vec!["md".into(), "markdown".into()],
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

        Ok(Self { root, config })
    }

    /// Save vault config to disk.
    pub fn save_config(&self) -> Result<()> {
        let data = toml::to_string_pretty(&self.config)?;
        let config_path = self.root.join("vault.toml");
        std::fs::write(&config_path, data)?;
        Ok(())
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
    pub fn write(&self, path: &Path, content: &str) -> Result<()> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, content).with_context(|| format!("write {}", full.display()))
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
