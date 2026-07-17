# agent-bench 20260714-134711

Question: Suppose I want to add C-style break; support. Based on the current architecture, which definitions and exhaustive handling sites must change from lexing through both assembly backends? Does local-variable scanning need a dedicated BreakStmt arm? Identify the best existing test location and the current loop test to extend. Cite file:line and distinguish required changes from places that already have a safe fallback.

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/natural r1 | 105 | 11 | 0 | 10 | ? | ? | turns=12 cost=$0.51479 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-grep r1 | 121 | 12 | 0 | 8 | ? | ? | turns=13 cost=$0.5923235 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-hybrid-slim r1 | 120 | 11 | 2 | 7 | ? | ? | turns=12 cost=$0.5075805 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |

## claude/natural r1 — commands
```
Bash	ls -la && echo "---FILES---" && find . -type f -not -path '*/\.*' | head -100
Read	baml-c-compiler/README.md
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/gen.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/x86.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/arm.baml
Read	baml-c-compiler/baml_src/ns_cc/tests.baml
Read	baml-c-compiler/baml_src/ns_cc/errors.baml
Read	baml-c-compiler/baml_src/ns_cc/driver.baml
```

## claude/natural r1 — final answer

I've now read the full pipeline. Here's the complete change surface for adding C-style `break;`, traced from lexing through both backends.

## Required changes (definitions)

**1. Lexer — `lexer.baml`**
- `TokKind` enum (`lexer.baml:3`): add `KwBreak` alongside the other keyword kinds (e.g. after `KwWhile` at line 11).
- `keyword_kind` (`lexer.baml:44`): add a `if (s == "break") { return TokKind.KwBreak; }` arm before the final `TokKind.Ident` fallback (line 63).
- **Safe fallback — no change:** the `tokenize` identifier branch (`lexer.baml:130-136`) already routes every identifier through `keyword_kind`, so the main loop needs nothing. `single_char_kind` is irrelevant (`break` is alphabetic).

**2. AST — `ast.baml`**
- Add a `class BreakStmt` node. Note it's a marker with no children; every existing node has ≥1 field (`IntLit`…`WhileStmt`), so if BAML rejects a zero-field class you'll need a dummy field (e.g. `dummy: int`).
- `Stmt` union (`ast.baml:63`): add `| BreakStmt`.

**3. Parser — `parser.baml`**
- `parse_stmt` (`parser.baml:147`): add a `if (k == TokKind.KwBreak) { self.pos += 1; self.eat(TokKind.Semi); return BreakStmt {...}; }` arm alongside the `KwReturn`/`KwInt`/`KwIf`/`KwWhile` arms. `parse_block` (`parser.baml:137`) is generic and needs nothing.

## Required changes (exhaustive handling sites)

**4. Shared context — `gen.baml`**
- `Gen` class (`gen.baml:7`): add a break-target field, e.g. `break_label: string`. A single field (not a stack) suffices because `gen_stmt` recurses, so nested loops save/restore in their own call frame.
- Both `Gen {...}` constructor literals must initialize it: **`x86.baml:229`** and **`arm.baml:268`**. These are required edits — the literals are exhaustive over fields.

**5. x86 backend — `x86.baml`**
- `gen_stmt` (`x86.baml:149`): the `match` is **exhaustive with no `_` wildcard** (arms end at `WhileStmt`, line 192). Adding `BreakStmt` to the union *forces* a new arm here — emit `jmp <gen.break_label>`.
- `WhileStmt` arm (`x86.baml:180`): wrap the body gen (line 187) with save/restore of `gen.break_label` around `end_l` (line 182): read old value, set `gen.break_label = end_l`, `gen_block(...)`, restore. Field reassignment is supported (cf. `gen.locals = {}` at line 196).

**6. ARM backend — `arm.baml`**
- `a64_gen_stmt` (`arm.baml:190`): same — **exhaustive, no wildcard** (ends at `WhileStmt`, line 231). New `BreakStmt` arm emits `b <gen.break_label>`.
- `WhileStmt` arm (`arm.baml:220`): save/restore `gen.break_label` around the body (line 226), using `end_l` (line 221).

## Does local-variable scanning need a dedicated BreakStmt arm?

**No.** `scan_locals` (`gen.baml:68`) already has a `_ => null` wildcard (`gen.baml:84`). `BreakStmt` declares no locals and holds no nested statement bodies, so the wildcard handles it correctly with no dedicated arm. This is the one match site that is a genuine safe fallback — in deliberate contrast to `gen_stmt`/`a64_gen_stmt`, which are exhaustive and *do* require new arms.

