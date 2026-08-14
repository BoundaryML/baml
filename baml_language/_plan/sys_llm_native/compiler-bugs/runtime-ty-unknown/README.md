# `Unknown` is not a valid `RuntimeTy` — ICE on `catch` + `.to_string()`

```
internal error: entered unreachable code:
`Unknown` is not a valid `RuntimeTy`: an error-recovery type reached runtime lowering
  crates/baml_type/src/runtime_ty.rs:237  (ResolvedAliases::convert)
  <- baml_compiler2_mir::lower::lower_tir_template          (lower.rs:395, union arm)
  <- baml_compiler2_mir::lower::tir2_to_template
  <- LoweringContext::ty_to_template
  <- LoweringContext::emit_frame_type_arg_ops
  <- LoweringContext::try_lower_to_string_fallback
  <- LoweringContext::lower_call <- lower_catch <- lower_function_body
```

Status: **FIXED**. Discovered via `crates/baml_tests/baml_src/ns_llm_bedrock/bedrock.baml`,
which panicked three parallel MIR-lowering workers — one per `catch_all (e) { _ => e.to_string() }`
site in that file. It reproduced from **any** entry point that compiles the corpus
(`baml-cli check`, `baml-cli test`, and the `baml_src.rs` harness's `bytecode` test via
`compile_multi_file`); the earlier belief that only the Rust harness was affected was a
warm-bytecode-cache artifact, not an `emit_test_cases` / `OptLevel` difference.

## Root cause

Two independent defects had to line up.

### 1. TIR: the catch-binder walker charged a bogus `Unknown` throw fact

A `catch`/`catch_all` binder's type is the union of the base expression's throw facts,
computed by `TypeInferenceBuilder::collect_throw_facts_from_expr`
(`crates/baml_compiler2_tir/src/builder.rs`). Its `Expr::Call` arm was a **partial copy** of
`throws_analysis::collect_from_expr`'s `Call` arm: it called
`collect_callee_escaping_throws` directly but omitted that function's three sugar-fallback
guards.

`recv.to_string()`, `recv.to_json()` and `Type.from_json(j)` never resolve to a real method —
the type checker deliberately leaves their callee `Unknown` and MIR rewrites them into a
concrete stdlib call (`string.from` / `baml.json.from` / `baml.json.to`). Routing them
through `collect_callee_escaping_throws` therefore reached its documented
*unaccounted-callee* default, which charges a `Ty::Unknown` fact.

So `f().to_string() catch_all (e) { ... }` typed `e` as `<real throws> | Unknown` — an
error-recovery sentinel in a value position, with **no diagnostic**, which is why
`baml-cli check` reported the corpus clean right up until MIR lowering panicked.

### 2. MIR: the sugar-fallback guards only rejected a *top-level* sentinel

`try_lower_to_string_fallback` (and its `to_json` / `from_json` twins) pass the receiver's
static type as a leading type arg so the shim's `T` binds under monomorphization. The guard
read:

```rust
if !matches!(t, Tir2Ty::Unknown { .. }) && !contains_typevar_where(t, ...)
```

`matches!` is a top-level test, but `contains_typevar_where` is recursive — an asymmetry.
A sentinel nested inside a union (exactly what defect 1 produces) sailed past it into
`ty_to_template`, which panics rather than degrading.

This second defect is the one that actually ICEs, and it is reachable **independently of
defect 1**: an unaccounted callee contributes a `Ty::Unknown` throw fact *by design*, so a
catch binder typed `SomeError | Unknown` is a legitimate, still-occurring TIR output (see
"Still open" below). MIR must tolerate it.

## The fix

Three files, ~128 insertions.

| File | Change |
| --- | --- |
| `crates/baml_compiler2_tir/src/throws_analysis.rs` | Extracted the three sugar-fallback special cases into a shared `pub(crate) fn sugar_fallback_call_throws(...) -> Option<BTreeSet<Ty>>`; `collect_from_expr` now consults it instead of inlining them. |
| `crates/baml_compiler2_tir/src/builder.rs` | `collect_throw_facts_from_expr`'s `Call` arm consults the same helper before `collect_callee_escaping_throws`, so the two walkers can no longer drift. |
| `crates/baml_compiler2_mir/src/lower.rs` | All three sugar-fallback lowerings now test `generics::contains_error_recovery(t)` (recursive) instead of a top-level `matches!`. |

### Why MIR *erases* rather than drops the type arg

The obvious MIR fix — extend the guard so a sentinel-bearing receiver falls to the existing
`_ => Vec::new()` arm (ntypeargs = 0) — is wrong, and trying it surfaced a second, latent
bug. `baml.String.from` is

```baml
function from<T>(value: T) -> string throws never { root._to_string_shim(value) }
```

