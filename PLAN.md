# Weft — Working Plan

Weft is a personal, local-first Markdown workspace in Rust. Plain files on
disk, typed properties on top, and several views over the same notes — with no
database, no webview, and no account.

This document describes **what exists today**, **what "modern" means for this
app in 2026**, and **the order in which the rest gets built**.

---

## 1. Status snapshot

| Item | State |
|------|-------|
| Workspace | 3 crates: `weft-core`, `weft-gui`, `weft-cli` (`weft` binary + `weft-cli`) |
| Size | ~9,400 lines of Rust |
| Build | `cargo check --workspace --all-targets` passes |
| Tests | Core 90 unit + 16 integration. GUI 34 unit + 5 integration. CLI 3 unit |
| Known failure | none |
| Git | M0–M3 shipped, plus the Weft rename |
| Docs | `README.md`, `docs/architecture.md`, `docs/roadmap.md` match the implementation |

### The one-sentence diagnosis

Weft today is a competent 2021-era Markdown editor: one note at a time, a real
wikilink graph, backlinks by title, and full-text search. It is not yet a
workspace — it has no tabs, no properties, no views over notes, and no safety
net under destructive actions.

---

## 2. Product definition (v1.0)

**Thesis.** Weft is a local-first thinking workspace: plain Markdown files,
typed properties on top, and many views over the same notes. Obsidian's power
with Capacities' defaults and the speed of a native Rust binary.

**Three pillars.**

1. **Write well** — a fast editor, quick capture, and a good reading surface.
2. **See structure** — typed properties and multiple views (table, board,
   calendar, timeline, cards) over the same files.
3. **Follow connections** — graph, backlinks with context, unlinked mentions,
   and a query language that spans all of it.

**Definition of done for v1.0 (personal scope).** You use Weft daily for a
month without opening another notes app, and you never fear losing a file.

### Scope decisions

- **Single user.** No collaboration, sharing, or presence.
- **Local only.** No hosted sync service. Backup is the user's job (git,
  Syncthing, rsync); Weft's job is to never corrupt or silently delete a file.
- **Plain Markdown.** Notes stay readable and editable in any other tool; no
  proprietary block database.
- **One window, many panes.** Native desktop only. No mobile, no web build.
- **Personal tool.** No i18n, no onboarding wizard, no telemetry, no
  auto-update. Install with `cargo install --path` or a release archive.

### Non-goals, with reasons

| Not building | Why |
|---|---|
| Live Preview / WYSIWYG inline rendering | egui's `TextEdit` is a plain buffer; inline-rendered Markdown means writing a text layout engine. Source + preview, both excellent, instead. |
| Mermaid, LaTeX | No pure-Rust renderers at the expected quality. Revisit only as an optional external-binary integration. |
| Realtime collaboration | Requires conflict CRDTs and a server; contradicts single-user local-first. |
| Mobile / web app | egui is desktop-first; a second frontend would double the surface. |
| JS plugin ecosystem | Sandboxing and API stability are their own project. WASM plugins may come after v1.0. |
| Cloud sync | Optional later integration (git or WebDAV), never a Weft account. |

**The bet:** the differentiator is not a richer editor or a bigger plugin
catalogue. It is **views and structure over plain files**, built into the core
instead of bolted on.

---

## 3. Competitive baseline (November 2026)

Checked against shipping releases, not memory.

| App | Mechanism worth learning from |
|---|---|
| **Obsidian 1.15** | **Bases**: one note set, many views (table, cards, list, kanban, map) driven by frontmatter, with filters, formulas, grouping, CSV export. Canvas, properties, outline, tabs/groups, daily notes, PDF export, clickable links in Live Preview. |
| **Notion** | Databases as a first-class object: typed properties, relations/rollups, table/board/calendar/timeline/gallery, templates. |
| **Capacities** | Objects over folders, the daily note as inbox, **related content** (unlinked mentions), templates, AI over your own notes. |
| **Logseq / Tana** | Outliner blocks, journals, queries, supertags — typed nodes without leaving the writing surface. |
| **Heptabase** | Whiteboard-first: notes as cards arranged spatially. |
| **Anytype** | Local-first objects, relations, and sets with encryption. |
| **Bear** | Tag-first plain-Markdown writing with a narrow, fast surface. |

**The pattern:** the modern leap is not a better editor. It is *views and
structure on top of plain notes*. That is the gap Weft closes in M5.

### What Weft has today