Likewise `gen_expr`/`a64_gen_expr` (`x86.baml:98`, `arm.baml:141`) match over `Expr`, not `Stmt`, so they're untouched.

## One design gap to decide

`break` outside any loop leaves `break_label` as `""`, producing a garbage `jmp ` / `b `. The clean handling: in the two `BreakStmt` arms, `throw CError { message: "break outside loop" }` when `gen.break_label == ""` (both `gen_stmt` functions already have `throws CError`). Not strictly required to compile valid programs, but it's the correct-parity behavior with `offset_of`'s undeclared-variable check (`gen.baml:57`).

## Tests

- **Best location:** `tests.baml` — these are the deterministic, no-LLM tests (`baml test`). All seven use the x86 path via `compile_source` and assert on emitted asm; there's no arm64 test precedent, so match that.
- **Current loop test to extend:** `while_and_compare` (`tests.baml:27`), which already asserts `setl`/`je`/`jmp`. Add a dedicated case rather than overloading it, e.g.:

```
test "break_exits_loop" {
    let asm = compile_source("int main(void) { int i = 0; while (i < 10) { i = i + 1; if (i == 3) { break; } } return i; }");
    assert.contains(asm, "jmp ");   // break + loop back-edge both present
}
```

A precise assertion would check that `break` targets the same `.Lendwhile{N}` label the loop's exit uses. If you want arm64 coverage too, add a parallel test through `compile_source_arm64` (`driver.baml:79`) asserting a `b Lendwhile…`.

## Summary table

| Layer | Site | Status |
|---|---|---|
| Lexer | `TokKind` `lexer.baml:3`, `keyword_kind` `lexer.baml:44` | **Required** |
| Lexer | `tokenize` loop `lexer.baml:130` | Safe fallback (routes via `keyword_kind`) |
| AST | new `BreakStmt`, `Stmt` union `ast.baml:63` | **Required** |
| Parser | `parse_stmt` `parser.baml:147` | **Required** |
| Frame scan | `scan_locals` `gen.baml:68` (`_` at :84) | **Safe fallback — no arm** |
| Context | `Gen` field `gen.baml:7` + literals `x86.baml:229`, `arm.baml:268` | **Required** |
| x86 | `gen_stmt` arm `x86.baml:149`, `WhileStmt` save/restore `x86.baml:180` | **Required (exhaustive, no wildcard)** |
| ARM | `a64_gen_stmt` arm `arm.baml:190`, `WhileStmt` save/restore `arm.baml:220` | **Required (exhaustive, no wildcard)** |
| Expr codegen | `gen_expr`/`a64_gen_expr` | Untouched (matches `Expr`, not `Stmt`) |
| Tests | `tests.baml`, extend near `while_and_compare` `tests.baml:27` | Recommended |

Want me to implement this end-to-end and run `baml test`?

## claude/guided-grep r1 — commands
```
Bash	ls -la && echo "---" && git log --oneline -5 2>/dev/null || echo "no git"
Bash	find baml_src -type f | head -100 && echo "---EXAMPLES---" && ls examples
Bash	for f in lexer ast parser; do echo "=== $f.baml ==="; wc -l baml_src/ns_cc/$f.baml; done; echo "=== backends ==="; wc -l baml_src/ns_cc/backends/*.baml baml_src/ns_cc/tests.baml baml_src/ns_cc/driver.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/gen.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/x86.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/arm.baml
Read	baml-c-compiler/baml_src/ns_cc/tests.baml
Read	baml-c-compiler/baml_src/ns_cc/driver.baml
Bash	cd baml_src/ns_cc && rg -n 'Gen \{|match \(s\)|_ =>|break;|WhileStmt|BreakStmt' .
```

## claude/guided-grep r1 — final answer

I have the full pipeline. Here's the change surface for C-style `break;`, from lexing through both backends.

## Required changes (won't compile or won't work otherwise)

**1. Lexer — `lexer.baml`**
- Add `KwBreak` to the `TokKind` enum (`lexer.baml:3-32`).
- Add a `"break" → TokKind.KwBreak` case in `keyword_kind`, before the `Ident` fallback at `lexer.baml:63`.
- `single_char_kind` needs nothing — `break` is a word, tokenized by the alpha branch at `lexer.baml:130-136`.

**2. AST — `ast.baml`**
- Add `class BreakStmt {}` — it carries no fields.
- Add it to the `Stmt` union at `ast.baml:63` (`... | WhileStmt | BreakStmt`).

**3. Parser — `parser.baml`**
- Add a `TokKind.KwBreak` branch in `parse_stmt` (`parser.baml:147-186`): advance, `eat(TokKind.Semi)`, return `BreakStmt {}`. `parse_block` (`parser.baml:137-145`) already loops over arbitrary statements, so no other parser change.

