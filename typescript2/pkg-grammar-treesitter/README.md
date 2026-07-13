# pkg-grammar-treesitter

Tree-sitter grammar for BAML. **This package is the single source of truth**
for the grammar; it is mirrored read-only to
[BoundaryML/baml-treesitter](https://github.com/BoundaryML/baml-treesitter)
(via `scripts/assemble-mirror.mjs`), which is what Neovim / Helix / Zed
consumers actually clone and build.

## Layout

| Path                  | What                                                             |
| --------------------- | ---------------------------------------------------------------- |
| `grammar.js`          | The grammar. Committed.                                          |
| `tree-sitter.json`    | tree-sitter 0.25 config (name `baml`, scope `source.baml`).      |
| `queries/*.scm`       | Highlight + injection queries (jinja injected into prompts).     |
| `test/corpus/*.txt`   | Corpus tests, organized by feature area.                         |
| `mirror/README.md`    | README template for the mirror repo.                             |
| `src/`, `bindings/`   | **Generated — gitignored.** Recreated by `tree-sitter generate`. |

## Develop

```sh
pnpm install                                 # gets tree-sitter-cli 0.25.x
pnpm generate                                # grammar.js -> src/parser.c
pnpm test                                    # corpus tests (test/corpus/)
./node_modules/.bin/tree-sitter parse ../pkg-grammar/tests/fixtures/<f>.baml
```

The quality bar: `tree-sitter parse` over every fixture in
`../pkg-grammar/tests/fixtures/*.baml` must produce **zero ERROR and zero
MISSING nodes** (that fixture set is generated from the real compiler's test
suite and covers the whole current language). A quick loop:

```sh
for f in ../pkg-grammar/tests/fixtures/*.baml; do
  ./node_modules/.bin/tree-sitter parse "$f" | grep -cE 'ERROR|MISSING' \
    | grep -qv '^0$' && echo "FAIL $f"
done
```

## Ground truth

When the language changes, update the grammar against (in priority order):

1. `typescript2/pkg-grammar/tests/fixtures/*.baml`
2. `baml_language/crates/baml_compiler_lexer/src/tokens.rs` (token shapes —
   e.g. identifiers may contain hyphens and `$`-joined segments)
3. `baml_language/crates/baml_compiler_syntax/src/syntax_kind.rs` +
   `baml_compiler_parser/src/parser.rs` (node taxonomy, precedence)

## Design decisions

- **No external scanner.** Raw strings `#"…"#` are supported for 1–8 hash
  levels with plain tokens: the body is a high-lexical-precedence
  "anything-but-quote" chunk plus a bare `"` token, and the closing `"#…#`
  delimiter beats the bare quote by longest-match — exactly the real
  parser's "N hashes close N hashes" rule. Backtick strings use the same
  trick for 1–3 tick ladders.
- Statements are `;`-optional (the real parser is newline-tolerant);
  ambiguities this creates are resolved with GLR conflicts plus dynamic
  precedence (constructor literals, config entries in test bodies).
- `const` is contextual (the real lexer has no `const` token), so
  `let const = x; const` parses.
- `is`-expression patterns suppress top-level destructures, mirroring the
  real parser's condition-position rule (`if r is Empty { … }`).

## Known gaps

- Raw strings support 1–8 `#` levels — the same bound as the TextMate
  grammar's `MAX_DELIMITER = 8` (`pkg-grammar/src/baml.ts`: "the lexer
  allows any count; 8 covers every realistic string"). The compiler allows
  arbitrary N, but truly unbounded delimiters would need an external
  scanner, which this grammar deliberately avoids; bump
  `MAX_RAW_STRING_HASHES` in `grammar.js` if that ever changes.
- Backtick strings support 1–3 tick ladders (same shape of limitation).
- Tagged template expressions (``tag`…` ``, BEP-049 §10) are not modeled;
  they do not appear in the fixture corpus yet.

## Mirror

`scripts/assemble-mirror.mjs --out <dir>` regenerates `src/parser.c` and
assembles the complete mirror-repo state (grammar + generated parser +
queries + corpus + `mirror/README.md` as the repo README). The
sync workflow rsyncs that over a BoundaryML/baml-treesitter checkout.
Do not edit the mirror repo by hand.
