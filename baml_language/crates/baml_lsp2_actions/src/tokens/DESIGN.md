# Principled rust-analyzer-style semantic tokens — design & plan

Target: the rust-analyzer highlighting architecture, faithfully, with the
prerequisite parser/AST changes it requires. No shortcuts. The 78 fixtures are
the behavior-preservation oracle at every step.

## RA architecture (what we mirror)

- `highlight(db, cfg, file, range: Option<TextRange>) -> Vec<HlRange>`. For a
  range, root = `covering_element(range)`; else the whole file.
- `traverse`: one `preorder_with_tokens()` `WalkEvent::{Enter,Leave}` loop with a
  small enter/leave **state stack**; per element, `range.intersect(elem).is_none()
  -> continue`. Tokens classify **syntactically** (`highlight::token`: literals /
  comments / punctuation(kind,parent_kind) / keyword(kind)). `ast::NameLike` nodes
  classify **semantically** (`name_like -> NameClass/NameRefClass::classify ->
  highlight_def`), then `skip_subtree()` so the inner ident token isn't re-done.
- Name bridge: `IdentClass::classify_token(token) = token.parent()` -> match
  `ast::Name` (def site) | `ast::NameRef` (ref site). NameRef spine: `NameRef ->
  PathSegment -> Path -> sema.resolve_path -> PathResolution -> Definition`.
  `highlight_def(Definition) -> HlTag::Symbol(SymbolKind) + mods`.
- On-demand + memoized: each classify routes through `Semantics::analyze` (a
  per-body `SourceAnalyzer`, source-to-def cache) and salsa-memoized
  `InferenceResult::of(body)`. First token in a body pays inference; rest reuse.
- LSP: `full` (compute + cache by uri + result_id), `full/delta` (compute full +
  diff token arrays), `range` (separate `highlight_range(frange)`).

## Our analog (what we already have / map to)

- Lossless CST (rowan) + `baml_compiler_syntax::ast` typed accessors = RA syntax
  tree + `ast::*`. Event-buffer parser with `with_node`/`wrap_events_in_node`.
- `Semantics`/`SourceAnalyzer`/`InferenceResult::of` analog: `resolve_name_at`
  (offset name res), `scope_body` (uniform scope->(ExprBody,source_map)),
  `infer_scope_types` (salsa-memoized **per ScopeId** — our body-granularity
  memo seam). No macros -> drop `descend_into_macros`/`Ranker` entirely.
- `Definition`/`highlight_def` = our `Resolution` + `classify.rs`. We do NOT copy
  RA's 24-variant Definition; ours is small (class/enum/interface/function/method/
  field/variant/param/local/type-alias/assoc-type/client/test/...).

## Prerequisite parser / AST changes (the foundation — do first, principled)

These remove the corners the current classifier cuts (text-matching literals/
keywords, positional structure recovery).

**P1 — Real literal & contextual-keyword tokens (parser/lexer/SyntaxKind).**
- Add `KW_TRUE`, `KW_FALSE`, `KW_NULL` token kinds. `true`/`false`/`null` must be
  syntactic, not text-matched (today bumped as WORD; recovered via `text==..` in
  ast.rs:711/3906, parser.rs:6190/6024/7437). Mechanism: lexer keywords if BAML
  reserves them, else uniform contextual remap at every literal site. Add to
  `is_literal` (true/false/null) — they classify as `boolean`/`keyword`.
- Add a custom `boolean` semantic-token type (RA's `BoolLiteral`); `true`/`false`
  -> `boolean`, `null` -> `keyword`/builtin. Wire legend + enum + paint.rs + viewer.
- Uniformly remap the remaining contextual keywords: `KW_AS` (`.as<T>` cast,
  `(T as I)`, `field as field`), `KW_TYPE` (assoc-type / type-alias) — today
  bumped as plain WORD and re-detected by `text=="as"`/`"type"`. After remap,
  `is_keyword` sees them and resolution stops string-matching.
- Reconcile `is_operator` (drop phantom `QUESTION_QUESTION`, add real compound
  assigns / single `QUESTION` / etc. that the parser actually emits) and add
  `HEADER_COMMENT` to `is_trivia`.

**P2 — Typed AST union enums + wrappers (baml_compiler_syntax/src/ast.rs,
accessor-only; CST already supports unless noted).** The classifier must read
structure off `ast::*`, not positionally.
- `ast::Expr` union enum (the core RA pattern) + `ast::Pattern`, `ast::Type`
  unions so child-expr accessors return typed nodes (today `FieldAccessExpr::base`,
  control-flow accessors return raw `SyntaxNode`).
- Wrappers + accessors: `ObjectLiteral` (`type_name()`/`path()` + `fields()`),
  `ObjectField` (`key()`/`value()`), `CallExpr` (`callee()`+`args()`) + `CallArgs`,
  `BinaryExpr` (`lhs()`/`op_token()`/`rhs()`), `SpawnExpr` (`name_expr()`/`body()`),
  `AwaitExpr`, give `UpcastExpr` real `base()`/`target_type()`, `GenericArgs`/
  `TypeArgs` (`args()`/`named_bindings()`), `TypeExpr::path_segments()` (mirror of
  `PathExpr::segments`), `GeneratorDef`/`TestsetDef`/`TestExprDef` (`name()`) +
  extend the `Item` enum, pattern wrappers (BINDING/DESTRUCTURE/FIELD/ARRAY/TYPE/
  WILDCARD/UNION/CHAIN). Lower-pri: Array/Map/Index/Lambda/Paren/Tagged/Is/Unary.
- Resolve the object-literal-generic-head mismatch: `Foo<int> {}` head is emitted
  as `PATH_EXPR` not `TYPE_ARGS` — normalize in parser or expose via ObjectLiteral.
- Def-vs-ref: our CST has no `ast::Name`/`ast::NameRef` split; classify by the
  identifier token's **parent node kind** (decl node -> def site; path/member/type
  -> ref site). No grammar change needed for this.