**4. x86 backend — `x86.baml`**
- `gen_stmt`'s `match (s)` (`x86.baml:150-192`) is **exhaustive with no `_` wildcard** — adding `BreakStmt` to the union forces a new arm here or the file won't compile. The arm emits `jmp <loop-end-label>`.
- The `WhileStmt` arm (`x86.baml:180-191`) must publish its `end_l` so the nested break can reach it (push before `gen_block`, pop after).

**5. arm backend — `arm.baml`**
- `a64_gen_stmt`'s `match (s)` (`arm.baml:190-231`) is likewise exhaustive with no wildcard — a `BreakStmt` arm is required, emitting `b <loop-end-label>`.
- The `WhileStmt` arm (`arm.baml:220-230`) must publish its `end_l` the same way.

**6. Shared context — `gen.baml` (the real architectural addition)**
The loop-exit label is today a *local* (`end_l`) inside each backend's while arm, so a nested statement has no way to see it. You need a loop-label stack on the shared `Gen` class (`gen.baml:7-64`), e.g. a `break_labels: string[]` field plus push/peek/pop. Both while arms push their end label; both break arms peek it. Because it's a new field on `Gen`, **both constructor literals must be updated**: `x86.baml:229` and `arm.baml:268`. (A break with an empty stack — i.e. outside any loop — should raise `CError`; that's the natural place for the "break statement not within a loop" diagnostic.)

## Already has a safe fallback — no dedicated arm needed

- **Local-variable scanning: no `BreakStmt` arm required.** `scan_locals` (`gen.baml:68-88`) matches `Stmt` with a `_ => null` catch-all at `gen.baml:84`. A `break` declares no variables and has no nested body, so it correctly falls through the wildcard. This is the one exhaustive `Stmt` match that does *not* need touching.
- **Expression codegen is untouched.** `gen_expr` (`x86.baml:98-140`) and `a64_gen_expr` (`arm.baml:141-181`) match over `Expr`, not `Stmt`. `break` is a statement, so these exhaustive matches are unaffected.
- **Parser block/loop plumbing.** `parse_block` and the `eat`/`cur` helpers already handle any statement; only the new `parse_stmt` branch is required.

## Tests

- **Best location:** `tests.baml` — the deterministic, no-LLM suite (`baml test`).
- **Loop test to extend:** `while_and_compare` at `tests.baml:27-32`. Extend it (or clone it beside) with a break in the loop body, e.g. `while (i < 3) { if (i == 1) { break; } i = i + 1; }`, and assert the emitted `jmp` targets the same `endwhile` label the loop's `je` uses.
- **Coverage gap to note:** every test here goes through `compile_source` → `codegen` (x86 only). The arm64 path (`compile_source_arm64`, `driver.baml:79`) has *no* existing tests, so the `arm.baml` break arm would be exercised only if you add an arm64 assertion (`assert.contains(asm, "b Lendwhile...")`).

## claude/guided-hybrid-slim r1 — commands
```
Bash	baml describe --search statement,keyword,loop,while --output compact 2>&1 | head -60
Bash	baml describe --search codegen,emit,assembly,backend --kind function --output compact 2>&1 | head -60
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Grep	local|scan|collect|VarDecl
Read	baml-c-compiler/baml_src/ns_cc/backends/gen.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/x86.baml
Read	baml-c-compiler/baml_src/ns_cc/backends/arm.baml
Grep	function (gen_block|a64_gen_block)
Read	baml-c-compiler/baml_src/ns_cc/tests.baml
```

## claude/guided-hybrid-slim r1 — final answer

I now have the complete pipeline. Here's the analysis.

## Summary

`break;` cuts across lexer → AST → parser → **both** codegen backends. The load-bearing constraint is that both `match (s)` sites over `Stmt` are **exhaustive with no wildcard arm**, so adding a `BreakStmt` variant *forces* new arms there (compile error otherwise) — whereas `scan_locals` has a `_ => null` catch-all and needs no change.

## Required changes, in pipeline order

**1. Lexer — `lexer.baml`**
- `TokKind` enum (`lexer.baml:3`): add `KwBreak`. **Required.**
- `keyword_kind` (`lexer.baml:44`): add `if (s == "break") { return TokKind.KwBreak; };` before the `TokKind.Ident` fallback at `lexer.baml:63`. This is a *misleadingly* safe fallback — without it `break` lexes as an `Ident`, no error, but wrong semantics. **Required.**
- `single_char_kind` (`lexer.baml:66`): no change — `break` is a keyword, not punctuation.

**2. AST — `ast.baml`**
- Add `class BreakStmt { }` (an empty/marker node). **Required.**
- `Stmt` union (`ast.baml:63`): add `| BreakStmt`. **Required** — this is what triggers the exhaustiveness obligations below.

**3. Parser — `parser.baml`**
- `parse_stmt` (`parser.baml:147`): add an arm alongside the other keyword dispatches (e.g. after the `KwWhile` block at `parser.baml:176-182`): `if (k == TokKind.KwBreak) { self.pos += 1; self.eat(TokKind.Semi); return BreakStmt { }; };`. **Required** — without it, a `KwBreak` token falls through to `parse_expr()` at `parser.baml:183` and throws `"expected an expression, found 'break'"`.

**4. Codegen — the real design work (both backends)**

The hard part: `break` must jump to the *enclosing* loop's end label, but today `end_l` is a local created inside the `WhileStmt` arm (`x86.baml:182`, `arm.baml:222`) and never threaded anywhere. You must carry the current loop-exit target. Cleanest shared mechanism:

- `Gen` class (`gen.baml:7`): add a stack field, e.g. `break_labels: string[]`, and initialize it at both `Gen { ... }` constructions (`x86.baml:229`, `arm.baml:268`). **Required** (some context-carrying mechanism is unavoidable).

Then in **each** backend:
- **x86 `gen_stmt`** (`x86.baml:149`, exhaustive `match`, no `_`):
  - `WhileStmt` arm (`x86.baml:180-191`): push `end_l` before `gen_block(wh.body, gen)` (`x86.baml:187`), pop after. **Required.**
  - Add a `BreakStmt` arm emitting `jmp <top-of-break_labels>`. **Required** (both for behavior *and* to keep the match exhaustive).
- **arm `a64_gen_stmt`** (`arm.baml:190`, also exhaustive):
  - `WhileStmt` arm (`arm.baml:220-230`): push/pop `end_l` around `a64_gen_block(wh.body, gen)` (`arm.baml:226`). **Required.**
  - Add a `BreakStmt` arm emitting `b <top-of-break_labels>`. **Required.**

Note: `gen_block` (`x86.baml:142`) and `a64_gen_block` (`arm.baml:183`) need **no signature change** with the `Gen`-field-stack approach — they just forward to `gen_stmt`. (Only the alternative of threading a label parameter would touch them; the `Gen` stack is why I recommend it.)

## Does local-variable scanning need a dedicated `BreakStmt` arm?

**No.** `scan_locals` (`gen.baml:68`) matches `VarDecl`/`IfStmt`/`WhileStmt` and ends with `_ => null` (`gen.baml:84`). `BreakStmt` declares no variables, so the wildcard handles it correctly. This is a genuine safe fallback — leave it. (Contrast with the two `gen_stmt` matches, which have no wildcard and therefore *must* gain arms.)

## Tests

- **Best location:** `tests.baml` — the deterministic, no-LLM/no-token suite (`tests.baml:1`).
- **Loop test to extend/mirror:** `while_and_compare` (`tests.baml:27-32`). `compile_source` emits x86 there (it asserts `setl`/`je`/`jmp`), so add a sibling test like:
  ```
  test "break_exits_loop" {
      let asm = compile_source("int main(void) { int i = 0; while (i < 10) { i = i + 1; if (i > 3) { break; } } return i; }");
      assert.contains(asm, "jmp ");   // break jumps to the .Lendwhile label
  }
  ```
  Modeling it on `while_and_compare` gives you the loop scaffolding for free; assert that the `break` produces a `jmp` to the loop's end label.

## Safe-fallback / no-change sites (don't touch)

- `scan_locals` wildcard (`gen.baml:84`) — handles `BreakStmt` correctly.
- `single_char_kind` (`lexer.baml:66`) — keyword, not punctuation.
- Expression layer: `parse_expr`/`parse_primary` (`parser.baml:25-135`) and `gen_expr` — `break` is a statement.
- Driver `compile_source`/`compile`/`codegen`/`codegen_arm64` (`x86.baml:228`, `arm.baml:267`) — they just orchestrate parse→codegen; no arm to add.
- `gen_block` / `a64_gen_block` — unchanged under the recommended `Gen`-stack design.

The one subtle trap: the `keyword_kind` `Ident` fallback (`lexer.baml:63`) and the `parse_stmt` `parse_expr` fallback (`parser.baml:183`) will *silently accept or mis-error* on `break` rather than cleanly reject it, so both are required edits even though the code "compiles" without them. The only place a fallback is legitimately sufficient is `scan_locals`.
