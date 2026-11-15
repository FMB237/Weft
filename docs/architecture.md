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
| `workspace.rs` | Panes, tabs, and session restore: open/close/reorder/split/merge, rename and delete propagation, atomic save to `<vault>/.weft/workspace.toml`, and sanitizing of restored state. |
| `file_ops.rs` | `FileOps` trait over a vault plus `write_atomic` (temp file, `fsync`, rename). |
| `trash.rs` | Moves deleted files and directories to the platform trash, with `<vault>/.weft/trash/` as a fallback that preserves the original path. |
| `outline.rs` | ATX heading extraction for the outline panel: line scan that skips YAML frontmatter and fenced code blocks, honours the CommonMark indent/marker rules, and reports each heading's level, line, and byte offset. `heading_at` maps an offset to the section that contains it. |
| `find.rs` | In-note find and replace: byte-accurate matches, case-insensitive by default via a lowered-text map (so offsets survive case folds that change length), replace-all, and wrap-around stepping. |
| `fuzzy.rs` | Quick-switcher scoring: case-insensitive subsequence match that rewards word boundaries, consecutive characters, and early matches; `rank` sorts stably by score then path. |
| `nav.rs` | Per-pane back/forward stacks. Entries carry the caret and scroll; revisiting the current note updates it in place, renames rewrite entries, and deletes drop them. |
| `history.rs` | File-recovery snapshots: records a note's previous contents on save into `<vault>/.weft/history/<note>/<ms>.md`, deduplicates unchanged content, and prunes to a per-note retention count. |
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
  dialog modals, panel layout, and the status bar. Owns the `Workspace` and
  keeps the active tab's buffer in the editor fields; every other tab holds its
  own unsaved buffer.
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
- `<vault>/.weft/workspace.toml` — panes, tabs, order, active tab, view mode,
  cursor/scroll hints, and the split ratio. Unsaved buffers stay in memory.
- `<vault>/.weft/history/<note>/<timestamp>.md` — recovery snapshots, one per
  changed save, capped per note. Snapshots skip `fsync` because they are
  derived data; a crash can cost the newest copy but never corrupt one.
- eframe storage — window state (`editor_app_state`) and preferences
  (`editor_settings`), serialized as JSON. `editor_app_state` also keeps the
  last vault and note as a fallback for sessions saved before tabs existed.

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

weft-gui/src/
  panels/outline.rs, panels/properties.rs
  panels/views/{table,cards,board,calendar,timeline}.rs
```

The workspace model and the navigation stacks already landed in `weft-core`
(`workspace.rs`, `nav.rs`) rather than in the GUI: session restore and
back/forward are pure data, and keeping them in the core makes the rules
testable without egui.

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
