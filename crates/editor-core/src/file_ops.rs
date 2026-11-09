//! File system operations for vaults.
//!
//! [`FileOps`] abstracts CRUD operations over a vault's storage so that
//! different backends can be swapped in without changing callers.

use std::path::Path;

use anyhow::Context;
use anyhow::Result;

/// File system operations (Create, Read, Update, Delete) for a vault.
pub trait FileOps {
    /// List all tracked files in the vault.
    fn list_files(&self) -> Result<Vec<std::path::PathBuf>>;

    /// Read a file's contents relative to the vault root.
    fn read(&self, path: &Path) -> Result<String>;

    /// Write `content` to a file relative to the vault root.
    fn write(&self, path: &Path, content: &str) -> Result<()>;

    /// Delete a file relative to the vault root.
    fn delete(&self, path: &Path) -> Result<()>;

    /// Resolve a relative path to an absolute path within the vault.
    fn resolve(&self, path: &Path) -> std::path::PathBuf;
}

impl FileOps for crate::vault::Vault {
    fn list_files(&self) -> Result<Vec<std::path::PathBuf>> {
        self.list_files()
    }

    fn read(&self, path: &Path) -> Result<String> {
        self.read(path).with_context(|| format!("read {:?}", path))
    }

    fn write(&self, path: &Path, content: &str) -> Result<()> {
        self.write(path, content)
            .with_context(|| format!("write {:?}", path))
    }

    fn delete(&self, path: &Path) -> Result<()> {
        self.delete(path).with_context(|| format!("delete {:?}", path))
    }

    fn resolve(&self, path: &Path) -> std::path::PathBuf {
        self.resolve(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn trait_read_write_delete_roundtrip() {
        let dir = tempdir().expect("create temp dir");
        let vault = crate::vault::Vault::open(dir.path()).expect("open vault");

        let path = Path::new("note.md");
        vault.write(path, "# hello").expect("write file");
        let contents = vault.read(path).expect("read file");
        assert_eq!(contents, "# hello");
        assert_eq!(vault.resolve(path), dir.path().join("note.md"));

        vault.delete(path).expect("delete file");
        assert!(vault.read(path).is_err(), "deleted file should fail to read");
    }

    #[test]
    fn trait_list_files_filters_extensions() {
        let dir = tempdir().expect("create temp dir");
        let vault = crate::vault::Vault::open(dir.path()).expect("open vault");

        std::fs::File::create(dir.path().join("a.md")).expect("create a.md");
        std::fs::File::create(dir.path().join("b.txt")).expect("create b.txt");

        let files = vault.list_files().expect("list files");
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"a.md".to_string()));
        assert!(!names.contains(&"b.txt".to_string()));
    }
}
