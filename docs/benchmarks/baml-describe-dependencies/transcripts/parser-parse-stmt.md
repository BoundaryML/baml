# Claude `root.cc.Parser.parse_stmt` dependency transcripts

Sanitized export preserving measurements, commands, and final answers.


Question: For the method root.cc.Parser.parse_stmt, identify its contract dependencies including its return and error types and where they are defined, then identify its direct implementation dependencies such as referenced fields, methods, enum variants, and classes. Explain which dependencies matter most before changing root.cc.Parser.parse_stmt. Include file:line citations.

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/natural r1 | 70 | 9 | 0 | 4 | ? | ? | turns=10 cost=$0.32678300000000005 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/natural r2 | 66 | 9 | 0 | 5 | ? | ? | turns=10 cost=$0.34140000000000004 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-grep r1 | 69 | 10 | 0 | 4 | ? | ? | turns=11 cost=$0.3304435 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-grep r2 | 67 | 7 | 0 | 0 | ? | ? | turns=8 cost=$0.2428965 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-hybrid-slim r1 | 60 | 4 | 1 | 3 | ? | ? | turns=5 cost=$0.2244315 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-hybrid-slim r2 | 50 | 5 | 1 | 3 | ? | ? | turns=6 cost=$0.18090050000000002 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |

## claude/natural r1 — commands
```
Bash	ls -la && echo "---" && find . -name "cc*" -not -path '*/node_modules/*' 2>/dev/null | head
Grep	def parse_stmt|parse_stmt
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Bash	ls -R baml-c-compiler/baml_src
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/errors.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Bash	cat baml-c-compiler/baml.toml; echo "---GREP cc alias---"; grep -rn "as cc\|namespace\|ns_cc" baml-c-compiler/baml_src/ns_cc/driver.baml 2>/dev/null | head
Grep	ReturnStmt|VarDecl|IfStmt|WhileStmt|ExprStmt|parse_stmt|match .*Stmt|: Stmt
```

## claude/natural r1 — final answer

`root.cc` maps to the `baml_src/ns_cc/` namespace. The method is defined at `baml_src/ns_cc/parser.baml:147-186`.

## Contract dependencies (the signature)

`function parse_stmt(self) -> Stmt throws CError` (`parser.baml:147`)

- **Return type `Stmt`** — a union type alias: `type Stmt = ReturnStmt | ExprStmt | VarDecl | IfStmt | WhileStmt;` at `ast.baml:63`. Its five member classes:
  - `ReturnStmt` — `ast.baml:39`
  - `ExprStmt` — `ast.baml:43`
  - `VarDecl` — `ast.baml:47`
  - `IfStmt` — `ast.baml:52`
  - `WhileStmt` — `ast.baml:58`
- **Error type `CError`** — `class CError { message: string }` at `errors.baml:3`.

## Direct implementation dependencies (the body, `parser.baml:148-185`)

**Fields (on `self: Parser`, defined `parser.baml:4-6`):**
- `self.pos` — advanced directly (`:150, :156, :171`) and via helpers.

**Methods called:**
- `self.cur()` — `parser.baml:8` (returns `Token`), used for dispatch on `.kind`.
- `self.eat(TokKind)` — `parser.baml:16`; consumes a token and is the source of the `throws CError` contract (`:19`). Called for `Semi`, `Ident`, `Assign`, `LParen`, `RParen`.
- `self.parse_expr()` — `parser.baml:133` (returns `Expr`), for return values, initializers, conditions, and bare expression statements.
- `self.parse_block()` — `parser.baml:137` (returns `Stmt[]`), for `if`/`else`/`while` bodies — a mutually-recursive call back into `parse_stmt`.

**Enum variants — `TokKind` (`lexer.baml:3-32`):** dispatched on `KwReturn` (`:8`), `KwInt` (`:7`), `KwIf` (`:11`), `KwElse` (`:12`), `KwWhile` (`:13`); and consumed via `eat`: `Ident`, `Assign`, `Semi`, `LParen`, `RParen`.

**Classes constructed:** `ReturnStmt{value}` (`:153`), `VarDecl{name,init}` (`:161`), `IfStmt{cond,then_body,else_body}` (`:174`), `WhileStmt{cond,body}` (`:181`), `ExprStmt{expr}` (`:185`) — plus the returned `Expr`/`Token` types (`ast.baml:37`, `lexer.baml:34`).

