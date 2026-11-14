//! File-recovery snapshots.
//!
//! Every save can leave a copy of the note under
//! `<vault>/.weft/history/<note path>/<timestamp>.md`. That turns an
//! accidental overwrite — by Weft, by another editor, or by a sync client —
//! into something the user can look at and restore.
//!
//! Snapshots are plain Markdown, written atomically, bounded by a retention
//! count per note, and skipped when the content has not changed since the
//! newest snapshot, so a timed autosave does not churn the history folder.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::file_ops::write_atomic_quick;
use crate::workspace::{WORKSPACE_DIR, is_safe_tab_path};

/// Directory for snapshots, inside [`WORKSPACE_DIR`].
pub const HISTORY_DIR: &str = "history";
/// Snapshots kept per note unless the caller asks for another count.
pub const DEFAULT_RETENTION: usize = 20;

/// One stored snapshot of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Vault-relative path of the note.
    pub note: PathBuf,
    /// Milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    /// Absolute path of the snapshot file.
    pub path: PathBuf,
}

impl Snapshot {
    /// Human-readable local timestamp for the recovery dialog.
    pub fn label(&self) -> String {
        use chrono::TimeZone;
        chrono::Local
            .timestamp_millis_opt(self.timestamp_ms)
            .single()
            .map_or_else(
                || format!("{} ms", self.timestamp_ms),
                |time| time.format("%Y-%m-%d %H:%M:%S").to_string(),
            )
    }
}

/// Snapshot storage for one vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStore {
    root: PathBuf,
}

impl HistoryStore {
    /// A store rooted at a vault.
    pub fn new(vault_root: &Path) -> Self {
        Self {
            root: vault_root.to_path_buf(),
        }
    }

    /// Directory holding snapshots for one note.
    pub fn note_dir(&self, note: &Path) -> PathBuf {
        self.root.join(WORKSPACE_DIR).join(HISTORY_DIR).join(note)
    }

    /// Record `content` as a snapshot of `note`.
    ///
    /// Returns the snapshot path, or `None` when the newest snapshot already
    /// has identical content.
    pub fn record(&self, note: &Path, content: &str) -> Result<Option<PathBuf>> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        self.record_at(note, content, stamp)
    }

    /// Record a snapshot with an explicit timestamp, for deterministic tests.
    pub fn record_at(&self, note: &Path, content: &str, stamp: u128) -> Result<Option<PathBuf>> {
        if !is_safe_tab_path(note) {
            bail!(
                "refusing to snapshot a path outside the vault: {}",
                note.display()
            );
        }
        let dir = self.note_dir(note);
        if Self::newest_content(&dir)?.as_deref() == Some(content) {
            return Ok(None);
        }
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("create history directory {}", dir.display()))?;
        let path = unique_snapshot_path(&dir, stamp);
        write_atomic_quick(&path, content)
            .with_context(|| format!("write snapshot {}", path.display()))?;
        Ok(Some(path))
    }

    /// Snapshots of one note, newest first.
    pub fn snapshots(&self, note: &Path) -> Result<Vec<Snapshot>> {
        Self::collect(&self.note_dir(note), note)
    }

    /// Every snapshot in the vault, newest first.
    pub fn all_snapshots(&self) -> Result<Vec<Snapshot>> {
        let base = self.root.join(WORKSPACE_DIR).join(HISTORY_DIR);
        let mut snapshots = Vec::new();
        collect_snapshots(&base, &base, &mut snapshots)?;
        sort_newest_first(&mut snapshots);
        Ok(snapshots)
    }

    /// Read a snapshot's contents.
    pub fn read(&self, snapshot: &Snapshot) -> Result<String> {
        std::fs::read_to_string(&snapshot.path)
            .with_context(|| format!("read snapshot {}", snapshot.path.display()))
    }

    /// Keep the newest `keep` snapshots of `note`, deleting the rest.
    ///
    /// Returns how many files were removed.
    pub fn prune(&self, note: &Path, keep: usize) -> Result<usize> {
        let snapshots = self.snapshots(note)?;
        let mut removed = 0;
        for snapshot in snapshots.into_iter().skip(keep) {
            if std::fs::remove_file(&snapshot.path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Record a snapshot and prune to `keep`, reporting how many were dropped.
    pub fn record_with_retention(
        &self,
        note: &Path,
        content: &str,
        keep: usize,
    ) -> Result<(Option<PathBuf>, usize)> {
        let recorded = self.record(note, content)?;
        let removed = if recorded.is_some() {
            self.prune(note, keep)?
        } else {
            0
        };
        Ok((recorded, removed))
    }

    fn collect(dir: &Path, note: &Path) -> Result<Vec<Snapshot>> {
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut snapshots = Vec::new();
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("list snapshots in {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(timestamp_ms) = snapshot_timestamp(&path) else {
                continue;
            };
            snapshots.push(Snapshot {
                note: note.to_path_buf(),
                timestamp_ms,
                path,
            });
        }
        sort_newest_first(&mut snapshots);
        Ok(snapshots)
    }

    fn newest_content(dir: &Path) -> Result<Option<String>> {
        let Ok(newest) = Self::collect(dir, Path::new("")) else {
            return Ok(None);
        };
        match newest.first() {
            Some(snapshot) => Ok(Some(std::fs::read_to_string(&snapshot.path)?)),
            None => Ok(None),
        }
    }
}

fn sort_newest_first(snapshots: &mut [Snapshot]) {
    snapshots.sort_by(|left, right| {
        right
            .timestamp_ms
            .cmp(&left.timestamp_ms)
            .then_with(|| right.path.cmp(&left.path))
    });
}

fn collect_snapshots(base: &Path, dir: &Path, snapshots: &mut Vec<Snapshot>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_snapshots(base, &path, snapshots)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md")
            && let Some(timestamp_ms) = snapshot_timestamp(&path)
            && let Ok(note) = path.strip_prefix(base)
            && let Some(note_dir) = note.parent()
        {
            snapshots.push(Snapshot {
                note: note_dir.to_path_buf(),
                timestamp_ms,
                path,
            });
        }
    }
    Ok(())
}

