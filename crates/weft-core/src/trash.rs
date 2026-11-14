//! Moving deleted notes out of the vault instead of unlinking them.
//!
//! Delete is the one action a notes app cannot take back, so [`trash_entry`]
//! prefers the platform trash (freedesktop on Linux, Finder on macOS, the
//! Recycle Bin on Windows) and falls back to `<vault>/.weft/trash/` when no
//! trash is available — headless sessions, containers, and unusual mounts.
//!
//! The fallback keeps the vault-relative structure under a unique stamp
//! directory, so a restored entry lands back at its original path.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::file_ops::write_atomic;
use crate::workspace::{WORKSPACE_DIR, is_safe_tab_path};

/// Directory for the in-vault fallback, inside [`WORKSPACE_DIR`].
pub const TRASH_DIR: &str = "trash";

/// Where a deleted entry ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashLocation {
    /// The platform trash.
    System,
    /// The in-vault fallback, at this absolute path.
    Vault(PathBuf),
}

impl TrashLocation {
    /// Human-readable description for notices and CLI output.
    pub fn describe(&self) -> String {
        match self {
            Self::System => "the system trash".to_string(),
            Self::Vault(path) => path.display().to_string(),
        }
    }
}

/// Move a vault-relative file or directory to the trash.
///
/// Returns where it went. The entry must stay inside the vault; absolute paths
/// and parent traversals are refused before anything is moved.
pub fn trash_entry(vault_root: &Path, relative: &Path) -> Result<TrashLocation> {
    if !is_safe_tab_path(relative) {
        bail!(
            "refusing to trash a path outside the vault: {}",
            relative.display()
        );
    }
    let full = vault_root.join(relative);
    if !full.exists() {
        bail!("nothing to trash at {}", full.display());
    }

    if trash::delete(&full).is_ok() {
        return Ok(TrashLocation::System);
    }

    let target = move_to_fallback_trash(vault_root, relative)?;
    Ok(TrashLocation::Vault(target))
}

/// Move an entry into `<vault>/.weft/trash/<stamp>/<relative>`.
///
/// Returns the destination. This is the code path used when the platform
/// trash is unavailable, and it is also what tests exercise directly.
pub fn move_to_fallback_trash(vault_root: &Path, relative: &Path) -> Result<PathBuf> {
    if !is_safe_tab_path(relative) {
        bail!(
            "refusing to trash a path outside the vault: {}",
            relative.display()
        );
    }
    let source = vault_root.join(relative);
    if !source.exists() {
        bail!("nothing to trash at {}", source.display());
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let destination = fallback_destination(vault_root, relative, stamp);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create trash directory {}", parent.display()))?;
    }

    std::fs::rename(&source, &destination).with_context(|| {
        format!(
            "move {} to the in-vault trash at {}",
            source.display(),
            destination.display()
        )
    })?;

    // Leave a manifest behind so a future "recently deleted" browser can offer
    // a restore without guessing which vault path a stamp belongs to.
    let manifest = destination
        .parent()
        .map(|parent| parent.join("origin.txt"))
        .unwrap_or_default();
    if !manifest.as_os_str().is_empty() {
        // The file is already safely in the trash; a missing note here is not
        // worth failing the delete, so the error is intentionally dropped.
        let _ = write_atomic(&manifest, &format!("{}\n", relative.display()));
    }

    Ok(destination)
}

/// First free `<vault>/.weft/trash/<stamp>-<n>/<relative>` path.
pub fn fallback_destination(vault_root: &Path, relative: &Path, stamp: u128) -> PathBuf {
    let trash_root = vault_root.join(WORKSPACE_DIR).join(TRASH_DIR);
    for attempt in 0..1000 {
        let candidate = trash_root.join(format!("{stamp}-{attempt}")).join(relative);
        if !candidate.exists()
            && !candidate
                .parent()
                .is_some_and(|parent| parent.join("origin.txt").exists())
        {
            return candidate;
        }
    }
    // A thousand collisions within one millisecond means something is very
    // wrong; fall back to a pid-qualified path rather than looping forever.
    trash_root
        .join(format!("{stamp}-{}", std::process::id()))
        .join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn fallback_trash_preserves_file_contents_and_location() {
        let dir = vault();
        std::fs::write(dir.path().join("note.md"), "# keep me").expect("write note");

        let target = move_to_fallback_trash(dir.path(), Path::new("note.md")).expect("trash");

        assert!(!dir.path().join("note.md").exists());
        assert!(target.starts_with(dir.path().join(WORKSPACE_DIR).join(TRASH_DIR)));
        assert_eq!(
            std::fs::read_to_string(&target).expect("read trashed note"),
            "# keep me"
        );
        assert_eq!(
            std::fs::read_to_string(target.parent().expect("stamp dir").join("origin.txt"))
                .expect("read origin"),
            "note.md\n"
        );
    }

    #[test]
    fn fallback_trash_moves_whole_directories() {
        let dir = vault();
        std::fs::create_dir_all(dir.path().join("projects/alpha")).expect("mkdir");
        std::fs::write(dir.path().join("projects/alpha/note.md"), "body").expect("write");

        let target =
            move_to_fallback_trash(dir.path(), Path::new("projects")).expect("trash directory");

        assert!(!dir.path().join("projects").exists());
        assert!(target.join("alpha/note.md").is_file());
    }

    #[test]
    fn fallback_destination_avoids_collisions() {
        let dir = vault();
        let first = fallback_destination(dir.path(), Path::new("a.md"), 42);
        std::fs::create_dir_all(first.parent().expect("parent")).expect("mkdir");
        std::fs::write(&first, "one").expect("write first");

        let second = fallback_destination(dir.path(), Path::new("a.md"), 42);
        assert_ne!(first, second, "a second delete gets its own stamp slot");
    }

    #[test]
    fn trash_refuses_paths_outside_the_vault() {
        let dir = vault();
        assert!(trash_entry(dir.path(), Path::new("../escape.md")).is_err());
        assert!(trash_entry(dir.path(), Path::new(".weft/workspace.toml")).is_err());
        assert!(trash_entry(dir.path(), Path::new("/tmp/absolute.md")).is_err());
        assert!(trash_entry(dir.path(), Path::new("missing.md")).is_err());
    }
}