| Capability | Lives in | Wired to GUI? |
|---|---|---|
| Markdown parsing, wikilinks | `weft-core/src/parser.rs` | Preview, titles |
| Link graph edges | `weft-core/src/graph_updater.rs` | Yes |
| Force-directed layout | `weft-core/src/layout.rs` | Yes |
| Layout persistence | `weft-core/src/layout_store.rs` | Yes (`<vault>/.editor/layout.json`) |
| Tantivy full-text search | `weft-core/src/index.rs` | Yes (search panel) |
| File watcher | `weft-core/src/watcher.rs` | Yes (background thread) |
| Clustering | `weft-core/src/clustering.rs` | Yes (graph colouring) |
| Backlinks / centrality | `weft-core/src/graph_queries.rs` | Backlinks by title; centrality not surfaced |
| Export | `weft-core/src/export.rs` | Yes (HTML document + vault bundle) |
| Plugins | `weft-core/src/plugins.rs` | Lifecycle only |

### What is missing (the honest list)

**Fundamentals**

- No tabs, split panes, back/forward navigation, or per-note cursor restore.
- No outline panel, breadcrumbs, quick switcher, or find & replace.
- No `[[` or `#tag` autocomplete.
- Wikilinks are not clickable in preview; they render as literal text.
- Delete is permanent: no trash, no snapshots, no atomic-write guarantee.
- No image paste/display, attachments, or drag-and-drop file moves.
- No properties panel, aliases, `[[note#heading]]`, or note embeds.

**Modern mechanisms**

- No views over notes: no table, board, calendar, timeline, or cards.
- No typed properties, so no filters, sorts, or grouping to build views on.
- No daily notes, templates, or task aggregation.
- No local graph, graph filters, or unlinked mentions.
- No canvas or whiteboard.

---

## 4. Architecture and ownership rules

Keep the three-crate split. Fix responsibilities instead of moving files.

```
weft/
├── Cargo.toml
├── README.md
├── PLAN.md                     # this document
├── docs/{architecture,roadmap}.md
├── .github/workflows/ci.yml
├── crates/
│   ├── weft-core/              # domain only, no egui dependency
│   ├── weft-gui/               # presentation + orchestration only
│   └── weft-cli/               # thin CLI over weft-core
├── examples/vault/
└── target/                     # ignored
```

### Ownership rules

- **`EditorApp` owns state.** Panels take `&mut` slices; they never clone the
  graph or own documents.
- **`weft-core` must not depend on `egui`.** Replace `egui::Pos2` with
  `[f32; 2]` before M5; new core modules take no GUI types.
- **One settings source.** `Settings` owns theme/font/autosave; `AppState`
  owns the workspace.
- **Every module is either wired or deleted.** No placeholders left behind.
- **Views are data, not code.** A saved view is a filter/sort/group
  description that the core evaluates; the GUI renders the result.

### Modules added by this plan

```
weft-core/src/
  properties.rs   typed property schema, inference, coercion, validation
  query.rs        filter/sort/group evaluation over a note set
  views.rs        saved view definitions (.weft/views.toml)
  tasks.rs        checkbox extraction, due dates, buckets
  daily.rs        daily-note path/format resolution
  templates.rs    template expansion ({{date}}, {{title}}, {{cursor}})
  history.rs      file-recovery snapshots (.weft/history)
  trash.rs        system trash with in-vault fallback

weft-gui/src/
  workspace.rs    panes, tabs, session restore
  nav.rs          per-pane back/forward history
  panels/outline.rs
  panels/properties.rs
  panels/views/{table,cards,board,calendar,timeline}.rs
  quick_switcher.rs
```

---

## 5. Storage and data model

Everything user-visible stays plain Markdown. Bookkeeping lives in `.weft/`,
which the vault walker must always ignore.

| Path | Contents |
|---|---|
| `vault.toml` | Vault config: extensions, daily-note folder/format, template folder, property overrides |
| `.weft/workspace.toml` | Panes, tabs, order, active tab, split ratio, cursor/scroll per tab |
| `.weft/views.toml` | Saved views: name, source folder/query, filter, sort, group, visible properties, layout |
| `.weft/history/<path>/<timestamp>.md` | File-recovery snapshots, written on save, with retention |
| `.weft/trash/` | Fallback when the system trash is unavailable |
| `.weft/layout.json` | Graph node positions. Today they live in `.editor/layout.json`; moving them is optional and would need a one-time migration, not a rewrite |