- (Lower-pri) `parse_generator` opaque body leaves interior strings un-noded.

## P1 finding (2026-06): token-kind remap has a broad blast radius

Re-lexing `true`/`false`/`null` from `WORD` to `KW_TRUE`/`KW_FALSE`/`KW_NULL`
(the principled form) compiles + passes the compiler logic tests, but ripples
into **every consumer keyed on the token kind**: hover (`on_hover` iterator
inference tests changed — hover resolves off the token), CST/parser/formatter
snapshots (`baml_tests` config_dictionary/json_map_literal), etc. So the
remap is NOT highlighter-local; it requires mapping and updating hover /
completions / definition / formatter / lowering and regenerating their
snapshots. **Deferred** until that full consumer map is done (a fan-out search).
For now the `boolean` token type is delivered via a transitional text match in
`tokens.rs::token` (true/false -> `boolean`, null -> `keyword`); the inert
`KW_*` kinds + `is_ident_token`/`is_keyword` additions are already in place,
ready for the proper remap. `as`/`type` highlighting already works via the
existing context handlers, so the remap is a *mechanism* upgrade, not a feature.

**`as`/`type` remap also confirmed broad (attempted + reverted):** remapping the
5 `as`/`type` bump sites to `KW_AS`/`KW_TYPE` (and making `classify_token` read
the kinds) compiled and kept the 78 fixtures green, but broke compiler lowering
— `baml_compiler2_ast` `function_type_throws_preserves_omission_vs_explicit_never`
started emitting a spurious "type alias" diagnostic (the type-alias path keys on
the `type` token). So BOTH parser-token remaps (true/false/null AND as/type) are
deferred on the same evidence-based cost/benefit: they ripple into hover /
lowering / formatter / CST snapshots for a highlighter-purity-only gain, while
`classify_token`'s contextual disambiguation (the `as`/`type`/`true`/`false`
checks are scoped to a known parent node, not free substring scanning) is correct
and reasonable. Doing the remap properly means mapping + updating every
token-kind consumer first (a dedicated fan-out), not a highlighter-local change.

## Revised sequencing: architecture first, parser-token remap as a mapped follow-up

The RA *system* (Phases B/C/D) is LSP-only — no parser change, no broad
fallout — and is the high-value core. Do it first; do the broad parser-token
remap (P1 full) afterward as a carefully-mapped change.

## Status (2026-06)

DELIVERED + committed + validated (PR #3867):
- **B (on-demand resolution):** `scope_resolution_index` — per-`ScopeId`
  salsa-cached resolution index (RA body-granularity memo; editing one scope
  invalidates only its index). `build` merges them for a full document;
  `resolve_token_class` resolves one name on demand (RA `Semantics::resolve`),
  walking the scope chain. The `Walk` is parameterized by a `resolve` closure.
- **D (scaling):** `semantic_tokens_in_range` (RA `highlight_range`) — range-gated
  walk, resolves only the scopes the viewport touches; proven equal to
  full-filtered-to-range across every sub-range (`range_tokens_test`). LSP:
  `semanticTokens/range` + `full/delta` advertised + wired (per-file token cache,
  monotonic `result_id`, prefix/suffix token-granularity diff; diff unit-tested).
- **A1 (boolean):** `true`/`false` -> `boolean` token type; inert KW_* token
  foundation laid.

IN PROGRESS:
- **C (flat preorder traversal) + A2 (typed accessors):** rewriting the recursive
  `Walk` into one `preorder_with_tokens()` loop + `classify_token` parent-kind
  dispatch reading typed `ast::*` accessors. Behavior-preserving (78 fixtures +
  range test are the oracle).

DEFERRED (documented cost/benefit):
- **P1 broad remap (true/false/null -> KW_*):** ripples into hover/formatter/CST
  snapshots for a highlighter-purity gain; the boolean output already ships via a
  transitional text match. The contained `as`/`type` -> KW_AS/KW_TYPE remap is
  worthwhile and smaller-radius; do it once C lands (it changes how `classify_token`
  reads `as`/`type`).

## Phases (each ends green: 78 fixtures unchanged unless the change is the point)

- **A. Parser/AST prerequisites (P1 then P2).** Land literal/keyword tokens +
  `is_*` fixes (update walker to read new kinds; output preserved except
  true/false -> `boolean`). Then land typed-AST union enums + wrappers (additive;
  no classifier change yet).
- **B. `Semantics` bridge (`resolve_token`).** token -> parent-kind dispatch ->
  Resolution (root via `resolve_name_at`; member via `scope_body`+inference;
  qualified via `resolve_path_at`; type via type res) -> classify. On-demand,
  reusing `infer_scope_types` memo. Validate equals the current index per-token.
- **C. Flat traversal.** Replace recursive `node()`/handlers with one
  `preorder_with_tokens()` loop: syntactic token classify + `resolve_token` for
  names, reading context off the new typed-AST accessors; range-gate ready. Drop
  `index::build`. Validate fixtures unchanged.
- **D. LSP scaling.** Range (`covering_element` + range-gated walk; advertise +
  handler). Delta (stable `result_id` + per-uri encoded-token cache + prefix/
  suffix diff; advertise `Delta{delta:true}`). Unify position encoding.

## Validation discipline
Every slice: `cargo check` the touched crates; run the 78 `semantic_tokens`
fixtures WITHOUT `UPDATE_EXPECT` (must stay green, except the deliberate
true/false->boolean change); run tir/mir + runtime interface tests after any
parser/inference change; clippy clean. Adversarial review at each phase boundary.
