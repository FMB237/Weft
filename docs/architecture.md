# Architecture

Weft is a three-crate Rust workspace. The core crate owns all domain
logic and has no dependency on the GUI; the GUI is a thin presentation layer
over it, and the CLI exposes the same vault operations for scripts.

```
weft-core   domain: vault IO, parsing, graph, layout, search, watcher, export
weft-gui    eframe/egui desktop app
weft-cli    clap command-line interface
```

## Data flow

```
filesystem ──► Vault ──► parser/document ──► LinkGraph ──► layout ──► GraphCanvas
     │             │                       │
     │             ├──► SearchIndex        └──► backlinks / queries
     │             └──► export
     └──► notify watcher ──► WatchEvent ──► index + graph updates
```

`EditorApp` holds the authoritative GUI state: the open vault, the current
document buffer, the link graph, the layout store, the search index, and the
watcher. Panels never own vault data; they publish events (`NodeSelected`,
`CreateNote`, `EntryDeleted`, …) through an in-process event bus, and the app
dispatches them once per frame.

## weft-core

| Module | Responsibility |
|---|---|
| `vault.rs` | Opens a vault root, reads/writes/deletes files and directories, lists markdown files and directories, skips hidden bookkeeping folders. |
| `document.rs` | Document model and `Frontmatter`. Frontmatter is parsed as TOML first, then YAML; malformed blocks fall back to an empty frontmatter. |
| `parser.rs` | Markdown → HTML, wikilink extraction (fenced code excluded), shared frontmatter splitting, YAML→TOML value conversion. |
| `graph.rs` | `LinkGraph` stored in a `slotmap`: nodes keyed by path, weighted edges, stable ids. |
| `graph_updater.rs` | Builds and incrementally updates the graph from vault files; resolves wikilink targets and keeps unresolved links as placeholders. |
| `layout.rs` | Deterministic force-directed layout in graph space. |
| `layout_store.rs` | Persists node positions to `<vault>/.editor/layout.json`. |
| `index.rs` | Tantivy in-memory full-text index with snippets and exact frontmatter tag filters. |
| `clustering.rs` | Label-propagation community detection used for graph colouring. |
| `graph_queries.rs` | Backlinks, shortest path, centrality. |
| `watcher.rs` | `notify` watcher on a background thread with debouncing; exposes a channel of `WatchEvent`s. |
| `plugins.rs` | Lifecycle-only plugin registry and dispatch. |
| `export.rs` | Standalone HTML for one document (with escaped title) and a markdown bundle for a vault. |

### Frontmatter

A block starts on the first non-blank line with `---` and ends at a line
containing only `---` or `...`. LF and CRLF are accepted. Parsing tries TOML
(`key = value`) first and falls back to YAML (`key: value`, block sequences).
Unknown keys are collected into `Frontmatter::other`; null YAML values are
dropped because TOML has no null.

The GUI saves the raw editor buffer verbatim, so the original frontmatter
formatting and dialect survive a round-trip. `Document::full_content()` is the
non-GUI serializer and always writes TOML.

## weft-gui

- `app.rs` — eframe entry point, persisted `AppState`, keyboard shortcuts,
  dialog modals, panel layout, and the status bar.
- `theme.rs` — design tokens (`Palette`), light/dark/system resolution,
  `Theme::apply` (visuals, spacing, text styles), bundled Inter and
  `JetBrains Mono` fonts, and contrast helpers used by tests.
- `widgets/chrome.rs` — shared `card`, `section_header`, `icon_button`,
  `tab_bar`, `empty_state`, `badge`, `toast`, and `status_bar` widgets.
- `widgets/markdown_editor.rs` — multiline editor with syntax-highlighted
  layout.
- `widgets/graph_canvas.rs` — custom-painted graph with pan/zoom, hover,
  selection, cluster colouring, and theme-aware colours.
- `panels/` — file tree, editor/preview split, graph, search, and backlinks.

Rendering is immediate-mode: each frame reads app state, draws, and dispatches
events after all panels have run. Repaints are requested only when needed
(animation, dirty auto-save timers, watcher events).

## Persistence

- `<vault>/vault.toml` — vault name and markdown extensions.
- `<vault>/.editor/layout.json` — graph node positions.
- eframe storage — window/app state (`editor_app_state`) and preferences
  (`editor_settings`), serialized as JSON.

## Planned modules (M4–M8)

The forward plan in `PLAN.md` adds bookkeeping under `<vault>/.weft/` and two
layers to the core, both free of `egui` types:

```
weft-core/src/
  properties.rs   typed property schema, inference, coercion
  query.rs        filter/sort/group over a note set
  views.rs        saved view definitions
  tasks.rs        checkbox extraction and buckets
  daily.rs        daily-note path resolution
  templates.rs    template expansion
  history.rs      file-recovery snapshots
  trash.rs        system trash with in-vault fallback

weft-gui/src/
  workspace.rs    panes, tabs, session restore
  nav.rs          per-pane back/forward history
  panels/outline.rs, panels/properties.rs
  panels/views/{table,cards,board,calendar,timeline}.rs
```

Rule: views are **data** (a filter/sort/group description in
`.weft/views.toml`), evaluated by `weft-core`; the GUI only renders results.
That keeps the feature testable without driving egui.

## Testing

- Unit tests live beside each module.
- `crates/weft-core/tests/integration_tests.rs` covers cross-module flows,
  including malformed and non-UTF-8 files.
- `crates/weft-core/benches/graph_benchmarks.rs` benchmarks graph operations
  and layout at 100, 500, and 1000 nodes (`cargo bench -p weft-core`).
- CI runs `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace`.
