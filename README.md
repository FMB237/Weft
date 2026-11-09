# Markdown Editor

A pure Rust markdown editor with Obsidian-like graph visualization (brain graph, backlinks, node canvas). Built with **eframe/egui** for the desktop GUI and a modular Rust core.

## Crates

| Crate | Description |
|-------|-------------|
| `editor-core` | Vault CRUD, markdown parsing, link graph, layout, search index, export |
| `editor-gui` | Desktop GUI (eframe/egui) with file tree, editor, preview, graph, search panels |
| `editor-cli` | Command-line tool to init vaults and manage markdown files |

## Getting Started

### Prerequisites

Rust (stable). Install via [rustup](https://rustup.rs/).

### Launch the GUI

```bash
cargo run -p editor-gui
```

This opens a 1200×800 window titled "Markdown Editor".

### Launch the CLI

```bash
cargo run -p editor-cli -- init /path/to/my_vault
cargo run -p editor-cli -- create /path/to/my_vault/note.md --content "# Hello"
cargo run -p editor-cli -- list /path/to/my_vault
```

Run `cargo run -p editor-cli -- --help` for the full command list.

### Run Tests

```bash
cargo test -p editor-core -p editor-gui -p editor-cli
```

## Vault

A vault is a directory of markdown files with an optional `vault.toml` config:

```toml
name = "My Vault"
extensions = ["md", "markdown"]
```

## Export

The editor-core crate exposes an `export` module:

- `editor_core::export::export_document(vault, doc_path, output_path)` — renders a single document to a self-contained HTML file.
- `editor_core::export::export_vault_bundle(vault, output_dir)` — copies every markdown file in the vault (plus `vault.toml`) into an output directory as a markdown bundle.

## Example

See `examples/vault/` for a sample vault structure.

## License

MIT
