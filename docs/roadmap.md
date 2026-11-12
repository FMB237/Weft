# Roadmap

The authoritative, checkbox-level plan lives in [PLAN.md](../PLAN.md). This
page is the short version.

## Shipped

### M0 — Stabilize the baseline

Workspace split, CI gates (`fmt`, `clippy -D warnings`, `test`), formatting and
lint cleanup.

### M1 — Daily-driver loop

Vault open/reopen, nested file tree with create/rename/delete,
syntax-highlighted editor, live preview, manual and timed auto-save, settings
persistence, export, and keyboard shortcuts.

### M2 — Connect the graph

Wikilink edges, incremental graph updates, force-directed layout persisted to
`<vault>/.editor/layout.json`, backlinks, tantivy search with snippets, tag
filters, background filesystem watching with a conflict banner, cluster
colouring, and click-to-create for unresolved link placeholders.

### M3 — Design and correctness

- Design tokens, light/dark/system themes, bundled Inter and JetBrains Mono.
- Shared chrome widgets and a persistent status bar; Editor/Graph tabs.
- YAML frontmatter fallback while preserving the original block on save.
- Escaped HTML export titles.
- CLI `--vault <root>` with vault-relative path resolution.
- Malformed and non-UTF-8 files handled without panics.
- `docs/architecture.md`, expanded tests, and 100/500/1000-node layout
  benchmarks.

### Rename

The project is Weft: `weft-core`, `weft-gui` (binary `weft`), `weft-cli`.

## Next

Ordered so each milestone makes the next one cheaper. Full checklists in
[PLAN.md](../PLAN.md).

1. **M4 — Workspace.** Tabs and split panes, per-pane back/forward history,
   outline, breadcrumbs, quick switcher, command palette v2, find & replace,
   `[[` autocomplete, clickable links in preview, image paste. Data safety
   lands here too: trash, atomic writes, `.weft/history` snapshots, workspace
   restore, and background indexing for large vaults.
2. **M5 — Properties and views.** The keystone. Typed properties over
   frontmatter, a filter/sort/group engine, and table, cards, and board views
   saved in `.weft/views.toml`.
3. **M6 — Time and tasks.** Daily notes, templates, task buckets
   (Today/Upcoming/Inbox/Done), calendar and timeline views.
4. **M7 — Connections.** Backlink context, unlinked mentions, local graph with
   hop depth, graph filters and groups, `[[note#heading]]` and embeds, search
   operators, saved searches, and `weft-cli doctor`.
5. **M8 — Canvas and media.** Whiteboard canvas, attachments and inline
   images, PDF and themed HTML export.
6. **M9 — Ship v1.0.** Performance budgets, keybinding remap, docs and
   screenshots, Linux packaging, and a month of daily dogfood.

## Explicit non-goals

No Live Preview/WYSIWYG, no Mermaid or LaTeX rendering, no realtime
collaboration, no mobile or web app, no JS plugin ecosystem, and no hosted sync
service. Weft is a single-user, local-first desktop tool; the differentiator is
views and structure over plain Markdown files, built into the core.
