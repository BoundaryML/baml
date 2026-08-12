# pkg-grammar-hljs

The [highlight.js](https://highlightjs.org) language definition for BAML (`src/baml.js`). This package is the single source of truth; a sync workflow mirrors it to the read-only repo [BoundaryML/baml-highlightjs](https://github.com/BoundaryML/baml-highlightjs), which publishes it to npm as [`@boundaryml/baml-highlightjs`](https://www.npmjs.com/package/@boundaryml/baml-highlightjs).

- `src/baml.js` — the ESM highlight.js `LanguageFn`. Keyword lists and literal forms are derived from the lexer (`baml_language/crates/baml_compiler_lexer/src/tokens.rs`) and the TextMate grammar (`../pkg-grammar/src/baml.ts`); keep them in sync when the language changes.
- `tests/` — runs every shared fixture in `../pkg-grammar/tests/fixtures/*.baml` through `hljs.highlight` and snapshots the HTML of representative fixtures into `tests/snapshots/`.
- `mirror/` — templates (`README.md`, `package.json`, `publish.yml`) the sync workflow copies into the mirror repo.

```sh
pnpm --filter @b/pkg-grammar-hljs test
```
