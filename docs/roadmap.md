# Roadmap

## Shipped

### M0 — Stabilize the baseline

Workspace split, CI gates (`fmt`, `clippy -D warnings`, `test`), formatting and
lint cleanup.

### M1 — Daily-driver loop

Vault open/reopen, nested file tree with create/rename/delete, syntax-highlighted
editor, live preview, manual and timed auto-save, settings persistence, export,
and keyboard shortcuts.

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

## Next

Ordered by user value, not effort.

1. **Note tabs and split history.** Reopen recent notes quickly instead of one
   document at a time.
2. **Graph focus mode.** Filter the graph to a neighbourhood (N hops), search
   node titles, and jump to a node from the command palette.
3. **Backlink context.** Show the sentence around each backlink instead of only
   the source note title.
4. **Command palette v2.** Fuzzy-match commands and note titles, with recent
   items first.
5. **Editor ergonomics.** Line numbers, bracket/list continuation, and
   configurable keybindings.
6. **Export polish.** Optional dark HTML theme and PDF export.
7. **Plugin surface.** Give plugins a safe, documented event API and a settings
   hook; today plugins are lifecycle-only.
8. **Accessibility and i18n.** Full keyboard traversal audit, screen-reader
   labels, and externalized UI strings.
9. **Mobile and collaboration.** Explicitly out of scope; the local-first
   single-user design is intentional.
