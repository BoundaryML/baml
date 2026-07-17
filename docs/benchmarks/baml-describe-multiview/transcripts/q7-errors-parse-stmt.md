# agent-bench 20260714-134455

Question: What kinds of errors can root.cc.Parser.parse_stmt throw, and where do they originate?

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/natural r1 | 44 | 5 | 0 | 2 | ? | ? | turns=6 cost=$0.207208 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-grep r1 | 42 | 4 | 0 | 1 | ? | ? | turns=5 cost=$0.17609450000000001 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-hybrid-slim r1 | 46 | 6 | 4 | 0 | ? | ? | turns=7 cost=$0.2314245 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |

## claude/natural r1 — commands
```
Bash	ls baml-c-compiler
Grep	parse_stmt
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Grep	class CError
Read	baml-c-compiler/baml_src/ns_cc/errors.baml
```

## claude/natural r1 — final answer

`root.cc.Parser.parse_stmt` (defined at `baml_src/ns_cc/parser.baml:147`) declares `throws CError`, and **`CError` is the only error type it can throw** — BAML's `throws` is a typed effect, and `CError` (`baml_src/ns_cc/errors.baml:3`, just a `{ message: string }`) is the sole error class in the compiler.

`parse_stmt` never throws directly in its own body. Every error originates in a callee. Tracing its call graph, there are exactly **two distinct throw sites** it can reach:

## 1. `eat` — "syntax error: unexpected '<text>'"
`parser.baml:19`. Thrown whenever the current token's kind doesn't match the expected one.

`parse_stmt` hits this both directly and transitively:
- **Directly** — the `eat` calls inside `parse_stmt`: `Semi` after a return/expr/var-decl statement (lines 152, 160, 184), `Ident`/`Assign` in an `int` declaration (157–158), and `LParen`/`RParen` around `if`/`while` conditions (165, 167).
- **Transitively** — `parse_block` (line 137) calls `eat(LBrace)`/`eat(RBrace)` (138, 143) and recurses back into `parse_stmt` (141); and `parse_primary` calls `eat(RParen)` to close a parenthesized expression (42) or a call's argument list (56).

## 2. `parse_primary` — "expected an expression, found '<text>'"
`parser.baml:61`. Thrown when the parser is looking for an operand but the current token starts no valid expression (not an int/string/`-`/`(`/identifier).

Reached whenever `parse_stmt` parses an expression: the `parse_expr()` calls at lines 151, 159, 166, 179, 183 descend through `parse_assign → parse_equality → parse_relational → parse_add → parse_mul → parse_primary`, so a malformed operand anywhere in a statement's expression surfaces here.

## Summary
| Error | Type | Origin | Trigger |
|---|---|---|---|
| `"syntax error: unexpected '…'"` | `CError` | `Parser.eat` (`parser.baml:19`) | Expected token kind not found (missing `;`, `)`, `}`, identifier, etc.) |
| `"expected an expression, found '…'"` | `CError` | `Parser.parse_primary` (`parser.baml:61`) | A required operand token isn't a valid expression start |

Note the lexer also raises `CError` (per the `errors.baml` comment), but that happens *before* parsing — by the time `parse_stmt` runs, tokens already exist, so those two sites are the only ones in its reachable path.

## claude/guided-grep r1 — commands
```
Grep	parse_stmt
Grep	Parser
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Grep	class CError
```

## claude/guided-grep r1 — final answer

`parse_stmt` (`baml_src/ns_cc/parser.baml:147`) declares `throws CError`, and `CError` (`baml_src/ns_cc/errors.baml:3`, just a `message: string`) is the **only** error type it can throw. All errors are syntax errors — there are exactly two throw sites in the file, both reachable from `parse_stmt`:

**1. `eat` — "syntax error: unexpected '<tok>'"** (`parser.baml:19`)
Thrown whenever the current token's kind doesn't match the expected kind. `parse_stmt` reaches this both directly and transitively:
- Directly in `parse_stmt`: `eat(Semi)` after `return`/var-decl/expr statements (lines 152, 160, 184), `eat(Ident)` and `eat(Assign)` in the `int x = ...` decl (157–158), `eat(LParen)`/`eat(RParen)` around `if`/`while` conditions (165, 167, 178, 179).
- Via `parse_block` (`parse_stmt:168,172,181` → `parse_block:137`): `eat(LBrace)` / `eat(RBrace)` (138, 143).
- Via the expression parser: `eat(RParen)` for a parenthesized expr (`parse_primary:42`).

**2. `parse_primary` — "expected an expression, found '<tok>'"** (`parser.baml:61`)
Thrown when the token stream is where an expression is expected but the current token starts no valid primary (not an int/str/`-`/`(`/ident). `parse_stmt` reaches this through the full expression chain: `parse_expr` → `parse_assign` → `parse_equality` → `parse_relational` → `parse_add` → `parse_mul` → `parse_primary` (lines 133→124→109→94→79→64→25), invoked from `parse_stmt` at lines 151, 159, 166, 179, and 183.

**Recursion:** because `if`/`while` bodies call `parse_block`, which loops calling `parse_stmt` again (`parser.baml:141`), a `CError` from either site can originate arbitrarily deep in nested statements/blocks and propagates straight up (no `catch` anywhere in the file).

So: one error *type* (`CError`), two *origins* — the token-mismatch check in `eat` and the "no valid expression" fallthrough in `parse_primary`.

## claude/guided-hybrid-slim r1 — commands
```
Bash	baml describe root.cc.Parser.parse_stmt --view source --output compact
Bash	baml describe parse_stmt --view source --max-lines 43 --output compact
Bash	baml describe root.cc.Parser.parse_stmt --view source --max-lines 43 --output compact
Bash	baml describe root.cc.Parser.eat root.cc.Parser.cur root.cc.Parser.parse_expr root.cc.Parser.parse_block --view source --output compact --max-lines 80
Grep
Grep	throw
```

## claude/guided-hybrid-slim r1 — final answer

`parse_stmt` (`baml_src/ns_cc/parser.baml:147-186`) is declared `throws CError`, and **`CError` is the only error type it can throw** — BAML's typed `throws` makes that exhaustive. There are exactly two `throw` sites in the whole parser, and `parse_stmt` can reach both:

**1. `CError` from `eat` — `parser.baml:19`**
`throw CError { message: "syntax error: unexpected '<text>'" }`, raised when the current token's kind doesn't match the expected one. `parse_stmt` triggers this directly through its many `self.eat(...)` calls for the required punctuation/keywords of each statement form:
- `Semi` after `return`, a `int` decl, or an expression statement
- `Ident` + `Assign` in an `int` variable declaration
- `LParen`/`RParen` around `if`/`while` conditions

**2. `CError` from `parse_primary` — `parser.baml:61`**
`throw CError { message: "expected an expression, found '<text>'" }`, raised when no valid expression start is found. This surfaces indirectly: every expression `parse_stmt` reads (`self.parse_expr()` for return values, initializers, conditions, and expression statements) flows `parse_expr → parse_assign → parse_equality → parse_relational → parse_add → parse_mul → parse_primary`, and any level in that chain may throw this.

`parse_block` (called for `if`/`while` bodies) adds no new error kind — it just re-propagates `CError` from its own `eat` calls and from the nested `parse_stmt`.

Notably, `self.cur()` (`parser.baml:8-10`) is **not** an error origin: on out-of-range it returns a synthesized `Eof` token (`?? Token { kind: Eof, ... }`) rather than throwing. So end-of-input doesn't fault in `cur` — it becomes an unexpected `Eof` token that then fails at the next `eat` (line 19) or in `parse_primary` (line 61).

All `CError` values are the plain struct defined at `baml_src/ns_cc/errors.baml:3`, carrying just a `message` string.