whose body reads `T` from a frame slot, so a zero-type-arg call traps at runtime:

```
VM internal error: could not realize type template:
template references frame type-arg slot 0 but the frame has 0 type args
```

The guard's comment claims dropping to ntypeargs=0 is safe; it is not, which means the
pre-existing out-of-scope-typevar branch of that same guard is also broken (left as-is —
out of scope here, and apparently unreached by the corpus).

So instead of dropping the arg, a receiver type containing an error-recovery sentinel is
**erased whole to `Ty::BuiltinUnknown`** — the real top type `unknown`, which lowers
cleanly. That is the only sound static erasure of a type we admit we do not know, it keeps
ntypeargs = 1 so the shim's frame is well-formed, and the shim dispatches on the runtime
value anyway.

Verified this changes **no** stdlib bytecode: regenerating
`bytecode_format__bytecode_display_textual.snap` with and without the fix produced
byte-identical output.

## Repro

`baml_src/main.baml` in this directory. Run:

```
baml-cli test --from _plan/sys_llm_native/compiler-bugs/runtime-ty-unknown/baml_src
```

Minimal form (3 lines — ICE'd before the fix, passes after):

```baml
function repro_a() -> string {
    baml.json.parse("x").to_string() catch_all (e) { _ => e.to_string() }
}
```

Both halves are required: a sugar `to_string()` in the catch **base** (defect 1 injects the
`Unknown` fact) and a `.to_string()` in the **handler** (defect 2 lowers `e` through
`try_lower_to_string_fallback`). Either alone compiles fine.

## Still open (unrelated to this ICE, no longer fatal)

A cross-package call to `ai.wire.merge_request_body` is left **unaccounted** by the throws
analysis, so the call site charges the `Ty::Unknown` fallback fact even though the function
is `throws never` (`baml describe ai.wire.merge_request_body` prints no `throws` clause).
Both `instantiated_callee_throws` and `named_callee_summary` return `None` for it.

Symptom — the catch binder is typed `unknown` instead of `never`, which forces a `_ =>` arm
on a call that provably cannot throw and infects every caller:
`aws.internal.build_request` *describes* as 7 throw types, yet `catch_all` over a call to it
reports **eight** — the 7 plus `unknown` — purely because of this one inner call. That extra
member is what made `bedrock.baml` reach the ICE.

Ruled out as the cause, each by direct experiment:

- being never-throwing — `baml.json.stringify` is fine;
- inferred rather than declared throws — `ai.wire.render_output_format` and
  `ai.wire.sanitize_for_client` are fine;
- recursion alone — a recursive same-package function is fine;
- being cross-package — `aws.internal.encode_path_label` is fine.

The distinguishing trait is that `merge_request_body` delegates to the directly-recursive
`ai.wire._merge_json`; the throw-set summary for that pair appears not to reach
`lookup_named_throw_summary`. Not chased further — after the MIR fix it only degrades a
type, it no longer crashes. Repro B is commented out at the bottom of `baml_src/main.baml`.

## On the reported "hang"

While verifying, `baml-cli test -i vertex_rejects_claude` hung (>10 min) instead of failing.
It hung **with the fix reverted too**, so it was never related to this change. That test has
since been rewritten as `vertex_claude_raw_predict` as part of the Claude-on-Vertex work; the
seven `vertex*` tests now pass 3/3 consecutive runs in ~44 s each. Considered resolved, and
in any case pre-existing.

## Verification

- `cargo test -p baml_tests --test baml_src` — `bytecode` ok, `baml_test` ok (2793 passed, 0 failed).
- `baml-cli test -i llm_bedrock` — 30/30 (the 31st is the `AWS_PROFILE`-gated `live::bedrock` leaf; the "34" in the original report was a miscount).
- `baml-cli test --from crates/baml_tests/baml_src` on a **cold** `BAML_CACHE_DIR` — 2793/2793, no panics.
- `cargo test -p baml_compiler2_mir -p baml_compiler2_tir -p baml_compiler2_emit` — all ok (324 + 41 + 3).
- `cargo fmt --check` clean; `cargo clippy` no new warnings.

Snapshots regenerated (all consequences of the new untracked `ns_llm_*` corpus, not of the
compiler change): `snapshots/baml_src/_root.snap` (+9 `$init_test` registrations),
`prompt_tag_runtime.snap` (pre-existing `OpenAiClient` -> `ResponsesClient` drift),
`llm_google.snap` (Claude-on-Vertex rewrite), plus seven new `llm_*.snap` files.

`crates/baml_tests/tests/bytecode_format` fails on this branch for an unrelated reason (new
`vercel.internal._gw_images_*` stdlib not yet snapshotted) — confirmed identical with and
without this fix.
