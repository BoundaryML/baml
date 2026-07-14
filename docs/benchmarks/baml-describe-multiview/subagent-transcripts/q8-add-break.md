# agent-bench 20260714-143328

Question: Suppose I want to add C-style break; support. Based on the current architecture, which definitions and exhaustive handling sites must change from lexing through both assembly backends? Does local-variable scanning need a dedicated BreakStmt arm? Identify the best existing test location and the current loop test to extend. Cite file:line and distinguish required changes from places that already have a safe fallback.

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/guided-subagent r1 | 82 | 16 | 19 | 0 | ? | ? | turns=2 cost=$0.27734605 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m],claude-sonnet-5 |

## claude/guided-subagent r1 — commands
```
Agent	Navigate BAML break; support
Bash	baml describe --search continue,while,loop,stmt --kind enum --output compact
Bash	baml describe --search WhileStmt,ContinueStmt --output compact
Bash	baml describe cc.Stmt --view source --output compact
baml describe cc.WhileStmt --view source
Bash	baml describe --search break,Break,keyword,Lexer,Token --output compact
Bash	baml describe cc.keyword_kind --view source
baml describe cc.Token --view source
Bash	baml describe cc.Parser --view source --max-lines 400
Bash	baml describe cc.TokKind --view source
Bash	baml describe cc.scan_locals --view source
Bash	baml describe cc.gen_stmt --view source
Bash	baml describe cc.a64_gen_stmt --view source
Bash	baml describe cc.gen_stmt --view source --max-lines 46
echo ---
baml describe cc.a64_gen_stmt --view source --max-lines 44
Bash	baml describe --search while,test --kind test --file tests.baml --output compact
Bash	baml describe --search WhileStmt,while --file tests.baml --output compact
echo ---
baml describe --search while --kind test --output compact
Bash	baml describe --search while_and_compare --output compact
Bash	baml describe cc.Stmt --view usage --output compact
```

## claude/guided-subagent r1 — final answer

Based on the navigator's evidence packet, here's what adding C-style `break;` support requires:

## Required changes — lexing through both backends

**1. Lexer (two sites)**
- `baml_src/ns_cc/lexer.baml:3-32` — add a `KwBreak` variant to the `TokKind` enum (which currently stops at `KwWhile`, lines 8-9).
- `baml_src/ns_cc/lexer.baml:44-64` — add an `if (s == "break") { return TokKind.KwBreak; }` branch to `keyword_kind`. Without it, `break` falls through to the intentional `Ident` fallback and is lexed as an identifier.

**2. AST**
- `baml_src/ns_cc/ast.baml:63` — add a `BreakStmt` class (mirroring `WhileStmt` at lines 58-61) and extend the `Stmt` union.

**3. Parser**
- `baml_src/ns_cc/parser.baml:147-206` — add a `KwBreak` branch to `parse_stmt` before the bare-expression fallthrough; otherwise `break;` is parsed as an (invalid) expression statement.

**4. Both backends — no safe fallback**
- `baml_src/ns_cc/backends/x86.baml:149-193` — `gen_stmt`'s `match` lists all five current `Stmt` variants with **no `_` wildcard**, so it becomes non-exhaustive the moment `BreakStmt` exists. Add a `BreakStmt` arm.
- `baml_src/ns_cc/backends/arm.baml:190-232` — `a64_gen_stmt` mirrors x86 exactly, same exhaustive match, same requirement.
- Both also need loop-end-label tracking threaded through `gen_block`/`a64_gen_block` (`x86.baml:142`, `arm.baml:183`) and their `WhileStmt` arms so `break` knows which label to jump to — context these call sites don't currently carry.

## Does local-variable scanning need a dedicated `BreakStmt` arm?

**No.** `scan_locals` at `baml_src/ns_cc/backends/gen.baml:68-88` ends in a `_ => null` wildcard (line 84) that already safely absorbs any node declaring no locals, including the future `BreakStmt`. No source change needed here.

## Tests

- **Best location:** `baml_src/ns_cc/tests.baml`, in the loop/`while` test cluster.
- **Test to extend/model on:** `test "while_and_compare"` at `baml_src/ns_cc/tests.baml:27-28`. Add a new `compile_source(...)` test with a loop that exits early via `break;` alongside it.

## Required vs. already-safe

| Site | Status |
|---|---|
| `lexer.baml:3-32` (`TokKind`) | **Required** — add `KwBreak` |
| `lexer.baml:44-64` (`keyword_kind`) | **Required** — add `"break"` branch |
| `ast.baml:63` (`Stmt` union) | **Required** — add `BreakStmt` |
| `parser.baml:147-206` (`parse_stmt`) | **Required** — add `KwBreak` branch |
| `x86.baml:149-193` (`gen_stmt`) | **Required** — exhaustive match, no wildcard |
| `arm.baml:190-232` (`a64_gen_stmt`) | **Required** — exhaustive match, no wildcard |
| `gen.baml:68-88` (`scan_locals`) | **Safe fallback** (`_ => null`) — no arm needed |

The two backend matches are the sites most likely to bite you: they're exhaustive with no wildcard, so they'll fail to compile the instant `BreakStmt` joins the union — unlike `scan_locals`, which will silently and correctly do nothing.

