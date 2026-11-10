//! CLI entry point.
#![expect(
    clippy::print_stdout,
    reason = "CLI output is the user-facing interface"
)]

use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use editor_core::{document::Document, vault::Vault};

#[derive(Parser)]
#[command(name = "editor-cli")]
#[command(about = "Markdown editor CLI", long_about = None)]
struct Cli {
    /// Vault root directory. Note paths are resolved relative to this root.
    ///
    /// When omitted, a note path is treated as relative to its own parent
    /// directory for backward compatibility.
    #[arg(long, global = true, value_name = "ROOT")]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a vault at PATH
    Init {
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Create a new markdown file
    Create {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(short, long)]
        content: Option<String>,
    },
    /// Read a markdown file
    Read {
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Update a markdown file
    Update {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(short, long)]
        content: String,
    },
    /// Delete a markdown file
    Delete {
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// List all markdown files in the vault
    List {
        /// Vault root, if `--vault` was not supplied.
        #[arg(value_name = "PATH", default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let vault_root = cli.vault;

    match cli.command {
        Commands::Init { path } => {
            let root = vault_root.as_deref().unwrap_or(&path);
            let vault = Vault::open(root)?;
            vault.save_config()?;
            println!("Initialized vault at {}", vault.root.display());
        }
        Commands::Create { path, content } => {
            let (vault, relative) = open_target(vault_root.as_deref(), &path)?;
            let content = content.unwrap_or_default();
            vault.write(&relative, &content)?;
            let _document = Document::new(relative.clone(), &content);
            println!("Created {}", relative.display());
        }
        Commands::Read { path } => {
            let (vault, relative) = open_target(vault_root.as_deref(), &path)?;
            let content = vault.read(&relative)?;
            println!("{content}");
        }
        Commands::Update { path, content } => {
            let (vault, relative) = open_target(vault_root.as_deref(), &path)?;
            vault.write(&relative, &content)?;
            println!("Updated {}", relative.display());
        }
        Commands::Delete { path } => {
            let (vault, relative) = open_target(vault_root.as_deref(), &path)?;
            vault.delete(&relative)?;
            println!("Deleted {}", relative.display());
        }
        Commands::List { path } => {
            let root = vault_root.as_deref().unwrap_or(&path);
            let vault = Vault::open(root)?;
            for file in vault.list_files()? {
                println!("{}", file.display());
            }
        }
    }

    Ok(())
}

/// Open the vault that `path` belongs to and return the path relative to it.
fn open_target(vault_root: Option<&Path>, path: &Path) -> Result<(Vault, PathBuf)> {
    if let Some(root) = vault_root {
        let vault = Vault::open(root)?;
        let relative = note_path_in_vault(&vault.root, path)?;
        return Ok((vault, relative));
    }

    // Historical behaviour: the note's parent directory is the vault root.
    // Filtering the empty parent keeps bare filenames from resolving to "".
    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let vault = Vault::open(root)?;
    let relative = path
        .file_name()
        .map_or_else(|| path.to_path_buf(), PathBuf::from);

    Ok((vault, relative))
}

/// Resolve a note path to a vault-relative path.
///
/// Absolute paths are accepted only when they live under `root`; relative
/// paths are used as-is. `..`, root prefixes, and empty paths are rejected so
/// a command can never write outside the selected vault.
fn note_path_in_vault(root: &Path, path: &Path) -> Result<PathBuf> {
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .map_err(|_| {
                anyhow::anyhow!(
                    "{} is outside the vault root {}",
                    path.display(),
                    root.display()
                )
            })?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };

    if relative.as_os_str().is_empty() {
        bail!("note path is empty");
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("note path escapes the vault: {}", path.display());
    }

    Ok(relative)
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests panic on fixture setup failures")]

    use super::*;

    #[test]
    fn relative_paths_stay_relative_to_vault_root() {
        let root = Path::new("/vault");
        assert_eq!(
            note_path_in_vault(root, Path::new("notes/a.md")).unwrap(),
            PathBuf::from("notes/a.md")
        );
    }

    #[test]
    fn absolute_paths_inside_vault_are_stripped() {
        let root = Path::new("/vault");
        assert_eq!(
            note_path_in_vault(root, Path::new("/vault/notes/a.md")).unwrap(),
            PathBuf::from("notes/a.md")
        );
    }

    #[test]
    fn paths_outside_vault_are_rejected() {
        let root = Path::new("/vault");
        assert!(note_path_in_vault(root, Path::new("/elsewhere/a.md")).is_err());
        assert!(note_path_in_vault(root, Path::new("../outside.md")).is_err());
        assert!(note_path_in_vault(root, Path::new("")).is_err());
    }
}
