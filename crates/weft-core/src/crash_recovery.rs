//! Crash recovery for unsaved buffers.
//!
//! When the editor crashes or is killed, any dirty (unsaved) buffers are lost.
//! This module provides a mechanism to periodically write dirty buffers to a
//! crash recovery directory, and restore them on next launch.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Manages crash recovery files for a vault.
#[derive(Debug)]
pub struct CrashRecovery {
    /// Root directory for crash recovery files.
    recovery_dir: PathBuf,
}

impl CrashRecovery {
    /// Create a new crash recovery manager for the given vault root.
    pub fn new(vault_root: &Path) -> Result<Self> {
        let recovery_dir = vault_root.join(".weft").join("crash_recovery");
        std::fs::create_dir_all(&recovery_dir)
            .with_context(|| format!("create crash recovery dir {}", recovery_dir.display()))?;
        Ok(Self { recovery_dir })
    }

    /// Get the crash recovery file path for a note.
    fn recovery_path(&self, note_path: &Path) -> PathBuf {
        // Use a hash of the note path to create a safe filename
        let hash = blake3::hash(note_path.as_os_str().as_encoded_bytes());
        self.recovery_dir.join(format!("{}.toml", hash.to_hex()))
    }

    /// Save a dirty buffer to the crash recovery file.
    ///
    /// Returns true if the file was written (i.e., the buffer was dirty).
    pub fn save(&self, note_path: &Path, content: &str) -> Result<bool> {
        let path = self.recovery_path(note_path);
        let data = CrashRecoveryData {
            note_path: note_path.to_path_buf(),
            content: content.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let toml = toml::to_string_pretty(&data).context("serialize crash recovery data")?;
        crate::file_ops::write_atomic(&path, &toml)
            .with_context(|| format!("write crash recovery {}", path.display()))?;
        Ok(true)
    }

    /// Load a crash recovery file for a note, if it exists.
    pub fn load(&self, note_path: &Path) -> Result<Option<CrashRecoveryData>> {
        let path = self.recovery_path(note_path);
        if !path.exists() {
            return Ok(None);
        }
        let toml = std::fs::read_to_string(&path)
            .with_context(|| format!("read crash recovery {}", path.display()))?;
        let data: CrashRecoveryData = toml::from_str(&toml)
            .with_context(|| format!("parse crash recovery {}", path.display()))?;
        Ok(Some(data))
    }

    /// Delete the crash recovery file for a note (call when the note is saved).
    pub fn clear(&self, note_path: &Path) -> Result<()> {
        let path = self.recovery_path(note_path);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("remove crash recovery {}", path.display()))?;
        }
        Ok(())
    }

    /// Scan all crash recovery files and return a map of note path -> data.
    pub fn scan_all(&self) -> Result<HashMap<PathBuf, CrashRecoveryData>> {
        let mut map = HashMap::new();
        for entry in std::fs::read_dir(&self.recovery_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let toml = std::fs::read_to_string(&path)?;
                if let Ok(data) = toml::from_str::<CrashRecoveryData>(&toml) {
                    map.insert(data.note_path.clone(), data);
                }
            }
        }
        Ok(map)
    }

    /// Clear all crash recovery files.
    pub fn clear_all(&self) -> Result<()> {
        for entry in std::fs::read_dir(&self.recovery_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

/// Data stored in a crash recovery file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrashRecoveryData {
    /// The note path (vault-relative).
    pub note_path: PathBuf,
    /// The unsaved content.
    pub content: String,
    /// Unix timestamp when this was written.
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn crash_recovery_save_and_load() {
        let dir = tempdir().unwrap();
        let recovery = CrashRecovery::new(dir.path()).unwrap();

        let note_path = PathBuf::from("notes/test.md");
        let content = "# Test\n\nUnsaved content.";

        // Save
        recovery.save(&note_path, content).unwrap();

        // Load
        let loaded = recovery.load(&note_path).unwrap();
        assert!(loaded.is_some());
        let data = loaded.unwrap();
        assert_eq!(data.note_path, note_path);
        assert_eq!(data.content, content);

        // Clear
        recovery.clear(&note_path).unwrap();
        let loaded = recovery.load(&note_path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn crash_recovery_scan_all() {
        let dir = tempdir().unwrap();
        let recovery = CrashRecovery::new(dir.path()).unwrap();

        let note1 = PathBuf::from("notes/one.md");
        let note2 = PathBuf::from("notes/two.md");
        recovery.save(&note1, "content 1").unwrap();
        recovery.save(&note2, "content 2").unwrap();

        let all = recovery.scan_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key(&note1));
        assert!(all.contains_key(&note2));
    }
}