**Property model.** Types are inferred from frontmatter (ISO dates, numbers,
booleans, arrays, `[[links]]`, `#tags`) and can be pinned in `vault.toml` under
`[properties]`. One schema feeds views, task buckets, and canvas cards.

**Why `views.toml` and not Obsidian `.base`.** No clutter in the note list and
no partially-compatible file format. If interop ever matters, exporting a view
to `.base` YAML is a bounded feature, not a data migration.

---

## 6. Roadmap

### Shipped

| Milestone | Scope | Commit |
|---|---|---|
| M0 | Clean baseline, CI gates, dependency and dead-code removal | `648fb53` |
| M1 | Daily-driver loop: tree CRUD, editor, preview, autosave, shortcuts, persistence | `b566c31` |
| M2 | Wikilink edges, layout persistence, backlinks, search, watcher, clusters | `c0e04d6`, `6ca85ba` |
| M3 | Design system, themes, bundled fonts, YAML frontmatter, export/CLI correctness, docs, benchmarks | `90343fe`, `aaaf7f5` |
| — | Rename to Weft; path normalization; honest state-load errors | `94206c3`, `1508ea9` |

---

### M4 — Workspace

**Goal:** stop being a one-note-at-a-time app, and make destructive actions
recoverable before anything can bulk-edit the vault.

**Tabs and panes**

- [ ] `workspace.rs`: `Pane { tabs, active }`, one or two panes, horizontal and
      vertical split, drag a tab between panes, overflow menu.
- [ ] Tab ops: new (`Ctrl+T`), close (`Ctrl+W`), cycle (`Ctrl+Tab`), reopen
      closed (`Ctrl+Shift+T`), close others / close to the right.
- [ ] Tabs are `(path, mode, cursor, scroll)`; two tabs on one note are legal.
- [ ] Session restore writes the whole workspace (panes, tabs, order, active,
      split ratio) to `.weft/workspace.toml` instead of one `open_document`.
- [ ] `nav.rs`: per-pane back/forward stack (`Alt+Left/Right`, mouse buttons),
      entries carry cursor and scroll, not just a path.
- [ ] Breadcrumbs in the editor header: `vault / folder / note.md`.

**Find and open**

- [ ] Outline panel: headings from the parser, active-heading highlight,
      click-to-scroll, filter box.
- [ ] `Ctrl+O` becomes the quick switcher (open vault moves to `Ctrl+Shift+O`):
      fuzzy over title, path, and aliases; recent first; `Enter` creates the
      note when nothing matches.
- [ ] Command palette v2: fuzzy, grouped, recent-first, shortcut hints,
      context-filtered (note / graph / view).
- [ ] `Ctrl+F` becomes in-note find & replace; vault search moves to
      `Ctrl+Shift+F`.
- [ ] Recent and pinned notes in the sidebar.

**Editor fundamentals**

- [ ] Wikilinks clickable in preview (opens the note; modifier-click opens a
      new tab).
- [ ] `[[` note autocomplete and `#` tag autocomplete from the index.
- [ ] Smart lists: continue bullets/numbers/checkboxes on `Enter`, indent and
      outdent with `Tab`, `Enter` on an empty item ends the list.
- [ ] Paste a URL over a selection → `[text](url)`.
- [ ] Drag a note from the tree into the buffer → `[[note]]`.
- [ ] Per-note cursor, scroll, and mode (source / split / preview) restore.
- [ ] Clipboard image paste saved to an attachments folder and inserted as a
      Markdown image.

**Data safety**

- [ ] Delete moves to the system trash (`trash` crate) with `.weft/trash/`
      fallback; File tree and CLI share the same path.
- [ ] Atomic writes everywhere: temp file, `fsync`, rename.
- [ ] `history.rs`: snapshot on every save into `.weft/history/`, retention
      policy, and a **File recovery** dialog with restore.
- [ ] A crashed session's unsaved buffer is offered back on next launch.

**Scale**

- [ ] Move tantivy indexing and graph rebuild off the frame loop; paint the
      shell first and fill panels when ready.
- [ ] Cold start under 2s and first paint under 300ms for a 10,000-note vault.

**Done when:** five notes live in two panes, history goes back three steps, a
note is reachable without the tree, links in preview navigate, and every
deleted or overwritten file is recoverable.

---

### M5 — Properties and views (keystone)

**Goal:** the modern leap. One note set, many views, driven by frontmatter.

- [ ] `properties.rs`: types `text`, `number`, `date`, `bool`, `select`,
      `multi-select`, `link`, `tags`; inference from values; overrides in
      `vault.toml`; tolerant coercion (bad values never crash a view).
