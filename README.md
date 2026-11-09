# Markdown Editor

A local-first markdown editor written in Rust. Desktop UI is built with
**eframe/egui**; the storage, parsing, graph, search and export logic lives in
a separate, GUI-independent core crate.

> **Status:** early. The foundation builds and tests cleanly; the editor is
> usable for basic note-taking, but several planned features (rich preview,
> persistence, graph editing) are still in progress. See [PLAN.md](PLAN.md).

## Workspace

| Crate | Description |
|-------|-------------|
| `editor-core` | Vault CRUD, markdown/frontmatter parsing, link graph, layout, search index, file watching, export |
| `editor-gui` | Desktop GUI (eframe/egui): file tree, editor, preview, graph, search |
| `editor-cli` | Command-line tool to initialise vaults and manage markdown files |

## Getting started

Requires a stable Rust toolchain (edition 2024, so Rust 1.85 or newer).
Install via [rustup](https://rustup.rs/).

```bash
# Build everything
cargo build --workspace

# Run the desktop app
cargo run -p editor-gui

# Run the CLI
cargo run -p editor-cli -- --help
```

### CLI examples

```bash
cargo run -p editor-cli -- init /path/to/my_vault
cargo run -p editor-cli -- create /path/to/my_vault/note.md --content "# Hello"
cargo run -p editor-cli -- list /path/to/my_vault
```

## Vaults

A vault is a directory of markdown files with an optional `vault.toml`
config:

```toml
name = "My Vault"
extensions = ["md", "markdown"]
```

Documents may begin with TOML frontmatter:

```markdown
---
title = "My note"
tags = ["rust", "notes"]
---

# Body
```

Wikilinks (`[[other-note]]`) are parsed into a link graph used by the graph
panel and backlink queries.

## Development

The repository treats formatting, lints and tests as gates:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on pushes and pull requests
(`.github/workflows/ci.yml`).

### Layout

```
crates/
  editor-core/
    src/
      document.rs      document model + TOML frontmatter
      parser.rs        markdown parsing, wikilink extraction
      graph.rs         link graph
      graph_queries.rs path/reachability/centrality queries
      graph_updater.rs incremental graph updates from watch events
      layout.rs        force-directed layout
      layout_store.rs  persisted node positions
      vault.rs         vault config + file operations
      watcher.rs       debounced filesystem watching
      index.rs         tantivy full-text search
      export.rs        single-document / vault export
  editor-gui/
    src/
      app.rs           application state and panel orchestration
      theme.rs         colour/visual tokens
      settings.rs      user preferences
      event_bus.rs     cross-panel event dispatch
      panels/          file tree, editor split, preview, graph, search
      widgets/         markdown editor, syntax highlighting, graph canvas
  editor-cli/
    src/main.rs        clap-based CLI
```

## Export

`editor-core` exposes:

- `editor_core::export::export_document(vault, doc_path, output_path)` —
  renders a single document to a self-contained HTML file.
- `editor_core::export::export_vault_bundle(vault, output_dir)` — copies every
  markdown file in the vault (plus `vault.toml`) into an output directory.

## License

MIT. See [LICENSE](LICENSE).
