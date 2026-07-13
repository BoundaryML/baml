# BAML syntax highlighting: distribution playbook

How the grammar family reaches every syntax-highlighting registry on the
internet, and what remains to land. The architecture rule: **all sources of
truth live in this monorepo; mirrors are write-only build artifacts; no repo
we own is ever edited by hand.** See `README.md` for the package family and
`mirror/SUPPORT.md` for live status.

```text
BoundaryML/baml (humans edit here, sync-grammar-mirror fans out)
├─ pkg-grammar            → BoundaryML/textMate-baml
│    npm @boundaryml/baml-grammar · Linguist submodule · Shiki source URL
│    grammars/baml.sublime-syntax (bat, Package Control) · syntaxes/baml.xml (KDE)
├─ pkg-grammar-hljs       → BoundaryML/baml-highlightjs   npm @boundaryml/baml-highlightjs
└─ pkg-grammar-treesitter → BoundaryML/baml-treesitter    git-pinned (nvim/Zed/Helix)
```

## One-time bootstrap (before the fan-out works)

1. Create the mirror repos `BoundaryML/baml-highlightjs` and
   `BoundaryML/baml-treesitter` (public, empty, default branch `main`, with an
   initial empty commit so the sync's clone succeeds).
2. For each, generate an SSH keypair, add the public half as a write-access
   deploy key on the mirror, and store the private half as a monorepo actions
   secret: `BAML_HIGHLIGHTJS_DEPLOY_KEY`, `BAML_TREESITTER_DEPLOY_KEY`
   (pattern copied from `TEXTMATE_BAML_DEPLOY_KEY`).
3. First npm publish of `@boundaryml/baml-highlightjs` must be manual (OIDC
   cannot create a package): `npm publish --access public` from a mirror
   checkout, then configure the trusted publisher (repo
   BoundaryML/baml-highlightjs, workflow publish.yml) in npm settings.
4. Run the `Sync grammar mirror` workflow once via workflow_dispatch and
   confirm all three mirrors populate, the textMate/highlightjs mirrors get
   `v*` tags, and npm publishes land.

## Downstream landings (each is a one-time PR; propagation is automatic after)

Ordered by reach-per-effort. Fixtures in `tests/fixtures/` are the shared
conformance suite; `tests/fixtures/showcase__golden_sample.baml` (mirrored as
`samples/baml.sample`) is the sample to submit everywhere.

| # | Registry | Action | Auto-update mechanism afterwards |
|---|---|---|---|
| 1 | [Shiki](https://github.com/shikijs/textmate-grammars-themes) | PR: entry in `sources-grammars.ts` pointing at the textMate-baml mirror blob URL + `samples/baml.sample`. Fork exists: BoundaryML/textmate-grammars-themes | Shiki refetches the mirror weekly |
| 2 | GitHub Linguist | shipped — keep the auto submodule-bump | Linguist releases (~quarterly) |
| 3 | [highlight.js](https://github.com/highlightjs/highlight.js) | PR adding `@boundaryml/baml-highlightjs` to `SUPPORTED_LANGUAGES.md` (third-party section) | mirror auto-publishes npm/CDN |
| 4 | [bat](https://github.com/sharkdp/bat) | PR: submodule `assets/syntaxes/02_Extra/baml` → textMate-baml (pin a `v*` tag) + `bat cache --build` regression assets | bat maintainers bump submodules each release |
| 5 | Sublime Package Control | PR to [packagecontrol channel](https://github.com/wbond/package_control_channel) registering textMate-baml (releases from `v*` tags) | Package Control pulls tags |
| 6 | [Pygments](https://github.com/pygments/pygments) | PR: hand-written `BamlLexer` + `samples/baml.sample` as example file | manual; fixture drift caught by our conformance CI reminder |
| 7 | [Chroma](https://github.com/alecthomas/chroma) | PR: XML lexer converted from the Pygments lexer (`_tools/pygments2chroma`) | regenerate + re-PR on drift |
| 8 | [Rouge](https://github.com/rouge-ruby/rouge) | PR: hand-written Ruby lexer + spec from golden sample | manual |
| 9 | [KSyntaxHighlighting](https://invent.kde.org/frameworks/syntax-highlighting) | MR: `syntaxes/baml.xml` (staged in the mirror) | re-MR on drift |
| 10 | [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) | PR: parser config pointing at baml-treesitter + our queries | their update bot bumps revisions |
| 11 | [Helix](https://github.com/helix-editor/helix) | PR: `languages.toml` entry pinning a baml-treesitter rev | re-PR to bump (infrequent) |
| 12 | Zed | Update BoundaryML/zed-baml `extension.toml` grammar pin from the old tree-sitter-baml to baml-treesitter | monorepo workflow can bump the pin |
| 13 | Docusaurus users | docs page: prism-react-renderer custom-language snippet (Prism itself is frozen — do not upstream) | n/a (docs) |
| 14 | "Works today" docs | docs page: Shiki custom lang via `@boundaryml/baml-grammar`, `@shikijs/monaco` for Monaco, hljs `registerLanguage` via `@boundaryml/baml-highlightjs` | n/a (docs) |
| 15 | Long tail | GtkSourceView, classic Vim syntax, Emacs mode, micro/nano/Notepad++ — community-friendly issues | manual |

Open decision: `textMate-baml` predates the `baml-*` mirror naming convention
(`baml-highlightjs`, `baml-treesitter`). Renaming it to `baml-textmate` would
be consistent — GitHub redirects old URLs — but Linguist's submodule URL and
the Shiki source URL should then be updated in the same window. Defer until
both of those PRs exist, then decide.

The old `BoundaryML/tree-sitter-baml` repo (last pushed 2025-04, pre-dates the
expression language) stays untouched until baml-treesitter is proven in Zed;
then archive it with a pointer.
