# Markdown Editor — Working Plan

A personal, Obsidian-like Markdown editor in Rust with a real link graph,
backlinks, full-text search, and a polished desktop UI.

This document replaces the old aspirational `PLAN.md`. It describes **what
exists today**, **what is broken**, and **the order in which to fix it**.

---

## 1. Status snapshot

| Item | State |
|------|-------|
| Workspace | 3 crates: `editor-core`, `editor-gui`, `editor-cli` |
| Size | ~7,800 lines of Rust |
| Build | `cargo check --workspace --all-targets` passes |
| Tests | Core: 90 unit + 16 integration pass. GUI: 33 unit + 5 integration pass. CLI: 3 unit |
| Known failure | none |
| Git | Initialized; M0, M1, M2, and M3 milestones committed |
| Docs | `README.md`, `docs/architecture.md`, and `docs/roadmap.md` match the implementation |

### The one-sentence diagnosis

The daily-driver loop is complete: the editor, preview, file tree, graph edges,
backlinks, search, and filesystem watching all work together. The remaining gap
is design polish and a few correctness items (YAML frontmatter, export/CLI
escaping, docs).

### Core capabilities vs. GUI wiring

| Capability | Lives in | Wired to GUI? |
|---|---|---|
| Markdown parsing, wikilinks | `editor-core/src/parser.rs` | Preview + titles |
| Link graph edges | `editor-core/src/graph_updater.rs` | Yes |
| Force-directed layout | `editor-core/src/layout.rs` | Yes |
| Layout persistence | `editor-core/src/layout_store.rs` | Yes (`<vault>/.editor/layout.json`) |
| Tantivy full-text search | `editor-core/src/index.rs` | Yes (search panel) |
| File watcher | `editor-core/src/watcher.rs` | Yes (background thread) |
| Clustering | `editor-core/src/clustering.rs` | Yes (graph colouring) |
| Backlinks / centrality | `editor-core/src/graph_queries.rs` | Backlinks yes; centrality not surfaced |
| Export | `editor-core/src/export.rs` | Yes |
| Plugins | `editor-core/src/plugins.rs` | Lifecycle only |

---

## 2. Product definition (MVP)

The app is "done enough" when a person can use it as a daily notes app:

1. Open a vault folder and have it reopen on next launch.
2. Navigate a nested file tree; create, rename, and delete notes.
3. Edit Markdown with syntax highlighting and an accurate live preview.
4. Save manually and automatically, with no data loss on switch/close.
5. See a graph whose **edges reflect wikilinks**, drag nodes, and keep layout.
6. See backlinks for the open note and jump to them.
7. Search the vault and open results.
8. Export a note to standalone HTML and the vault to a bundle.
9. Use keyboard shortcuts for the common actions.
10. Experience a coherent visual design (spacing, type, color, states).

Non-goals for now: collaboration, mobile, AI, PDF export, canvas/whiteboard.

---

## 3. Architecture

Keep the three-crate split. Fix responsibilities instead of moving files.

```
markdown-editor/
├── Cargo.toml                  # workspace
├── README.md                   # match reality
├── PLAN.md                     # this document
├── .gitignore                  # /target, layout caches
├── LICENSE
├── docs/
│   ├── architecture.md
│   └── roadmap.md
├── .github/workflows/ci.yml
├── crates/
│   ├── editor-core/            # domain only, no egui dependency
│   │   ├── src/
│   │   │   ├── document.rs     # Document + frontmatter
│   │   │   ├── vault.rs        # vault root + file IO
│   │   │   ├── parser.rs       # markdown -> AST/HTML + wikilinks
│   │   │   ├── graph.rs        # LinkGraph (slotmap)
│   │   │   ├── graph_updater.rs# incremental edge building
│   │   │   ├── layout.rs       # force-directed layout
│   │   │   ├── layout_store.rs # persisted positions
│   │   │   ├── index.rs        # tantivy search
│   │   │   ├── watcher.rs      # notify + debounce
│   │   │   └── export.rs
│   │   └── tests/
│   ├── editor-gui/             # presentation + orchestration only
│   │   ├── src/
│   │   │   ├── app.rs          # single owner of app state
│   │   │   ├── theme.rs        # design tokens + apply_theme()
│   │   │   ├── panels/         # file_tree, editor_split, graph, search,
│   │   │   │                   # backlinks, settings
│   │   │   └── widgets/        # graph_canvas, markdown_editor, markdown_preview
│   │   └── tests/
│   └── editor-cli/             # thin CLI over editor-core
├── examples/vault/             # links must resolve
└── target/                     # ignored
```

### Ownership rules

- **`EditorApp` owns state.** Panels take `&mut` slices; they never clone the
  graph or own documents.
