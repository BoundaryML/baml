# Post-merge compiler-bug status

Empirical re-test of every documented compiler bug against the **merged** compiler
(`c50bf98f4` = `origin/canary`'s hir_ty type-inference foundations `a0f4605e8` merged
over the `0dd1cff77` stdlib migration), using the freshly built
`target/debug/baml-cli`. No `cargo` was used; probe projects live under
`/tmp/bamlprobe/p1..p9`.

Date: 2026-08-14.

## Verdict table

| # | Bug | Verdict | Observed behavior |
| --- | --- | --- | --- |
| 1 | `x = <null-expr> ?? x` self-assignment loses the fallback | **FIXED** | `lower_null_coalesce` no longer pre-writes `dest` before the branch — it now writes `dest` on exactly one arm (`lower_if` shape, new `bb_lhs` block), so a RHS that reads the assignment target sees the unclobbered value. Regression ns `ns_null_coalesce_assign` (8 tests); stdlib workaround `_anth_keep_int` deleted, `llm_anthropic` 33/33. |
| 2 | Parser E0010: string literal containing `client`/`prompt` misclassifies the function as LLM | **FIXED** (was STILL-BROKEN) | `function f() -> string { "the client sent nothing" }` → 5× E0010. Also `"the prompt was empty"`, `` `the client sent nothing` `` (backtick), `"client"`. Not triggered: `"the tools list"`, `"client = x"`, `"a client."`, `"clients here"`. — **FIXED**: `looks_like_llm_function_body_from` now steps over string literals via a new `skip_string_literal_from` (quoted/backtick/raw/byte, `${…}` interpolations and nested literals included), so string contents are never read as syntax and a literal's braces no longer skew `brace_depth`; `client`/`prompt` stay triggers and real LLM bodies still classify. Regression coverage: `crates/baml_tests/baml_src/ns_llm_classifier_strings/` + parser unit tests `string_contents_never_classify_a_body_as_llm` / `real_llm_bodies_still_classify_with_string_aware_scan`. |
| 3 | `baml.json.from_json<union-of-media>` ignores the BEP-038 `kind` | **FIXED** | `deserialize_media` (`crates/bex_vm/src/package_baml/json.rs`) now rejects an envelope whose present `kind` tag disagrees with the target media kind, so the first-match-wins union loop discriminates; `kind`-less envelopes still decode for any media target. Regression ns `ns_media_json_union`; `ai.content.Media.new`'s manual `kind` dispatch simplified to a direct `from_json<root.MediaPart>` (moves the 3 `__ai_std__` ppir/mir/codegen snaps). |
| 4 | MIR panic `type variable not found in type args: E` for a root-namespace generic-over-closure fn called from a test | **NOT REPRODUCIBLE** (presumed FIXED-UPSTREAM) | Five variants of the `mock_json_serve<T, E>` shape declared at root namespace — called from a function, called directly in a `test`, closure annotated / unannotated, throwing closure (non-`never` `E`), nested generic in the closure body, all with the real `spawn`/`defer`/`baml.http.Server` machinery — 5/5 PASS. |
| 5 | Throws-analysis phantom `unknown` | **FIXED-UPSTREAM** | (a) Repro B now compiles clean; forcing a type mismatch on the binder shows `ai.wire.merge_request_body`'s catch binder is `never` (assigns to anything) and `aws.internal.build_request`'s is **exactly the 7 declared members**, no `unknown`. (b) Repro A still PASSES — the ICE has not returned. |
| 6 | `v is type` constant-false | **FIXED** | `let v: type \| int = reflect.type_of<int>(); v is type` → `false`. Also false via `if let t: type = v`, and via `let v: type?`. In `match (v) { let t: type => …, let i: int => … }` the value falls into the **`int`** arm. Contrast: `v is image` and `v is unknown` now work (their arms were added). `v is int` is correct in both directions. — **FIXED**: `realized_type_tag` (`crates/baml_compiler2_emit/src/emit.rs`) now maps `RealizedTy::Type` to `typetag::TYPE`, the same tag the MIR switch path already used, so `is`/`if let`/`match` all narrow `type`; regression ns `crates/baml_tests/baml_src/ns_type_value_narrowing/` (20 tests). `RustType`/`Resource`/`PromptAst` stay tagless on purpose — opaque native handles with no reconstructible runtime type (documented at the `_ => None` arm). |
| 7 | `catch (e)` with multiple declared throw types does not narrow `e` in typed arms | **NOT REPRODUCIBLE — behaves as designed** | Typed `catch` arms narrow correctly over a 2-throw and a 3-throw callee, including arms that read **different** fields per error class (`ErrA.code` / `ErrB.label` / `ErrC.flag`), a partial `catch` chained into `catch_all`, and a 7-throw stdlib callee (`aws.internal.build_request`) with `baml.errors.Io` / `baml.errors.Timeout` / `ai.errors.InvalidRequest` arms. 8/8 tests PASS. Matches `baml describe catch`: "binds the caught error to `e` and dispatches on its type". |
| 8 | Property-shorthand rewrite misses `if let` binders | **FIXED** (2026-08-14) | `infer.rs`'s `Expr::Map` arm now reads the parser's shorthand marker (`AstSourceMap::property_shorthand_exprs`) instead of re-deriving it from key text, and tests scope through the semantic index's `path_resolution` instead of a body-scope-only name list — so every binder form and nesting depth resolves, and a written `{ "key": key }` reports the generic unresolved-name diagnostic. Regressions: `ns_property_shorthand_binders` (20/20) + 3 `phase3a` unit tests; `vertex.baml` workaround removed, `llm_google` 30/30. Previously: fails for **every pattern binder**, not just `if let`: `if let`, `match`-arm `let`, `for (let x in …)`, `catch`/`catch_all` binder, and class-destructure `Box { key: let key }`. Also fires on **genuine** unquoted shorthand `{ key }` under `if let`. Does *not* fire for function params, plain `let`, or closure params. |
| 9 | `function` as a field name is E0010 | **STILL-BROKEN, and `@alias` is not a workaround** | `function: string,` → E0010. `"function": string,` → 4 errors; `r#function` → 5 errors. `fn_call: string @alias("function")` compiles, **but `@alias` does not rename the `baml.json` wire key**: `baml.json.to_string` emits `{"id":"1","fn_call":"get_weather"}` and `from_string` on `{"function":…}` raises `JsonDecodeError: missing required field 'fn_call'`. `@alias` is LLM-schema-only. |

