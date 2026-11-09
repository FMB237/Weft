//! Export functionality for the markdown editor.
//!
//! Supports exporting:
//! - a single document as a standalone HTML file, and
//! - an entire vault as a markdown bundle (directory copy of source files).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::vault::Vault;

/// Render a single document's content into a standalone HTML file.
///
/// The produced HTML is self-contained (includes inline CSS) and can be
/// opened in any browser without additional assets.
pub fn document_to_html(content: &str, title: &str) -> String {
    let body = crate::parser::markdown_to_html(content);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 800px;
            margin: 2rem auto;
            padding: 0 1rem;
        }}
        h1, h2, h3, h4, h5, h6 {{
            color: #111;
            font-weight: 600;
            margin-top: 1.5rem;
        }}
        h1 {{ border-bottom: 2px solid #eaeaea; padding-bottom: 0.5rem; }}
        code {{
            background: #f4f4f4;
            padding: 0.1rem 0.3rem;
            border-radius: 3px;
            font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        }}
        pre {{
            background: #f4f4f4;
            padding: 1rem;
            border-radius: 5px;
            overflow-x: auto;
        }}
        pre code {{ background: transparent; padding: 0; }}
        a {{ color: #0366d6; text-decoration: none; }}
        a:hover {{ text-decoration: underline; }}
        blockquote {{
            border-left: 4px solid #ddd;
            margin: 0;
            padding-left: 1rem;
            color: #555;
        }}
        table {{
            border-collapse: collapse;
            width: 100%;
            margin: 1rem 0;
        }}
        th, td {{ border: 1px solid #ddd; padding: 0.5rem; text-align: left; }}
        th {{ background: #f4f4f4; }}
        img {{ max-width: 100%; }}
        .frontmatter {{
            background: #f8f9fa;
            border: 1px solid #e1e4e8;
            border-radius: 5px;
            padding: 0.75rem;
            margin-bottom: 1rem;
            font-size: 0.9rem;
            color: #555;
        }}
    </style>
</head>
<body>
    <header>
        <h1>{title}</h1>
    </header>
    {body}
</body>
</html>"#
    )
}

/// Export a single document as a standalone HTML file.
///
/// Writes the HTML to `output_path`. Returns the path written.
pub fn export_document(vault: &Vault, doc_path: &Path, output_path: &Path) -> Result<PathBuf> {
    let content = vault
        .read(doc_path)
        .with_context(|| format!("read document {}", doc_path.display()))?;

    let title = doc_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string();

    let html = document_to_html(&content, &title);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output dir {}", parent.display()))?;
    }

    std::fs::write(output_path, &html)
        .with_context(|| format!("write html to {}", output_path.display()))?;

    Ok(output_path.to_path_buf())
}

/// Export an entire vault as a markdown bundle.
///
/// Copies every tracked markdown file in the vault into `output_dir`,
/// preserving relative paths, plus a `vault.toml` config file. Returns
/// the output directory path.
pub fn export_vault_bundle(vault: &Vault, output_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create output dir {}", output_dir.display()))?;

    let files = vault.list_files().with_context(|| "list vault files")?;

    for rel in &files {
        let content = vault
            .read(rel)
            .with_context(|| format!("read file {}", rel.display()))?;

        let dest = output_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
        std::fs::write(&dest, content)
            .with_context(|| format!("write bundle file {}", dest.display()))?;
    }

    // Also copy the vault config if present.
    let config_path = vault.root.join("vault.toml");
    if config_path.exists() {
        let dest = output_dir.join("vault.toml");
        std::fs::copy(&config_path, &dest)
            .with_context(|| format!("copy vault config to {}", dest.display()))?;
    }

    Ok(output_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_to_html_contains_content() {
        let html = document_to_html("# Hello World", "My Note");
        assert!(html.contains("<h1>My Note</h1>"));
        assert!(html.contains("<h1>Hello World</h1>"));
    }

    #[test]
    fn export_document_writes_standalone_html() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.write(Path::new("note.md"), "# Hello World").unwrap();

        let out = dir.path().join("out").join("note.html");
        let written = export_document(&vault, Path::new("note.md"), &out).unwrap();
        assert_eq!(written, out);

        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Hello World"));
    }

    #[test]
    fn export_vault_bundle_copies_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.write(Path::new("a.md"), "# a").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        vault.write(Path::new("sub/b.md"), "# b").unwrap();

        let out = dir.path().join("bundle");
        export_vault_bundle(&vault, &out).unwrap();

        assert!(out.join("a.md").exists());
        assert!(out.join("sub/b.md").exists());
        assert_eq!(std::fs::read_to_string(out.join("a.md")).unwrap(), "# a");
        assert_eq!(
            std::fs::read_to_string(out.join("sub/b.md")).unwrap(),
            "# b"
        );
    }
}
