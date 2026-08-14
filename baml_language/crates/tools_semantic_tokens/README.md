# tools_semantic_tokens

A local web viewer for BAML **semantic tokens** — the LSP-style, type-aware
classification produced by `baml_lsp2_actions::semantic_tokens`. It is the
semantic-token analog of `typescript2/pkg-grammar`'s grammar-preview app, but
because semantic tokens are computed by the Rust compiler (not a portable
TextMate grammar) this tool embeds the compiler directly instead of running in
the browser.

```sh
cargo run -p tools_semantic_tokens
# semantic-tokens viewer  ->  http://127.0.0.1:4319
```

It opens a browser (pass `--no-open` to skip) with:

- **Fixtures** — every `*.baml` under
  `crates/baml_lsp2_actions_tests/test_files/semantic_tokens/`. Each is shown
  side-by-side: **Current** (live tokens) vs **Expected snapshot** (the committed
  `//- semantic_tokens` block), with changed/added/removed tokens underlined and
  a per-fixture diff badge in the sidebar. **Accept snapshot** rewrites the
  fixture exactly as `UPDATE_EXPECT=1 cargo test` would.
- **Scratchpad** — paste arbitrary BAML and see live tokens (no snapshot).

Hover any token to inspect its type; the legend maps every token type to its
color.

## Flags

- `--port <N>` preferred port (falls back to the next free port; default 4319)
- `--fixtures-dir <DIR>` browse a different directory of `*.baml` fixtures
- `--no-open` don't launch a browser

## How it works

The tool reuses the inline-assertion test harness
(`baml_lsp2_actions_tests::{parser, runner, updater}`) so the token output, the
parsed expectations, and "accept" are identical to the Rust tests — there is no
second implementation to drift. Rendering assumes ASCII source (token positions
are byte offsets); the committed fixtures are ASCII.
