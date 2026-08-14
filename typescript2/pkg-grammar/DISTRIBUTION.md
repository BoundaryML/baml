# BAML syntax highlighting: how it works and how to help

Everything about where highlighting for `.baml` files comes from — for anyone
who wants to use it in their own site or editor, fix a highlighting bug, or
bring BAML support to a tool that doesn't have it yet. Current status per
consumer lives in `mirror/SUPPORT.md`.

## Use it in your own project

- **Shiki / VitePress / Astro / docs sites**: `npm install @boundaryml/baml-grammar`
  — the default export is a ready-to-use Shiki `LanguageRegistration`. Works
  with `@shikijs/monaco` for Monaco editors too.
- **highlight.js sites**: `npm install @boundaryml/baml-highlightjs`, or load
  `dist/baml.js` from a CDN after highlight.js and it self-registers.
- **Neovim / Helix / Zed**: the tree-sitter grammar lives at
  [BoundaryML/baml-treesitter](https://github.com/BoundaryML/baml-treesitter)
  (queries included) — pin a commit and point your config at it.
- **Anything TextMate-compatible**: the raw grammar is
  `grammars/baml.tmLanguage.json` in
  [BoundaryML/textMate-baml](https://github.com/BoundaryML/textMate-baml),
  alongside a `.sublime-syntax` and a KDE syntax definition.

## Where changes go

All grammars are maintained in this monorepo — `pkg-grammar` (TextMate +
Sublime + KDE), `pkg-grammar-hljs` (highlight.js), and `pkg-grammar-treesitter`
(tree-sitter). The GitHub mirror repos are read-only build artifacts that CI
regenerates, so highlighting fixes are always PRs to **this repo**, never to a
mirror.

```text
BoundaryML/baml (PRs welcome here; CI fans out on merge)
├─ pkg-grammar            → BoundaryML/textMate-baml
│    npm @boundaryml/baml-grammar · Linguist submodule · Shiki source URL
│    grammars/baml.sublime-syntax (bat, Sublime) · syntaxes/baml.xml (KDE)
├─ pkg-grammar-hljs       → BoundaryML/baml-highlightjs   npm @boundaryml/baml-highlightjs
└─ pkg-grammar-treesitter → BoundaryML/baml-treesitter    git-pinned (nvim/Zed/Helix)
```

Every port is tested against the shared fixtures in `tests/fixtures/`, and the
`grammar-tests` CI job runs all of them — so when the language grows, adding a
fixture shows exactly which ports need a matching update.
`tests/fixtures/showcase__golden_sample.baml` (published in the mirrors as
`samples/baml.sample`) is the canonical example file external registries use.

## Integrations that are live, and ones looking for an owner

Most consumers keep themselves up to date once connected; connecting a new one
is usually a single small PR to the upstream project. If you'd like to pick
one up, open an issue and we'll happily support you through it.

| Consumer | How it stays current | State |
|---|---|---|
| github.com (Linguist) | vendors textMate-baml as a submodule; bumped automatically | ✅ live |
| npm `@boundaryml/baml-grammar` / `@boundaryml/baml-highlightjs` | auto-published by the mirrors on every grammar change | ✅ live |
| Shiki (VitePress, Astro, most docs frameworks) | refetches the mirror weekly once registered | open — one-time PR to [shikijs/textmate-grammars-themes](https://github.com/shikijs/textmate-grammars-themes) (`sources-grammars.ts` entry + sample) |
| highlight.js language listing | npm/CDN already work; listing helps people find it | open — PR adding the package to hljs `SUPPORTED_LANGUAGES.md` |
| bat / delta (syntect) | submodules the `.sublime-syntax` pinned to a `v*` tag | open — PR to sharkdp/bat |
| Sublime Text | Package Control pulls our version tags once registered | open — registration PR |
| Kate / Pandoc (KSyntaxHighlighting) | `syntaxes/baml.xml` is ready in the mirror | open — upstream MR to KDE |
| Neovim (nvim-treesitter) | their bot bumps parser revisions after registration | open — parser-config PR |
| Helix | pins a baml-treesitter rev in `languages.toml` | open — PR |
| Zed | BoundaryML/zed-baml pins a grammar commit | open — move the pin from the old tree-sitter-baml to baml-treesitter |
| Pygments → Chroma (Sphinx/MkDocs; Hugo/Gitea) | hand-written lexer upstream; Chroma can convert from Pygments | open — the meatiest one; fixture drift shows up in our conformance CI |
| Rouge (GitLab, Jekyll) | hand-written lexer upstream | open |
| Docusaurus (Prism) | Prism is frozen upstream; a prism-react-renderer snippet covers it | open — docs page |
| GtkSourceView, Vim, Emacs, micro/nano/Notepad++ | one-off syntax files | open — great first contributions |

## Maintainer notes

- Each mirror has a write-access deploy key; private halves are monorepo
  actions secrets (`TEXTMATE_BAML_DEPLOY_KEY`, `BAML_HIGHLIGHTJS_DEPLOY_KEY`,
  `BAML_TREESITTER_DEPLOY_KEY`).
- npm publishes use OIDC trusted publishing (mirror repo + `publish.yml` in
  the npm package settings). A brand-new package needs one manual first
  `npm publish` from a mirror checkout before OIDC takes over.
- npm-published mirrors get a `v<version>` tag on every content change (bat,
  Package Control, and Linguist pin tags); the treesitter mirror is consumed
  by commit pin.
- Each `pkg-grammar-*` package's `scripts/assemble-mirror.mjs` emits the
  complete desired state of its mirror; the sync workflow rsyncs it with
  `--delete`. To add a file to a mirror, add it to the assemble script.
- `textMate-baml` predates the `baml-*` mirror naming. Renaming it to
  `baml-textmate` is fine (GitHub redirects), but do it in the same window as
  updating Linguist's submodule URL and the Shiki source URL.
- The old `BoundaryML/tree-sitter-baml` repo (pre-dates the expression
  language) stays as-is until baml-treesitter is proven in Zed, then gets
  archived with a pointer.