- [ ] Properties panel in the editor: form-style editor that writes back to
      frontmatter without disturbing unknown keys.
- [ ] Aliases (`aliases:` frontmatter) usable by links and the quick switcher.
- [ ] `query.rs`: filter groups (all/any), operators (`is`, `is not`,
      `contains`, `before`, `after`, `is empty`), multi-key sort, group-by.
- [ ] `views.rs` + `.weft/views.toml`: name, source (folder or query), filter,
      sort, group, visible properties, layout.
- [ ] **Table view** (`egui_extras::TableBuilder`): sortable columns, inline
      property editing, row → open note, CSV export.
- [ ] **Cards view**: gallery of note cards with a chosen property as subtitle.
- [ ] **Board view**: kanban grouped by a select property, drag between
      columns writes the property back.
- [ ] View chrome: view switcher tabs, filter bar, "N of M results" footer.

**Done when:** a folder of meeting notes becomes a sortable table and a kanban
board without touching Markdown, and dragging a card changes its frontmatter.

---

### M6 — Time and tasks

**Goal:** a reason to open Weft every morning.

- [ ] `daily.rs`: daily-note folder and filename format from `vault.toml`;
      ribbon button and `Ctrl+Shift+D`; opens or creates today's note.
- [ ] `templates.rs`: template folder, `{{date}}`, `{{time}}`, `{{title}}`,
      `{{cursor}}`; used by daily notes and "new from template".
- [ ] `tasks.rs`: extract `- [ ]` items with note, line, text, due date, and
      priority from properties.
- [ ] Tasks view: **Today**, **Upcoming**, **Inbox**, **Done** buckets; toggle
      completion writes back to the source line.
- [ ] **Calendar view**: month grid, notes by date property or daily note.
- [ ] **Timeline view**: notes on a horizontal axis by date range.
- [ ] Agenda panel on the right: today's tasks + today's daily note.

**Done when:** one shortcut opens today's note, and every open task in the
vault is visible in exactly one bucket.

---

### M7 — Connections

**Goal:** make the graph and search navigation tools, not decoration.

- [ ] Backlink **context**: the sentence around each link, not just the title.
- [ ] **Unlinked mentions**: notes that mention the title without linking,
      with one-click linking.
- [ ] Local graph: neighbourhood of the open note at 1–3 hops, with a hop
      control and a jump back to the full graph.
- [ ] Graph filters and groups: by tag, folder, search string; saved settings.
- [ ] Hover previews for wikilinks in preview and graph nodes.
- [ ] `[[note#heading]]` targets and `![[note]]` / `![[image.png]]` embeds.
- [ ] Search operators: `path:`, `tag:`, `prop:status=done`, `"phrase"`,
      `-exclude`; accent-insensitive matching.
- [ ] Saved searches in the sidebar.
- [ ] `weft-cli doctor`: broken links, orphan notes, duplicate titles,
      unparseable frontmatter.

**Done when:** you can start at any note, walk three hops, and find the
unlinked note that should have been linked.

---

### M8 — Canvas and media

**Goal:** spatial thinking, without leaving the vault.

- [ ] Canvas document type (`.canvas` JSON): note cards, text cards, links
      between cards, pan/zoom, snapping, multi-select, undo.
- [ ] Card actions: create note from card, open in tab, resize, colour.
- [ ] Edges carry labels; arrows follow Obsidian's canvas shape so files stay
      portable.
- [ ] Image paste, attachment folder, inline image rendering in preview.
- [ ] Drag a note from the tree onto the canvas or into the editor.
- [ ] Export: PDF via the OS print pipeline, HTML with light/dark themes.
- [ ] Print stylesheet for notes and tables.

**Done when:** a board of cards is as usable as the note view, and images
survive a round-trip to another Markdown editor.

---

### M9 — Ship v1.0

**Goal:** a month of daily use without surprises.

- [ ] Performance pass: incremental index updates, background graph rebuild,
      60fps table scrolling with 5,000 rows.
- [ ] Conflict UI for external edits (the banner exists; make it actionable).
- [ ] Keybinding remap and a keyboard-traversal audit.
- [ ] `docs/` refresh, screenshots, a 10-minute tour in the README.
- [ ] Release builds: Linux AppImage (and a plain tarball), `cargo install`
      instructions verified from a clean checkout.
- [ ] Dogfood for four weeks; every bug found goes into section 8.