- **`editor-core` must not depend on `egui`.** Replace `egui::Pos2` with
  `[f32; 2]` or `emath::Pos2`, then drop the `egui` dependency from core.
- **One settings source.** `Settings` owns theme/font/autosave. `AppState`
  owns vault path, open document, active panel, split ratio, layout. Delete
  `Panels.theme` and `AppState.theme` duplication.
- **Every module is either wired or deleted.** No placeholders left behind.

---

## 4. UI and styling decision

### Does Rust desktop use CSS?

Not literally, unless the app uses a webview (Tauri, Dioxus, Leptos). Pure-Rust
GUI frameworks style with **theme structs and design tokens**:

| Framework | Styling model | Fit for this app |
|---|---|---|
| **egui / eframe** | `Style`, `Visuals`, `Spacing`, `TextStyle`, `FontDefinitions` set on `Context` | **Best** — custom painter already powers the graph |
| iced | `Theme` + `StyleSheet` traits; closest to a stylesheet | Good, but graph canvas and text layout are weaker |
| Slint | `.slint` markup with CSS-like properties and `@theme` | Great visuals, harder custom canvas |
| gpui (Zed) | Tailwind-like `Styled` methods | Powerful, smaller ecosystem/docs |
| Tauri / Dioxus | Real HTML + CSS/Tailwind | Highest visual ceiling, but webview, not pure Rust UI |

### Decision

**Stay on egui/eframe.** It is not "beautiful by default", but it is highly
themeable, and the graph canvas is the one place where egui's `Painter` is a
genuine advantage. Rewriting in iced/Slint would cost weeks and force a graph
rewrite.

The design system becomes an explicit layer:

- `theme.rs` exports tokens:
  - palette: `bg`, `surface`, `surface_raised`, `border`, `text`,
    `text_muted`, `accent`, `success`, `warning`, `danger`
  - radius: 4 / 6 / 10 / pill
  - spacing: 4 / 8 / 12 / 16 / 24
  - type: Inter 13 UI, Inter 15 body, JetBrains Mono 13 code
  - shadows, focus ring, hover/active states
- `apply_theme(ctx, &Theme)` calls `ctx.set_fonts`, `ctx.set_style`, and sets
  `Visuals::{panel_fill, window_fill, extreme_bg_color, selection, widgets}`.
- Bundle Inter and JetBrains Mono with `include_bytes!` so the app looks the
  same on every OS.
- Build small shared widgets: `card`, `section_header`, `icon_button`,
  `tab_bar`, `empty_state`, `toast`, `status_bar`.
- Dark-first, with a light variant and a system-follow mode.
- Accessibility floor: 4.5:1 text contrast, visible focus, 28px hit targets.

egui API notes for the pinned 0.29.1:

- `Visuals` fields: `panel_fill`, `window_fill`, `extreme_bg_color`,
  `code_bg_color`, `faint_bg_color`, `selection`, `widgets`.
- `WidgetVisuals` fields: `bg_fill`, `weak_bg_fill`, `bg_stroke`, `rounding`,
  `fg_stroke`, `expansion`.
- `Spacing` fields: `item_spacing`, `button_padding`, `window_margin`,
  `indent`, `interact_size`, `scroll`.
- `Style` fields: `text_styles`, `spacing`, `visuals`, `animation_time`.

Fallback trigger: if after the M3 theme pass the UI still feels below the bar,
evaluate `iced` or `slint` for the shell **before** adding features. Do not
migrate mid-feature.

---

## 5. Roadmap

### M0 — Stabilize (½ day)

Goal: a clean baseline to build on.

- [x] `git init` and commit the current state untouched.
- [x] Add `.gitignore` (`/target`, `*.layout.json`, OS/editor files).
- [x] Fix `Settings::default`: `font_size = 15.0`,
      `auto_save_interval_secs = 0`; failing test goes green.
- [x] Remove dead code and dependencies:
  - [x] delete `crates/editor-gui/src/panels/editor.rs` (not a module)
  - [x] remove `petgraph`, `uuid`, `walkdir`, `ignore`, `parking_lot`
  - [x] either call or remove the unused `comrak` block in `parser.rs`
- [x] Reduce warnings to zero (or explicitly allow `missing_docs` in GUI).
- [x] Rewrite `README.md` and `PLAN.md` to match reality.
- [x] Add `.github/workflows/ci.yml`: `fmt --check`, `clippy`, `test`.

**Done when:** `cargo test --workspace` is green, `cargo clippy` has no
errors, and the repo has a first commit.

---

### M1 — Make it a working editor (2 days)

Goal: usable daily for notes, even before the graph is fixed.

