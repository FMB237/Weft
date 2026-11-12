//! File system operations for vaults.
//!
//! [`FileOps`] abstracts CRUD operations over a vault's storage so that
//! different backends can be swapped in without changing callers.

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;

/// Write `contents` to `path` without ever leaving a truncated file behind.
///
/// The data goes to a sibling temporary file first, is flushed with `fsync`,
/// and only then renamed over the target. A crash before the rename leaves the
/// previous contents intact; a crash after it leaves the complete new file.
/// The parent directory is synced on a best-effort basis, because some
/// platforms refuse to open a directory as a file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create directory {}", parent.display()))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));

    {
        let mut file = std::fs::File::create(&temp)
            .with_context(|| format!("create temp file {}", temp.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("write temp file {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temp file {}", temp.display()))?;
    }

    std::fs::rename(&temp, path).inspect_err(|_| {
        // Leaving a stray temp file behind is untidy but harmless; failing to
        // remove it must not mask the rename error.
        let _ = std::fs::remove_file(&temp);
    })?;

    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

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
        self.read(path)
            .with_context(|| format!("read {}", path.display()))
    }

    fn write(&self, path: &Path, content: &str) -> Result<()> {
        self.write(path, content)
            .with_context(|| format!("write {}", path.display()))
    }

    fn delete(&self, path: &Path) -> Result<()> {
        self.delete(path)
            .with_context(|| format!("delete {}", path.display()))
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
        assert!(
            vault.read(path).is_err(),
            "deleted file should fail to read"
        );
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

    #[test]
    fn atomic_write_creates_and_replaces_files() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join("nested").join("note.md");

        write_atomic(&path, "# first").expect("first write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "# first");

        write_atomic(&path, "# second").expect("replace write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "# second");
    }

    #[test]
    fn atomic_write_leaves_no_temporary_files() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join("note.md");
        write_atomic(&path, "body").expect("write");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("list dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
            })
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
    }
}