Net: **6 still broken** (1, 2, 3, 6, 8, 9 — of which 9 is a language limitation
rather than a miscompile), **1 fixed upstream** (5), **2 not reproducible**
(4, 7). Bug 8 is strictly worse than filed; nothing regressed relative to the
pre-merge state.

---

## What's missing — analyses for the STILL-BROKEN items

### 1. `??` writes the destination before evaluating its right operand

`crates/baml_compiler2_mir/src/lower.rs:6534` `lower_null_coalesce` is a
**destination-threaded** lowering that writes `dest` *eagerly*:

```rust
let lhs_op = self.lower_to_operand(lhs);
self.builder.assign(dest.clone(), Rvalue::Use(lhs_op.clone()));   // <-- dest := lhs
// … test lhs == null, branch …
self.builder.set_current_block(bb_rhs);
self.lower_expr(rhs, dest);                                        // <-- rhs read into the SAME dest
```

`AstStmt::Assign` (`lower.rs:11350`) hands the target place straight through as
that destination:

```rust
let place = self.lower_lvalue(target);
self.lower_expr(value, place);
```

So for `a = n ?? a`, `dest` **aliases** `a`. Step 1 stores `n` (null) into `a`;
step 3 then lowers the right operand, which reads the now-clobbered `a`. The
result is null — literally the reported "writes null before evaluating the RHS".
The missing piece is aliasing awareness at exactly one of two seams: either
`lower_null_coalesce` must land the left operand in a fresh temp and only copy
into `dest` on the join block, or `AstStmt::Assign` must route through a temp
whenever the value expression mentions the target place. Note `if`/`match` do not
pre-write `dest`, which is why `a = if (n == null) { a } else { n }` and the
`match` spelling are both correct — this is specific to `??`, not a general
destination-threading defect. The live workaround
`_anth_keep_int(next, previous)` in
`crates/baml_builtins2/baml_std/anthropic/ns_internal/messages.baml:964` (three
call sites) is still required; it works because passing through a function call
forces the right operand into an argument slot that does not alias the target.

**FIXED** (2026-08-14): `lower_null_coalesce` now branches first and assigns
`dest` on each arm (new `bb_lhs` block, same shape as `lower_if`); the
`_anth_keep_int` workaround is deleted. MIR/codegen insta snapshots shift by the
added block — regenerate.

### 2. The LLM-function classifier scans raw tokens with no string awareness