## What matters most before changing `parse_stmt`

1. **The `Stmt` union and its five node classes (`ast.baml:39-63`).** This is the real blast radius. `parse_stmt` is the *only* producer of `Stmt` values, but the three code-generation backends are the consumers that pattern-`match` on the union — `backends/gen.baml`, `backends/x86.baml`, `backends/arm.baml`. Adding, removing, or renaming a statement kind (or reshaping a node's fields) breaks those `match` sites and requires updating all three. Treat any change to the shape of what `parse_stmt` returns as a cross-file contract change.

2. **`eat()` and the `CError` contract (`parser.baml:16`, `errors.baml:3`).** Every mandatory-token consumption and every syntax error flows through `eat`. If you add a new statement form, route its punctuation through `eat` so error behavior and the `throws CError` propagation stay consistent — callers up the chain (`parse_block`, `parse_function`, top-level `parse` at `:209`) rely on that error type propagating.

3. **`TokKind` variants (`lexer.baml:3`).** Dispatch is by leading token. Any new statement keyword must first be produced by the lexer — added to the enum and to `keyword_kind` (`lexer.baml:44`) — or `parse_stmt` will never see it.

4. **`parse_block` / `parse_expr` recursion (`parser.baml:137, :133`).** These are the recursive-descent entry points for nested bodies and expressions; changes to their return types (`Stmt[]`, `Expr`) directly affect `parse_stmt`.

Lower priority: `cur`/`peek` and `self.pos` bookkeeping are local mechanics — safe to rely on, low ripple.

The one dependency that dominates the rest is the **`Stmt` union contract**, because it is shared with the backends rather than confined to the parser.

## claude/natural r2 — commands
```
Bash	ls -la && echo "---" && find . -type d -name cc 2>/dev/null | head
Bash	find . -type f | grep -v '\.git' | head -100 && echo "---BAML.TOML---" && cat baml.toml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/errors.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Bash	grep -rn "match\|ReturnStmt\|ExprStmt\|VarDecl\|IfStmt\|WhileStmt\|parse_stmt\|parse_block" baml_src/ns_cc/backends/gen.baml baml_src/ns_cc/driver.baml 2>/dev/null | head -40
Read	baml-c-compiler/baml_src/ns_cc/backends/gen.baml
Bash	grep -rn "ReturnStmt\|ExprStmt\|VarDecl\|IfStmt\|WhileStmt\|: Stmt\|Stmt\[\]" baml_src/ns_cc/backends/x86.baml baml_src/ns_cc/backends/arm.baml 2>/dev/null | head -40
```

## claude/natural r2 — final answer

I now have everything needed. Here is the dependency analysis for `root.cc.Parser.parse_stmt`.

## `Parser.parse_stmt` — Dependency Analysis

Defined at `baml_src/ns_cc/parser.baml:147-186`. Signature: `function parse_stmt(self) -> Stmt throws CError`.

### 1. Contract dependencies (the public surface — its return & error types)

These define what callers can rely on and what a change may break outside the function body.

| Dependency | Kind | Defined at |
|---|---|---|
| `Stmt` | Return type — union alias | `baml_src/ns_cc/ast.baml:63` (`type Stmt = ReturnStmt \| ExprStmt \| VarDecl \| IfStmt \| WhileStmt`) |
| `CError` | Error type (`throws`) | `baml_src/ns_cc/errors.baml:3` |

The five `Stmt` member classes are part of the contract because callers `match` on them:
- `ReturnStmt` — `ast.baml:39`
- `ExprStmt` — `ast.baml:43`
- `VarDecl` — `ast.baml:47`
- `IfStmt` — `ast.baml:52`
- `WhileStmt` — `ast.baml:58`

### 2. Direct implementation dependencies (what the body touches)

**Fields (of `self: Parser`, defined `parser.baml:4-6`):**
- `self.pos` — read/incremented throughout (`parser.baml:148,150,157,164,171,177`)
- `self.toks` — read indirectly via `self.cur()`

**Methods called:**
- `self.cur()` — `parser.baml:8` — dispatch on current token (`148,157,170`)
- `self.eat(k)` — `parser.baml:16` — consumes/validates tokens; source of most `CError` throws (`152,157,158,165,167,184`)
- `self.parse_expr()` — `parser.baml:133` — parses `return`/`if`/`while` conditions and expr statements (`151,159,166,179,183`)
- `self.parse_block()` — `parser.baml:137` — parses `if`/`while`/`else` bodies (`168,172,181`)

**Enum variants (`TokKind`, defined `lexer.baml:3-32`):** `KwReturn` (`:5`), `KwInt` (`:4`), `KwIf` (`:7`), `KwElse` (`:8`), `KwWhile` (`:9`), `Ident` (`:6`), `Assign` (`:17`), `Semi` (`:29`), `LParen`/`RParen` (`:25-26`) — all read via `.kind` comparisons.

**Classes constructed:** the five `Stmt` variants above, built with the field shapes from `ast.baml:39-61`. Also depends on `Token` (`lexer.baml:34`) for `.kind`/`.text` accessors.

### 3. What matters most before changing `parse_stmt`

1. **The `Stmt` union and its variant shapes (`ast.baml:63`, `:39-61`) — highest impact.** The return type is consumed by exhaustive `match` in three places: `scan_locals` (`gen.baml:70-85`), `gen_stmt` for x86 (`x86.baml:149-180`), and `a64_gen_stmt` for ARM (`arm.baml:190-220`). Adding a new statement kind, or renaming/re-typing a variant's fields (e.g. `IfStmt.then_body`/`else_body`, `WhileStmt.body`), forces coordinated edits in all three backends or codegen will fail/regress. This is the dependency to reason about first.

2. **`self.eat` and the `throws CError` contract (`parser.baml:16`, `errors.baml:3`).** `parse_stmt`'s error behavior is almost entirely delegated to `eat`. Any grammar change must keep the token-consumption discipline consistent (advance `self.pos` exactly once per consumed token) or you get desyncs and misleading syntax errors. Removing `throws CError` would ripple to every caller in the `throws` chain up to `parse` (`parser.baml:209`).

3. **`parse_expr` / `parse_block` (`parser.baml:133`, `:137`).** These are the recursive building blocks. Note `parse_block` calls back into `parse_stmt` (`parser.baml:141`), so the two are mutually recursive — a change to statement-termination or block delimiters must stay consistent across both.

4. **`TokKind` variants (`lexer.baml`).** Lower risk but a hard coupling: the dispatch keys (`KwReturn`, `KwInt`, `KwIf`, `KwWhile`) must exist in the enum and be produced by the lexer's `keyword_kind` (`lexer.baml:44`). Adding a new statement keyword requires adding the enum variant *and* wiring it in the lexer, not just editing `parse_stmt`.

**Lowest concern:** `self.cur`/`self.peek`/`self.pos` are internal cursor mechanics — safe to rely on and unlikely to affect anything outside the `Parser` class.

## claude/guided-grep r1 — commands
```
Grep	parse_stmt
Grep	(class|function|type|enum)\s+Parser
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Grep	(class|enum|type)\s+(Stmt|CError|TokKind|Token|Expr|ReturnStmt|VarDecl|IfStmt|WhileStmt|ExprStmt)\b
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/errors.baml
Read	baml-c-compiler/baml_src/ns_cc/lexer.baml
Grep	match|ReturnStmt|WhileStmt
Grep	match|ReturnStmt|WhileStmt|Stmt
Grep	(ReturnStmt|ExprStmt|VarDecl|IfStmt|WhileStmt|match .*Stmt|-> .*Stmt)
```

## claude/guided-grep r1 — final answer

Here is the dependency map for `Parser.parse_stmt`, defined at `baml_src/ns_cc/parser.baml:147` (namespace `ns_cc`, i.e. `root.cc`).

## Contract dependencies (the signature)

`function parse_stmt(self) -> Stmt throws CError` (`parser.baml:147`)

- **Return type `Stmt`** — a type-alias union: `type Stmt = ReturnStmt | ExprStmt | VarDecl | IfStmt | WhileStmt` at `baml_src/ns_cc/ast.baml:63`. Every `return`/tail path in the method must produce one of these five members.
- **Error type `CError`** — `class CError { message: string }` at `baml_src/ns_cc/errors.baml:3`. It is raised only indirectly here, via the `eat` helper.
- **Receiver `Parser`** — `class Parser { toks: Token[], pos: int }` at `baml_src/ns_cc/parser.baml:4`.

## Direct implementation dependencies

**Fields (via `self`)**
- `self.pos: int` (`parser.baml:6`) — advanced directly (`self.pos += 1` at lines 150, 164, 171, 177) and indirectly by the helpers.
- `self.toks` (`parser.baml:5`) — read only through `cur()`.

**Methods called**
- `self.cur() -> Token` (`parser.baml:8`) — line 148 for the dispatch token, and lines 170/183.
- `self.eat(k: TokKind) -> Token throws CError` (`parser.baml:16`) — the sole source of the `throws CError` behavior; used for `Semi`, `Ident`, `Assign`, `LParen`, `RParen` (lines 152, 157, 158, 160, 165, 167, 184).
- `self.parse_expr() -> Expr throws CError` (`parser.baml:133`) — lines 151, 159, 166, 179, 183.
- `self.parse_block() -> Stmt[] throws CError` (`parser.baml:137`) — lines 168, 172, 181.

**Enum variants — `TokKind`** (`baml_src/ns_cc/lexer.baml:3`), matched on `Token.kind` (`lexer.baml:35`): `KwReturn` (:8), `KwInt` (:7), `KwIf` (:9), `KwElse` (:11), `KwWhile` (:10), `Ident` (:6), `Assign` (:18), `LParen` (:25), `RParen` (:26), `Semi` (:29).

**Classes constructed** (the `Stmt` members)
- `ReturnStmt { value: Expr }` — `ast.baml:39` (built line 153)
- `VarDecl { name, init }` — `ast.baml:47` (line 161)
- `IfStmt { cond, then_body, else_body }` — `ast.baml:52` (line 174)
- `WhileStmt { cond, body }` — `ast.baml:58` (line 181)
- `ExprStmt { expr }` — `ast.baml:43` (line 185)

**Callers** (change ripples outward): `parse_block` calls it in a loop (`parser.baml:141`); `parse_block` itself is reached from `parse_function` (:205) and the top-level `parse` (:209).

## What matters most before changing `parse_stmt`

1. **The `Stmt` union at `ast.baml:63` is the load-bearing contract.** All three code-gen backends pattern-match exhaustively on exactly these five variants — `arm.baml:192-220`, `x86.baml:151-180`, `gen.baml:71-80`. If you add a new statement form in `parse_stmt`, you must add its class, extend the `Stmt` union, and add a match arm in every backend, or code generation breaks. This is the highest-impact dependency.

2. **The `eat` helper (`parser.baml:16`) and the `throws CError` contract (`errors.baml:3`).** `parse_stmt` delegates all token-consumption-and-validation to `eat`, which mutates `self.pos` and throws on mismatch. Any new syntax you parse should consume terminators through `eat` (not bare `self.pos += 1`) so error reporting stays uniform, and the method must keep its `throws CError` marker for callers `parse_block`/`parse` to propagate.

3. **`TokKind` dispatch tags (`lexer.baml:3`).** The statement kind is selected purely by the leading `TokKind`. Any new statement keyword needs a corresponding `TokKind` variant plus lexer support; the fall-through at lines 183-185 treats anything else as an expression statement, so ordering and the terminal `Semi` matter.

Lower-risk: `parse_expr`/`parse_block` and the individual node fields — these are stable interfaces you consume rather than constrain.

## claude/guided-grep r2 — commands
```
Bash	ls && echo "---" && rg -n 'parse_stmt' -g '*.baml' -l
Bash	nl -ba baml_src/ns_cc/parser.baml | sed -n '1,60p'
Bash	grep -n 'parse_stmt\|function ' baml_src/ns_cc/parser.baml
Bash	nl -ba baml_src/ns_cc/parser.baml | sed -n '133,215p'
Bash	ls baml_src/ns_cc/ && echo "---defs---" && rg -n 'class Stmt|class CError|enum CError|type Stmt|enum TokKind|class ReturnStmt|class VarDecl|class IfStmt|class WhileStmt|class ExprStmt|class Token|class Expr|type Expr' baml_src/ns_cc/
Bash	nl -ba baml_src/ns_cc/ast.baml | sed -n '37,75p'; echo "---errors---"; nl -ba baml_src/ns_cc/errors.baml | sed -n '1,20p'; echo "---lexer---"; nl -ba baml_src/ns_cc/lexer.baml | sed -n '3,45p'
Bash	rg -n 'namespace' baml_src/ns_cc/parser.baml baml_src/ns_cc/ast.baml baml_src/ns_cc/lexer.baml baml_src/ns_cc/errors.baml baml.toml 2>/dev/null | head
```

## claude/guided-grep r2 — final answer

I now have the complete picture. Here's the dependency analysis for `root.cc.Parser.parse_stmt` (defined at `baml_src/ns_cc/parser.baml:147-186`).

## Contract dependencies (signature: return + error types)

`function parse_stmt(self) -> Stmt throws CError`

- **Return type `Stmt`** — a union type alias: `baml_src/ns_cc/ast.baml:63`
  `type Stmt = ReturnStmt | ExprStmt | VarDecl | IfStmt | WhileStmt;`
  Every branch of `parse_stmt` must produce one of these five members. Its variants are defined at:
  - `ReturnStmt` — `ast.baml:39`
  - `ExprStmt` — `ast.baml:43`
  - `VarDecl` — `ast.baml:47`
  - `IfStmt` — `ast.baml:52`
  - `WhileStmt` — `ast.baml:58`
- **Error type `CError`** — `baml_src/ns_cc/errors.baml:3` (`class CError { message: string }`). Raised transitively via `eat` and the `parse_*` calls, never directly in `parse_stmt` itself.

## Direct implementation dependencies

**Fields (via `self`)**
- `self.pos` — mutated with `self.pos += 1` in each branch (`parser.baml:150,156,164,171,177`). Declared `parser.baml:6`.
- `self.toks` — read indirectly through `cur()`. Declared `parser.baml:5`.

**Methods called**
- `self.cur()` → `Token` — `parser.baml:8`; used for the dispatch token at `147:148` and lookahead at `170`.
- `self.eat(TokKind)` → `Token throws CError` — `parser.baml:16`; consumes `Semi`, `Ident`, `Assign`, `LParen`, `RParen` (lines 152,157,158,160,165,167,184).
- `self.parse_expr()` → `Expr` — `parser.baml:133`; used for return values, initializers, conditions.
- `self.parse_block()` → `Stmt[]` — `parser.baml:137`; used for `if`/`else`/`while` bodies (168,172,181). Note `parse_block` itself calls `parse_stmt` (`parser.baml:141`) — a mutual recursion cycle.

**Enum variants used — `TokKind`** (`lexer.baml:3`)
- Dispatch keywords: `KwReturn` (149), `KwInt` (155), `KwIf` (163), `KwElse` (170), `KwWhile` (176)
- Punctuation consumed: `Ident` (157), `Assign` (158), `Semi` (152,160,184), `LParen` (165,178), `RParen` (167,179)

**Type used indirectly**
- `Token` (`lexer.baml:34`) — the shape returned by `cur`/`eat`; `parse_stmt` reads `.kind` and `.text` off it (148,157).

## What matters most before changing `parse_stmt`

1. **The `Stmt` union contract (`ast.baml:63`) is the tightest coupling.** Every branch must return a member of the union. Adding a new statement form (e.g. a `for` loop) means adding a class *and* extending the `Stmt` alias — otherwise the return type won't accept it. This is the change most likely to ripple into consumers (codegen backends, the type checker) that pattern-match on `Stmt`.

2. **`TokKind` variant names (`lexer.baml:3-32`) are the dispatch surface.** The `if (k == TokKind.X)` chain is a hard dependency on the enum. Renaming/removing a variant, or the lexer failing to emit it, silently drops that branch to the fallthrough `ExprStmt` (183-185) rather than erroring — a correctness trap.

3. **`eat`'s `throws CError` behavior (`parser.baml:16-23`) is your error contract.** `parse_stmt` delegates all syntax-error reporting to `eat`. If you add branches, keep using `eat` for mandatory tokens so errors stay typed and uniform.

4. **The `parse_stmt` ↔ `parse_block` recursion (`parser.baml:141`)** means block-bearing statements (`if`/`while`) already recurse. New nested-body constructs should route through `parse_block` for consistent `{ }` handling and EOF-guarded loops.

Lower-risk: the AST leaf classes (`ReturnStmt`, `VarDecl`, etc.) are simple field bags — changing their fields is a local edit as long as the construction sites in `parse_stmt` match.

## claude/guided-hybrid-slim r1 — commands
```
Bash	baml describe root.cc.Parser.parse_stmt --view dependencies --output compact --max-lines 80
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
```

## claude/guided-hybrid-slim r1 — final answer

## `root.cc.Parser.parse_stmt` — dependency analysis

Defined at `baml_src/ns_cc/parser.baml:147`, signature `parse_stmt(self) -> Stmt throws CError`. It dispatches on the current token to build one of five statement forms.

### Contract dependencies (the public shape)

| What | Where |
|---|---|
| Return type `Stmt` (union alias) | `baml_src/ns_cc/ast.baml:63` — `type Stmt = ReturnStmt \| ExprStmt \| VarDecl \| IfStmt \| WhileStmt` |
| Error type `CError` (`throws`) | `baml_src/ns_cc/errors.baml:3` |

Every branch returns one of the five union members (`parser.baml:153, 161, 174, 181, 185`) and every error path throws `CError` (indirectly, via `eat`).

### Direct implementation dependencies

**Shared cursor state / token access**
- `self.pos` field — `parser.baml:6` (advanced manually at `:150,156,164,171,177`)
- `self.cur()` — `parser.baml:8` (dispatch read at `:148`, else-branch checks)
- `Token.kind` / `Token.text` — `lexer.baml:35` / `lexer.baml:36`
- `self.eat(k)` — `parser.baml:16` (consume-or-throw; the source of `CError`)

**Delegated sub-parsers**
- `self.parse_expr()` -> `Expr throws CError` — `parser.baml:133`
- `self.parse_block()` -> `Stmt[] throws CError` — `parser.baml:137`

**Dispatch keys — `TokKind` enum (`lexer.baml:3`) variants**
`KwReturn` (`:8`), `Semi` (`:29`), `KwInt` (`:7`), `Ident` (`:6`), `Assign` (`:18`), `KwIf` (`:10`), `LParen` (`:25`), `RParen` (`:26`), `KwElse` (`:11`), `KwWhile` (`:12`).

**Constructed AST classes + fields**
`ReturnStmt.value` (`ast.baml:39`), `VarDecl.name/init` (`:47`), `IfStmt.cond/then_body/else_body` (`:52`), `WhileStmt.cond/body` (`:58`), `ExprStmt.expr` (`:43`).

### What matters most before changing `parse_stmt`

1. **The `Stmt` union (`ast.baml:63`) — highest coupling.** Adding or renaming a statement form is a two-place edit: the new class *and* the union alias. Miss the alias and the new branch won't satisfy the `-> Stmt` return type; every consumer of `Stmt` (codegen, type-checker) also widens.

2. **The cursor advance-invariant (`self.pos` + `cur`/`eat`).** The sole caller, `parse_block` (`parser.baml:137-145`), loops `while cur != RBrace/Eof { push(parse_stmt()) }`. It relies on each `parse_stmt` call consuming exactly one statement's tokens and making forward progress. A new branch that reads the dispatch token but forgets to advance `pos` (`:150,156,164,171,177`) will spin `parse_block` in an infinite loop. This is the easiest thing to break and the hardest to see in the type signature.

3. **The `throws CError` contract (`errors.baml:3`).** Error reporting flows exclusively through `eat` (`parser.baml:16`). If you add a new failure mode, throw `CError` for consistency — changing or dropping `throws` ripples up through `parse_block` → `parse_function` → `parse`, all of which are declared `throws CError`.

4. **`TokKind` variants must stay in sync with the lexer (`lexer.baml:3`).** The branch conditions are the dispatch contract with the lexer; a keyword the lexer doesn't emit (or vice versa) silently falls through to the `ExprStmt` default (`:183-185`).

5. **`parse_expr` / `parse_block` sub-contracts** are lower risk — as long as they keep returning `Expr` / `Stmt[]` and `throws CError`, `parse_stmt` is insulated from their internals.

Practical rule: statement-shape changes → touch #1 and #2 together; error/keyword changes → #3 and #4.

## claude/guided-hybrid-slim r2 — commands
```
Bash	baml describe root.cc.Parser.parse_stmt --view dependencies --output compact --max-lines 80
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/ast.baml
Bash	grep -n "parse_stmt" baml_src/ns_cc/parser.baml
Read	baml-c-compiler/baml_src/ns_cc/parser.baml
```

## claude/guided-hybrid-slim r2 — final answer

I now have the full picture. Here's the dependency analysis for `root.cc.Parser.parse_stmt` (`baml_src/ns_cc/parser.baml:147`).

## Contract dependencies (the method's signature surface)

`function parse_stmt(self) -> Stmt throws CError`

- **Return type `Stmt`** — `baml_src/ns_cc/ast.baml:63`. A union alias: `type Stmt = ReturnStmt | ExprStmt | VarDecl | IfStmt | WhileStmt`. Every `return`/tail expression in the body must produce one of these five variants.
- **Error type `CError`** — `baml_src/ns_cc/errors.baml:3`. Declared via `throws CError`; propagated implicitly from the `parse_expr`/`eat`/`parse_block` calls (the body never throws it directly).

## Implementation dependencies (what the body actually touches)

**Cursor/state primitives (used on every path):**
- `Parser.cur()` — `parser.baml:8` — reads current token (`parser.baml:148`, and the `else` lookahead at `:170`).
- `Parser.pos` — `parser.baml:6` — advanced with `self.pos += 1` to consume tokens.
- `Parser.eat(...)` — `parser.baml:16` — consumes+asserts a token kind; also returns the token (`.text` read at `:157`).
- `Token.kind` — `lexer.baml:35`, and `Token.text` — `lexer.baml:36`.

**Sub-parsers:**
- `Parser.parse_expr()` — `parser.baml:133` — used in all five statement forms.
- `Parser.parse_block()` — `parser.baml:137` — used for `if`/`else`/`while` bodies (`:168`, `:172`, `:181`).

**Dispatch enum + variants** (`TokKind` — `lexer.baml:3`): `KwReturn` (`:8`), `KwInt` (`:7`), `KwIf` (`:10`), `KwElse` (`:11`), `KwWhile` (`:12`), plus the tokens fed to `eat`: `Semi` (`:29`), `Ident` (`:6`), `Assign` (`:18`), `LParen` (`:25`), `RParen` (`:26`).

**Constructed AST classes (the `Stmt` variants):**
- `ReturnStmt{value}` — `ast.baml:39-40`
- `VarDecl{name, init}` — `ast.baml:47-49`
- `IfStmt{cond, then_body, else_body}` — `ast.baml:52-55`
- `WhileStmt{cond, body}` — `ast.baml:58-60`
- `ExprStmt{expr}` — `ast.baml:43-44` (the fallback tail at `:185`)

## Which dependencies matter most before changing `parse_stmt`

1. **The `Stmt` union (`ast.baml:63`) and its five classes.** This is the hardest constraint. Every branch must return exactly one variant with its exact fields. Adding a new statement form means editing the union *and* adding a class — a change here ripples to every consumer of `Stmt` (e.g. `parse_block` returning `Stmt[]`, and any type-checker/codegen walking statements). This is the single most impactful contract to get right.

2. **`TokKind` dispatch variants (`lexer.baml`).** The method is a keyword-dispatch chain (`:149/155/163/176`) with an unguarded `ExprStmt` fallback at `:183-185`. Order and completeness matter: because the fallback swallows anything unmatched, a mis-keyed or missing variant check silently becomes an expression statement rather than an error. New syntax requires a lexer variant *and* a branch here.

3. **`eat` / `pos` / `cur` cursor contract.** Correctness hinges on consuming exactly the right tokens — each branch's `self.pos += 1` and `eat(...)` calls must stay balanced with the grammar (note `:150` manually consumes the keyword, then relies on `eat` for delimiters). An off-by-one here corrupts parsing for all following statements. `eat` is also the implicit source of most `CError`s, so it ties directly to the error contract.

4. **`parse_expr` / `parse_block`** are stable recursion points — important to call correctly, but you rarely need to change them to change `parse_stmt`, so they're lower risk than the above.

`CError` itself is low-risk: it's only propagated, never constructed here, so changing `parse_stmt` won't affect it unless you add explicit `throw`s.

