# Semantic tokens — known gaps & next steps

Status snapshot of the semantic-tokens classifier (`tokens.rs` + `tokens/index.rs`
+ `tokens/classify.rs`). Architecture mirrors rust-analyzer: walk the lossless
CST for token positions + syntactic tokens (keywords/operators/punctuation),
resolve identifier *meaning* through the HIR/inference. Keywords are a parser
concern (contextual keywords like `with` are re-lexed to `KW_*`, read by kind).

## Scaling — "do what rust-analyzer does" (highest priority)

Today the classifier always does **whole-file** work: `index::build` resolves
every scope in the file regardless of what's requested. That's the scaling gap,
not the recursive-vs-iterative traversal style.

Levers, in order of impact:

1. **On-demand per-token resolution.** Rebuild a `resolve_token(file, token)`
   bridge on top of `baml_compiler2_tir::inference::scope_body` (still present)
   and have the walker resolve each name lazily — RA's `Semantics::resolve`
   model. Drops the prebuilt whole-file index so a range request only pays for
   visible tokens. (We built `resolve_token` earlier this effort, then removed
   it once the index covered the cases — bring it back for scale.)
2. **Range / viewport highlighting** (`semanticTokens/range`). Take a range,
   skip subtrees whose `text_range()` doesn't intersect it, wire the request in
   `bex_project`. Works in the current recursive shape (early `return` on
   out-of-range nodes). Biggest single win on large files.
3. **Delta encoding** (`semanticTokens/full/delta`) — emit only the token diff.
4. **(Optional) iterative traversal shape.** Replace the recursive `node()`
   dispatch with one `preorder_with_tokens()` loop + match-on-kind/parent, like
   RA's `traverse`. Same asymptotics; only buys stack-depth robustness and
   convergence with RA. Do last, or never.

Already have: salsa-cached query, lossless rowan CST, typed AST accessors,
`scope_body` (uniform scope→body lookup), per-scope inference.

## Remaining feature items (from the highlighting punch-list)

- **Optional function parameters at call site** — not yet investigated.
- **Boolean literals** — `true`/`false` are currently `keyword`. RA uses a
  custom `boolean` semantic-token type (beyond the standard legend); add it to
  the legend + enum + paint.rs + viewer color map and classify there.

## Known resolution gaps / limitations

- **Test / testset bodies.** `testset` is not lowered into the compiler2 IR
  (absent from tir/hir/ppir); body code is classified only insofar as the scope
  iteration reaches it. Verify coverage; may need the testset lowering wired up.
- **Interface-member go-to-def** navigates to the *interface declaration*, not
  the specific member span. `MemberResolution::InterfaceMethod`/`InterfaceField`
  carry only `iface_loc` + name (no `func_loc` for required methods). Refine to
  the member span when desired.
- **Generic param *usages*** (the `T` in `v: T`, `T[]`) classify as `type`
  (fallback), while *declarations* (`class Box<T>`) are `typeParameter`. To make
  usages `typeParameter`, resolve the name to the in-scope generic param.
- **Backtick-string escapes** are not split into `escapeSequence` tokens (only
  regular + byte strings are).
- **Generator bodies** are parsed opaquely (deprecated); classified by shape
  (name→struct, key→property, value tokens→string), not via real structure.

## Principled fixes landed this effort (context)

- Interface members now resolve like class members: `resolve_interface_member`
  records `MemberResolution::InterfaceMethod`/`InterfaceField`
  (`builder.rs`), so casts / `Self` methods / chained interface calls classify
  through the normal path; typos stay neutral. MIR-safe (new variants → `None`
  item-ref, fall through to existing interface dispatch).
- `scope_body` (tir): uniform scope→`(ExprBody, source_map)` covering function /
  let / lambda (and spawn/block bodies that lower to closures) — the index walks
  every inference-bearing scope, not just top-level functions.
