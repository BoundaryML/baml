# tree-sitter-baml

Tree-sitter grammar for [BAML](https://boundaryml.com), the AI agent
programming language — classes, enums, interfaces with generics, LLM
functions with Jinja prompts, an expression language (lambdas, `match`
patterns, `let`/`const` bindings, loops, `catch`/`throw`, `spawn`/`await`),
and more.

- Grammar name: `baml` · file type: `.baml` · scope: `source.baml`
- Queries shipped: `queries/highlights.scm`, `queries/injections.scm`
  (prompt bodies are injected as `jinja`)
- `src/` contains the **generated** parser (`parser.c`) so editors can build
  this repo directly — do not edit it; it is regenerated from `grammar.js`
  on every sync.

## Neovim (nvim-treesitter)

```lua
local parser_config = require('nvim-treesitter.parsers').get_parser_configs()
parser_config.baml = {
  install_info = {
    url = 'https://github.com/BoundaryML/baml-treesitter',
    files = { 'src/parser.c' },
    branch = 'main',
  },
  filetype = 'baml',
}
vim.filetype.add({ extension = { baml = 'baml' } })
```

Then `:TSInstall baml`. Copy `queries/*.scm` into
`~/.config/nvim/queries/baml/` (or a plugin runtime path) for highlighting.

## Helix

In `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "baml"
scope = "source.baml"
file-types = ["baml"]
comment-token = "//"

[[grammar]]
name = "baml"
# Pin a specific commit (git log on this repo for the latest) so the parser
# and your queries cannot drift apart; bump the rev deliberately.
source = { git = "https://github.com/BoundaryML/baml-treesitter", rev = "<commit-sha>" }
```

Then `hx --grammar fetch && hx --grammar build`, and copy `queries/` to
`~/.config/helix/runtime/queries/baml/`.

## Zed

Zed consumes tree-sitter grammars through extensions. In an extension's
`extension.toml`:

```toml
[grammars.baml]
repository = "https://github.com/BoundaryML/baml-treesitter"
rev = "<commit sha>"
```

## Building from source

```sh
npm install -D tree-sitter-cli   # 0.25.x
npx tree-sitter generate         # only needed after editing grammar.js
npx tree-sitter test             # corpus tests
npx tree-sitter parse file.baml
```

---

## ⚠️ Read-only mirror

This repository is a **read-only mirror**, generated from
[`typescript2/pkg-grammar-treesitter`](https://github.com/BoundaryML/baml)
in the BAML monorepo (`src/` is emitted by `tree-sitter generate` at sync
time). **Do not edit it by hand and do not open pull requests here** — they
will be overwritten by the next sync. Report issues or contribute changes in
the monorepo instead.