/// Parse the millisecond timestamp from `<timestamp>[-n].md`.
fn snapshot_timestamp(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    let digits = stem.split('-').next()?;
    digits.parse::<i64>().ok()
}

fn unique_snapshot_path(dir: &Path, stamp: u128) -> PathBuf {
    let base = dir.join(format!("{stamp}.md"));
    if !base.exists() {
        return base;
    }
    for attempt in 1..1000 {
        let candidate = dir.join(format!("{stamp}-{attempt}.md"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stamp}-{}.md", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn recording_writes_a_snapshot_under_the_note_path() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());

        let path = store
            .record_at(Path::new("notes/idea.md"), "# idea", 1_700_000_000_000)
            .expect("record")
            .expect("first snapshot");

        assert!(path.starts_with(dir.path().join(WORKSPACE_DIR).join(HISTORY_DIR)));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "# idea");
        let snapshots = store.snapshots(Path::new("notes/idea.md")).expect("list");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].note, PathBuf::from("notes/idea.md"));
        assert_eq!(snapshots[0].timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn identical_content_is_not_snapshotted_twice() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        let note = Path::new("note.md");

        assert!(
            store
                .record_at(note, "same", 100)
                .expect("record")
                .is_some()
        );
        assert!(
            store
                .record_at(note, "same", 200)
                .expect("record")
                .is_none()
        );
        assert_eq!(store.snapshots(note).expect("list").len(), 1);
    }

    #[test]
    fn changed_content_creates_a_new_snapshot() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        let note = Path::new("note.md");

        store.record_at(note, "one", 100).expect("record");
        store.record_at(note, "two", 200).expect("record");

        let snapshots = store.snapshots(note).expect("list");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].timestamp_ms, 200, "newest first");
    }

    #[test]
    fn retention_keeps_the_newest_snapshots() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        let note = Path::new("note.md");

        for (index, content) in ["a", "b", "c", "d"].iter().enumerate() {
            store
                .record_at(note, content, 100 + index as u128)
                .expect("record");
        }
        assert_eq!(store.prune(note, 2).expect("prune"), 2);

        let snapshots = store.snapshots(note).expect("list");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(store.read(&snapshots[1]).expect("read"), "c");
        assert_eq!(store.read(&snapshots[0]).expect("read"), "d");
    }

    #[test]
    fn record_with_retention_reports_pruned_files() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        let note = Path::new("note.md");

        store.record_at(note, "a", 100).expect("record");
        store.record_at(note, "b", 200).expect("record");
        let (recorded, removed) = store
            .record_with_retention(note, "c", 2)
            .expect("record with retention");
        assert!(recorded.is_some());
        assert_eq!(removed, 1);
        assert_eq!(store.snapshots(note).expect("list").len(), 2);
    }

    #[test]
    fn all_snapshots_groups_notes_newest_first() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        store
            .record_at(Path::new("a.md"), "a1", 100)
            .expect("record");
        store
            .record_at(Path::new("sub/b.md"), "b1", 300)
            .expect("record");
        store
            .record_at(Path::new("a.md"), "a2", 200)
            .expect("record");

        let all = store.all_snapshots().expect("list all");
        let order: Vec<_> = all
            .iter()
            .map(|s| (s.note.clone(), s.timestamp_ms))
            .collect();
        assert_eq!(
            order,
            vec![
                (PathBuf::from("sub/b.md"), 300),
                (PathBuf::from("a.md"), 200),
                (PathBuf::from("a.md"), 100),
            ]
        );
    }

    #[test]
    fn snapshots_refuse_paths_outside_the_vault() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        assert!(store.record_at(Path::new("../escape.md"), "x", 1).is_err());
        assert!(
            store
                .record_at(Path::new(".weft/workspace.toml"), "x", 1)
                .is_err()
        );
    }

    #[test]
    fn a_missing_history_directory_lists_as_empty() {
        let dir = vault();
        let store = HistoryStore::new(dir.path());
        assert!(
            store
                .snapshots(Path::new("note.md"))
                .expect("list")
                .is_empty()
        );
        assert!(store.all_snapshots().expect("list all").is_empty());
    }
}
