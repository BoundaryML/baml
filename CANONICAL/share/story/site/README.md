# Story site build

Renders the twelve `../NN-*.md` story documents into one self-contained HTML
page: sidebar navigation with per-doc section TOCs, hash routing, light/dark
themes, [built]/[v1]/[open] status chips, the rolling-tape animation (doc 04),
the two-layer spine graphic (doc 00), mermaid diagrams, and 24 build-time
figures (`figures/`).

## Build

```bash
python3 -m venv .venv && .venv/bin/pip install markdown   # once
npm install -g @mermaid-js/mermaid-cli                     # once (diagram pre-rendering)
.venv/bin/python build.py                                  # writes studio-story.html
```

If `mmdc` is not on PATH, set `MMDC=/path/to/mmdc`. Without it the build still
succeeds, but mermaid diagrams fall back to source-text blocks that render
only in the Claude artifact viewer.

- `build.py` — the converter/assembler. Doc order, sidebar titles, and the two
  injected graphics live here. `build.py --check` verifies figure placement
  without writing anything (safe to run concurrently).
- `shell.html` — the page template: all CSS (design tokens for both themes),
  the router JS, the tape-animation and comments logic.
  `%%NAV%%`/`%%DOCS%%`/`/*%%FONTS%%*/`/`/*%%FIGCSS%%*/` are the splice points.
- `figures/` — build-time figures, one `<name>.html` fragment + `<name>.json`
  anchor spec each (the JSON says which doc and which rendered block the
  figure swaps or follows; see the manifest comment in `build.py`). The
  markdown docs keep their ASCII/table fallbacks — figures exist only in the
  rendered page, like the doc-00 spine. Shared CSS: `figures/_shared.css`
  (card base, status pills, layer panels, `--fig-ok/-warn/-err` status
  tokens) and `figures/_tree.css` (the `.rt-` run-tree component reused by
  five figures). The build fails if any figure loses its anchor.
- `fonts.css` — Inter, JetBrains Mono, and Fira Code (latin subset) embedded
  as data URIs so the optional fonts work offline and under the artifact CSP.
  Committed; regenerate only via `python3 fetch_fonts.py` (needs network).
- `studio-story.html` — the built output.

## Reader conveniences

- **Quick settings**, always visible at the bottom of the sidebar: theme
  (System default, Light, Dark), text font (Charter default, Inter, Georgia,
  Arial, Verdana), and code font (system mono default, JetBrains Mono,
  Fira Code). Persisted in localStorage.
- Comment boxes: **⌘/Ctrl+Enter** saves, **Esc** cancels (discards an edit;
  removes a brand-new empty thread).

## Review comments

The page has a built-in review workflow, designed for sharing this folder
directly (reviewers just open `studio-story.html` in a browser):

- Select any text → **＋ Comment** → the thread appears in the right-hand
  rail, anchored to the exact quoted text (page + verbatim quote +
  occurrence index). Threads hold an array of comments; each comment can be
  edited or deleted; deleting the last comment removes the thread. The
  floating button hides itself on scroll or navigation.
- Comments persist in the browser's localStorage. **Export** downloads a
  `story-comments-<date>.json` file for the reviewer to send back; each
  thread in it carries the doc id and the verbatim anchored quote.
- If a comment's anchor text is edited away in a later build, its card stays
  visible flagged "text not found" — comments are never silently dropped.

## Notes

- BAML/SQL syntax highlighting happens at build time in `build.py`; the BAML
  token rules mirror `typescript2/pkg-grammar-hljs/src/baml.js` (itself
  derived from the real lexer). No client-side highlighter runs.
- Mermaid diagrams are pre-rendered at build time (via `mmdc`) into static
  light and dark SVGs embedded in the page, so they display everywhere:
  opened as a local file, on localhost, and in the artifact viewer. No
  runtime diagram code ships in the page.
- Published artifact (redeploy by republishing `studio-story.html` to the same
  URL): https://claude.ai/code/artifact/1a6f1cad-4d46-4037-bf03-ebeda4257bf1
  (in the artifact sandbox the comment Export falls back to a copyable JSON
  prompt if downloads are blocked; the shared-folder flow is the primary one).
- Editing the markdown is all that's needed for content changes — rebuild and
  republish.