- [x] **State + persistence**
  - [x] Consolidate theme/settings into one `Settings`.
  - [x] Persist `Settings` and `AppState` on exit and every 30s.
  - [x] Reopen the last vault and last document on startup.
  - [x] Save `LayoutStore` to `<vault>/.editor/layout.json` on drag and exit.
- [x] **Preview**
  - [x] Add `egui_commonmark = "0.18"` (the release that targets egui 0.29).
  - [x] Replace the `TextEdit`-with-HTML in `panels/preview.rs` with
        `CommonMarkViewer`.
  - [x] Remove the duplicate preview: keep it inside the split **or** the
        right panel, not both.
  - [x] Show frontmatter as a metadata card, not as body text.
- [x] **Editor**
  - [x] Implement a real draggable split using the stored ratio.
  - [x] Apply `settings.font_size` to the editor text.
  - [x] Apply the existing `SyntaxHighlighter` through `TextEdit::layouter`.
  - [x] Dirty-state indicator and confirm-before-discard on switch/close.
  - [x] Window title: `filename — Markdown Editor` with `*` when dirty.
- [x] **File tree**
  - [x] Nested folders with expand/collapse, not a flat list.
  - [x] Context menu: New Note, New Folder, Rename, Delete, Reveal path.
  - [x] Guard destructive actions when the open note is dirty.
  - [x] Highlight the active file and keep it scrolled into view.
- [x] **Shortcuts**
  - [x] `Ctrl/Cmd+S` save, `Ctrl/Cmd+O` open vault, `Ctrl/Cmd+N` new note.
  - [x] `Ctrl/Cmd+P` command palette; `Ctrl/Cmd+G` graph (fix the mismatch).
  - [x] `Ctrl/Cmd+Shift+P` export note; `Ctrl/Cmd+Shift+B` export vault.
  - [x] `Ctrl/Cmd+W` close note; `Ctrl/Cmd+Tab` cycle notes.
  - [x] Implement Undo/Redo or remove the menu entries.
- [x] **Auto-save**
  - [x] Respect `settings.auto_save_interval_secs` (0 = off), not hardcoded 30s.

**Done when:** open → edit → preview → save → switch → quit → relaunch restores
the exact document with no lost edits.

---

### M2 — Graph, backlinks, search (3 days)

Goal: deliver the Obsidian-like features that make this project distinct.

- [x] **Build edges**
  - [x] Replace the node-only `rebuild_graph()` with `GraphUpdater`, or reuse
        its logic, so every wikilink becomes an edge.
  - [x] Fix `resolve_link` (`graph_updater.rs`) with this order:
        1. exact relative path
        2. `target + ".md"`
        3. case-insensitive match on file stem
        4. vault-root match
        5. otherwise keep as an unresolved placeholder node styled differently
  - [x] Skip wikilinks inside fenced code blocks.
  - [x] Add tests for `[[Home]]`, `[[Projects/Alpha]]`, aliases, missing
        targets, and duplicate links.
  - [x] Fix `examples/vault` links so the demo shows a connected graph.
- [x] **Graph panel**
  - [x] Stop cloning the graph in `panels/graph.rs`; pass `&mut LinkGraph`.
  - [x] Invalidate the layout cache on node/edge identity change, not just
        counts.
  - [x] Reset centering when the vault changes.
  - [x] Persist and reload node positions across sessions.
  - [x] Draw labels outside nodes, radius by degree, hover tooltip, edge
        arrows, click-empty-to-deselect.
  - [x] Optional: color nodes by `cluster_graph`.
  - [x] Clicking an unresolved placeholder creates the missing note and opens
        it (path-safety checked).
- [x] **Backlinks**
  - [x] New `panels/backlinks.rs` showing `GraphQuery::backlinks` for the open
        note; clicking opens the source note.
- [x] **Search**
  - [x] Index the vault on open and update on save/delete.
  - [x] Render `SearchPanel` (dock or tab) with results, snippets, and
        keyboard navigation.
  - [x] Replace the `snippet = title` stub in `index.rs` with a real snippet.
  - [x] Index frontmatter tags and support `#tag` filters, including tag-only
        searches.
- [x] **Watcher (optional but recommended)**
  - [x] Run `Watcher` on a background thread; poll the channel each frame.
  - [x] Apply events through `GraphUpdater` and `SearchIndex`.
  - [x] If dirty locally, never overwrite the buffer; show a conflict banner.

**Done when:** the example vault shows a connected graph, clicking a node
opens the note, backlinks update after saving, and search returns useful
results.

---

### M3 — Design and polish (2 days)

Goal: make it feel like a finished product.

- [x] **Design system**
  - [x] Implement `apply_theme` with the tokens above.
  - [x] Bundle Inter + JetBrains Mono.
  - [x] Build shared `card`, `section_header`, `icon_button`, `tab_bar`,
        `empty_state`, `toast`, `status_bar` widgets.
  - [x] Style menu bar, panels, scrollbars, selection, and focus rings.
  - [x] Light theme + system follow.
  - [x] Verify contrast and hit targets.
