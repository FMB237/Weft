//! CLI entry point.
#![expect(
    clippy::print_stdout,
    reason = "CLI output is the user-facing interface"
)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use editor_core::{document::Document, vault::Vault};

#[derive(Parser)]
#[command(name = "editor-cli")]
#[command(about = "Markdown editor CLI", long_about = None)]
struct Cli {
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
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => {
            let vault = Vault::open(&path)?;
            vault.save_config()?;
            println!("Initialized vault at {}", path.display());
        }
        Commands::Create { path, content } => {
            let vault = Vault::open(path.parent().unwrap_or(&std::path::PathBuf::from(".")))?;
            let content = content.unwrap_or_default();
            vault.write(&path, &content)?;
            let _doc = Document::new(path, &content);
            println!("Created markdown file");
        }
        Commands::Read { path } => {
            let vault = Vault::open(path.parent().unwrap_or(&std::path::PathBuf::from(".")))?;
            let content = vault.read(&path)?;
            println!("{content}");
        }
        Commands::Update { path, content } => {
            let vault = Vault::open(path.parent().unwrap_or(&std::path::PathBuf::from(".")))?;
            vault.write(&path, &content)?;
            println!("Updated markdown file");
        }
        Commands::Delete { path } => {
            let vault = Vault::open(path.parent().unwrap_or(&std::path::PathBuf::from(".")))?;
            vault.delete(&path)?;
            println!("Deleted markdown file");
        }
        Commands::List { path } => {
            let vault = Vault::open(&path)?;
            for file in vault.list_files()? {
                println!("{}", file.display());
            }
        }
    }

    Ok(())
}