`crates/baml_compiler_parser/src/parser.rs:4058`
`looks_like_llm_function_body_from` decides LLM-vs-expression body by walking the
raw token stream and returning `true` on a `TokenKind::Word` with text `client`
or `prompt` at `brace_depth == 1`, unless the *next* token is `= , ) . (`. String
literals are **not** skipped — the contents of `"…"` and `` `…` `` arrive as
ordinary `Word` tokens in this scan. The code already knows this; the comment at
line 4083 says so explicitly and is the stated reason `tools` was removed as a
trigger ("raw string contents lex as ordinary tokens in this scan, so a body
containing e.g. `tools/list` would misclassify") — but `client` and `prompt` were
left in place with the same hazard. This exactly predicts the observed matrix:
`"clients here"` is safe (text equality, `clients != client`), `"a client."` and
`"client = x"` are safe (the guard's next-token escape hatch fires), and every
other phrase is misclassified. The missing piece is string/template-literal span
skipping in the scanner — the guard is a next-token heuristic where a
lexical-context test is required. Since the scan already has the token stream, it
needs to advance past `STRING_LITERAL` / `RAW_STRING_LITERAL` spans (and the
interpolation-free interior of backtick templates) rather than treating their
words as candidate field names.

### 3. `deserialize_media` never reads the envelope's `kind`

`crates/bex_vm/src/package_baml/json.rs:1421` `deserialize_media` takes `kind`
as a **parameter derived from the static target type** and reads only `source`,
`value`, and `mime` out of the JSON object. It never compares `map.get("kind")`
against `kind.tag_str()` — even though the serializer three hundred lines up
(`serialize_media`, line 1049) writes exactly that tag. Combined with the union
arm at line 1243:

```rust
RealizedTy::Union(members, _) => {
    // Try each member structurally; first match wins.
    for member in members { if let Ok(v) = ty_serde_to_value(…, member, …) { return Ok(v); } }
```

the decode of `{"kind":"audio","source":"url",…}` against `Image | Audio` tries
`Image` first, and `deserialize_media(kind = Image)` *succeeds* because nothing
in it can reject an audio envelope. Hence the reversed-order probe flipping the
answer, and hence the mis-decoded value carrying `mime=audio/mpeg` inside an
`Image`. This is not affected by the `TyTemplate::Media` narrowing fix (that
fixed `is`/`match` in `emit.rs`, a different layer) nor by hir_ty (the target type
is correct; the *runtime decoder* is what discards the discriminant). The missing
piece is one guard in `deserialize_media`: reject when the envelope's `kind`
string is present and does not equal `kind.tag_str()`. That single check makes
the existing first-match-wins loop discriminate correctly for every media union
including `ai.MediaPart`, with no change to the union arm.

**FIXED** as analysed — the guard went into `deserialize_media`, before the
`source`/`value` reads. Decisions pinned by `ns_media_json_union`: the guard is
*not* union-only (a direct `from_json<image>` of an audio envelope now throws
`JsonDecodeError: media kind mismatch` rather than silently producing an
`Image` with an audio payload); an envelope with **no** `kind` still decodes for
any media target, so hand-built `{source, value, mime}` objects keep working
and continue to select a media union's first member; and `Generic`
(tag `"media"`) discriminates nothing, so it is accepted in either position.
`ai.content.Media.new`'s manual `baml.json.field(envelope, "kind")` dispatch was
the workaround for this bug and is now a plain
`baml.json.from_json<root.MediaPart>(envelope)`; that stdlib edit moves the
`__ai_std__` `03_ppir` / `04_5_mir` / `06_codegen` insta snapshots (not updated
here — left for the final sweep).

### 6. `RealizedTy::Type` has no arm in the `is`-lowering dispatch

`crates/baml_compiler2_emit/src/emit.rs:3363`'s `is_type` match has purpose-built
arms for containers/interfaces/unions (`emit_structural`), for function
signatures, for `TyTemplate::Media(..)` (added by the media fix, with a comment
describing precisely this failure mode), and for `TyTemplate::BuiltinUnknown`
(`emit_true`). Everything else falls to `other =>`, which narrows to a
`RealizedTy` and asks `realized_type_tag` (line 3638) for a coarse tag —
`emit_false` when there is none. `realized_type_tag` enumerates
Int/Bigint/String/Bool/Null/Float/Enum/List/Map/Function/Uint8Array/Literal and
ends `_ => None`. `RealizedTy::Type` is a `#[axis(concrete)]` leaf
(`crates/baml_type/src/family.rs:209`) with **no arm**, so `v is type` compiles to
constant-false — S2's analysis is confirmed verbatim, and it is the same one-arm
omission the Media fix repaired. The tag constant already exists:
`baml_type::typetag::TYPE = 10` (`crates/baml_type/src/typetag.rs:67`), so the fix
is a one-line `RealizedTy::Type { .. } => Some(baml_type::typetag::TYPE)` provided
the VM stamps that tag on reflection values; otherwise the Media route
(`TyTemplate::Type { .. } => emit_structural(self, ty_template)`) applies. The
sibling opaque leaves `RustType`, `Resource`, and `PromptAst` are in the identical
position with **no** tag constant at all, so they need the `emit_structural` route.
The `match` mis-routing is a direct consequence and not a second bug: the `type`
arm's test compiled to constant-false, and the final `let i: int` arm is exhaustive
so its test is elided — the last arm swallows the value, exactly as the
`BuiltinUnknown` comment warns.

### 8. The shorthand check re-derives shorthand-ness textually and cannot see pattern binders

Two independent defects compound, both in
`crates/baml_compiler2_hir_ty/src/infer.rs`'s new `Expr::Map` arm (line 2003).

First, it does **not** consult the authoritative flag. AST lowering records
genuine shorthand in `source_map.property_shorthand_exprs`
(`crates/baml_compiler2_ast/src/lower_expr_body.rs:4709` and `:4815`, both gated
on `!seen_colon`), and `AstSourceMap::is_property_shorthand_expr` exists to read
it. The hir_ty check ignores that and re-derives the property *textually* — "key
is a string literal whose text equals a single-segment value path" — which is true
for `{ "key": key }`, an explicitly quoted key that merely coincides with the
value's name. That is why quoting does not escape the diagnostic.

Second, its scope query is incomplete. It accepts the name if
`self.lower.resolve_value(segments)` (the global resolver) or
`self.local_binding_names()` finds it. `local_binding_names`
(`infer.rs:8348`) walks `index.ancestor_scopes` collecting only
`scope_bindings[..].bindings` and `.params` — i.e. `let`-statement binders and
function/closure parameters. **Every pattern binder is invisible to it**: `if let`,
`match`-arm `let`, `for (let x in …)`, `catch`/`catch_all`, and class-destructure
binders, all confirmed failing. This is the deeper of the two defects — it makes
even *genuine* unquoted shorthand `{ key }` under `if let` a hard error, which
quoting cannot work around. The minimal fix is to gate the whole block on
`source_map.is_property_shorthand_expr(*value)` (killing the quoted-key
false-positive outright) and to extend `local_binding_names` to include pattern
binders from the scope index. Note the underlying expression types fine either
way — this is a spurious hard error on valid code, not a mistyping.

### 9. `function` is a keyword and `@alias` does not reach the JSON codec

Two separate gaps. The parser reserves `function` as `TokenKind::Function`, so a
class member beginning with it is committed to the method-declaration grammar
before any field interpretation is possible — hence `expected 'function name',
found ':'`. There is no escape hatch: quoting (`"function": string`) and raw
identifiers (`r#function`) both fail worse than the bare form. The natural
workaround, `fn_call: string @alias("function")`, *compiles* but does not solve the
tool_calls wire-shape problem, because `@alias` is consumed only by the LLM
schema/`ctx.output_format` path and is not consulted by `baml.json`
serialization or deserialization — verified in both directions
(`to_string` emits `fn_call`; `from_string` on a `"function"` key raises
`JsonDecodeError: missing required field 'fn_call'`). So a wire shape with a
`function` key currently requires hand-building the `json` value (or a
`map<string, …>`) on both sides. Fixing this properly means either admitting
contextual keywords as field names in the class-member grammar, or wiring `@alias`
through `bex_vm/src/package_baml/json.rs`'s class serialize/deserialize paths —
the latter is the smaller change and independently useful.

---

## Repro locations

- Bug 5 repro A / B: `_plan/sys_llm_native/compiler-bugs/runtime-ty-unknown/baml_src/main.baml`
  (repro A still passes; repro B's commented block now compiles clean and the
  README's "Still open" section can be closed).
- Bug 8: `_plan/sys_llm_native/compiler-bugs/shorthand-if-let/repro.baml`
  (the README understates the scope — see analysis 8).
- Bugs 1–4, 6–9 probe projects: `/tmp/bamlprobe/p1` … `/tmp/bamlprobe/p9`
  (each a `baml.toml` + `baml_src/`, run with
  `baml-cli test --from /tmp/bamlprobe/pN/baml_src`).