- [x] **Correctness**
  - [x] Support YAML frontmatter (`serde_yaml` or `gray_matter`); preserve the
        original block on save instead of reserializing.
  - [x] Escape HTML in export titles (Markdown body HTML is left to
        pulldown-cmark).
  - [x] Fix CLI path handling: `--vault <root>` plus paths relative to root;
        current `create` can produce `vault/vault/note.md`.
  - [x] Handle malformed files without panicking; log and continue.
- [x] **Docs and tests**
  - [x] `docs/architecture.md`, `docs/roadmap.md`.
  - [x] Tests for wikilink resolution, frontmatter round-trip, and settings.
  - [x] Benchmark graph layout at 100 / 500 / 1000 nodes.

**Done when:** the app looks intentional, `cargo test` is green, and the
README screenshots/description match what the app actually does.

---

## 6. Bug backlog with exact locations

| # | Bug | Location | Status |
|---|---|---|---|
| 1 | Graph has no edges | `editor-gui/src/app.rs` `rebuild_graph` | fixed in M2 |
| 2 | Graph edits a clone | `editor-gui/src/panels/graph.rs:33` | fixed in M1 |
| 3 | Preview shows raw HTML | `editor-gui/src/panels/preview.rs:31` | fixed in M1 |
| 4 | Split ratio ignored | `editor-gui/src/panels/editor_split.rs:41` | fixed in M1 |
| 5 | State/settings never saved | `editor-gui/src/app.rs:41`, `:575` | fixed in M1 |
| 6 | Auto-save hardcodes 30s | `editor-gui/src/app.rs:219` | fixed in M1 |
| 7 | Shortcut labels are wrong | `editor-gui/src/app.rs:262`, `:435` | fixed in M1 |
| 8 | `Settings::default` font 0.0 | `editor-gui/src/settings.rs:15` | fixed in M0 |
| 9 | Frontmatter is TOML-only | `editor-core/src/document.rs:93` | fixed in M3 (YAML fallback) |
| 10 | Wikilinks do not resolve `.md` | `editor-core/src/graph_updater.rs:81` | fixed in M2 |
| 11 | `comrak` configured but unused | `editor-core/src/parser.rs:49` | fixed in M0 |
| 12 | Search snippet is the title | `editor-core/src/index.rs:128` | fixed in M2 |
| 13 | Export title is unescaped | `editor-core/src/export.rs:20` | fixed in M3 |
| 14 | CLI can double the vault path | `editor-cli/src/main.rs:64` | fixed in M3 (`--vault`) |
| 15 | Dead `panels/editor.rs` | `editor-gui/src/panels/` | fixed in M0 |
| 16 | Layout cache ignores identity | `editor-gui/src/widgets/graph_canvas.rs:99` | fixed in M2 |

---

## 7. Quality gates

Run before every merge:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Keep these behavioral tests as the safety net:

- `Vault` CRUD round-trip and extension filtering.
- `extract_wikilinks` (plain, alias, nested path, code fence).
- `GraphUpdater` create/modify/delete/rename.
- `LayoutStore` JSON round-trip and apply/capture.
- `export_document` / `export_vault_bundle`.
- Event bus publish/drain/subscribe.

Add GUI-logic tests as pure functions, not by driving egui:

- split-ratio clamping
- file-tree visible range
- dirty-state transition table
- shortcut map

---

## 8. Decision log

| Date | Decision | Reason |
|---|---|---|
| 2026-11-09 | Keep egui/eframe | Graph painter already works; pure Rust; themeable enough |
| 2026-11-09 | Preview via `egui_commonmark 0.18` | Matches egui 0.29; least code; fallback = custom renderer |
| 2026-11-09 | No CSS layer | Pure Rust UI uses design tokens; avoid a webview |
| 2026-11-09 | Core must not depend on egui | Keep domain portable and testable |
| 2026-11-09 | Graph edges via `GraphUpdater` | Logic exists and is tested |
| 2026-11-10 | Watcher on a dedicated thread with current-thread tokio | Keeps egui frame loop non-blocking; std channel is polled per frame |
| 2026-11-10 | Unresolved links become placeholder nodes | Keeps the graph honest and makes missing notes visible |

---

## 9. Order of work (short version)

1. M0 — clean baseline, green tests, first commit.
2. M1 — usable editor with preview, tree operations, persistence.
3. M2 — connected graph, backlinks, search, watcher.
4. M3 — design system, YAML frontmatter, export/CLI fixes, docs.

Do not start M3 visuals before M1 behavior. Do not rewrite the core.