**Done when:** a month passes without opening another notes app and without
losing a byte.

---

## 7. Quality gates

Run before every merge:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Behavioral tests that must stay green:

- `Vault` CRUD, extension filtering, hidden-directory skipping.
- `extract_wikilinks` (plain, alias, nested path, fenced code).
- `GraphUpdater` create/modify/delete/rename.
- `LayoutStore` round-trip and apply/capture.
- `export_document` / `export_vault_bundle`.
- Event bus publish/drain/subscribe.

New tests this plan requires (pure functions, never by driving egui):

- property inference and coercion, including malformed values
- filter/sort/group evaluation on a fixture note set
- view round-trip through `.weft/views.toml`
- daily-note path formatting across date boundaries
- template expansion, including missing variables
- task extraction and bucket assignment
- diff-tolerant snapshot writing and retention
- workspace model: open/close/reorder/restore tabs
- navigation history push/back/forward semantics

Performance budgets, checked with the criterion benches:

| Operation | Budget |
|---|---|
| Graph layout, 1,000 nodes | existing bench, no regression |
| Table view, 5,000 rows | 60fps scroll |
| Cold start, 10,000 notes | < 2s to first paint |
| Save snapshot | < 5ms for a 100KB note |

---

## 8. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Views become a mini-database engine | M5 drags for months | Start with one source (a folder), three layouts, and six operators; grow only when a real vault needs it. |
| Bulk property edits destroy data | Loss of trust | Trash, atomic writes, and snapshots land in M4, before M5 ships. |
| egui table/board performance | Sluggish UI | `TableBuilder` with virtualised rows; boards render only visible cards; measure at 5,000 notes in CI benches. |
| Canvas scope creep | Never ships | Cards + text + edges only; no component system, no HTML embeds. |
| Search and properties disagree | Confusing results | One note-metadata cache in core feeds index, views, and graph. |
| Personal-scope rot | Endless polish, no v1 | v1.0 is a date and a dogfood month, not a feature count. |

---

## 9. Decision log

| Date | Decision | Reason |
|---|---|---|
| 2026-11-09 | Keep egui/eframe | Graph painter already works; pure Rust; themeable enough |
| 2026-11-09 | Preview via `egui_commonmark 0.18` | Matches egui 0.29; least code; fallback = custom renderer |
| 2026-11-09 | No CSS layer | Pure Rust UI uses design tokens; avoid a webview |
| 2026-11-09 | Core must not depend on egui | Keep domain portable and testable |
| 2026-11-09 | Graph edges via `GraphUpdater` | Logic exists and is tested |
| 2026-11-10 | Watcher on a dedicated thread | Keeps the egui frame loop non-blocking |
| 2026-11-10 | Unresolved links become placeholder nodes | Keeps the graph honest and makes missing notes visible |
| 2026-11-12 | Rename to Weft, GUI binary `weft` | Name is the product; CLI stays `weft-cli` to avoid a build collision |
| 2026-11-12 | **Stay file + frontmatter, not an outliner** | Preserves plain Markdown, git, and the existing graph; block refs are a different product |
| 2026-11-12 | **Views are a core engine, not a plugin** | The differentiator must be native and tested, not an optional add-on |
| 2026-11-12 | **No Live Preview / WYSIWYG** | egui's text buffer cannot render inline Markdown without a layout engine; source + preview instead |
| 2026-11-12 | **Personal scope for v1.0** | One user, one vault, no i18n/onboarding/installers; ship sooner, dogfood longer |
| 2026-11-12 | **Workspace (M4) before views (M5)** | Every later surface lives in tabs and panes; data safety must precede bulk edits |

---

## 10. Order of work

1. **M4 — Workspace.** Tabs, panes, history, outline, quick switcher, editor
   fundamentals, trash and snapshots.
2. **M5 — Properties and views.** The keystone: typed properties, query engine,
   table / cards / board.
3. **M6 — Time and tasks.** Daily notes, templates, task buckets, calendar and
   timeline.
4. **M7 — Connections.** Backlink context, unlinked mentions, local graph,
   search operators, `weft-cli doctor`.
5. **M8 — Canvas and media.** Whiteboard, images, PDF and themed HTML export.
6. **M9 — Ship v1.0.** Performance, docs, packaging, a month of dogfood.

Rules of engagement: no feature starts before its milestone's data-safety
items are green; docs and tests ship in the same commit as the code; the
`PLAN.md` checklist is updated as items land, not afterwards.
