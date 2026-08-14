// Repro A (FIXED) — the ICE this directory was opened for.
//
//   internal error: `Unknown` is not a valid `RuntimeTy`:
//   an error-recovery type reached runtime lowering
//   (crates/baml_type/src/runtime_ty.rs:237, via
//    baml_compiler2_mir::lower::lower_tir_template)
//
// Two independent defects had to line up; both are fixed:
//
//  1. `Builder::collect_throw_facts_from_expr` (the walker that types a
//     catch/catch_all binder) duplicated `throws_analysis::collect_from_expr`'s
//     `Call` arm but omitted its to_string/to_json/from_json sugar-fallback
//     guards. Those three call shapes never resolve to a real method, so they
//     fell through to `collect_callee_escaping_throws`'s unaccounted-callee
//     default and charged a bogus `Ty::Unknown` throw fact. `e` was then typed
//     `... | Unknown`.
//  2. MIR's three sugar-fallback lowerings tested only for a TOP-LEVEL
//     `Tir2Ty::Unknown` receiver, so a sentinel nested inside a union sailed
//     past the guard into `ty_to_template`, which panics instead of degrading.
function repro_a() -> string {
    baml.json.parse("x").to_string() catch_all (e) { _ => e.to_string() }
}

test "repro_a_compiles_and_runs" {
    assert.equal(repro_a().length() > 0, true)
}

// Repro B (STILL OPEN, no longer fatal) — a cross-package call to
// `ai.wire.merge_request_body` is left UNACCOUNTED by the throws analysis, so
// the call site charges the `Ty::Unknown` fallback fact even though the
// function's own signature is `throws never` (`baml describe
// ai.wire.merge_request_body` prints no `throws` clause).
//
// Symptom: the catch binder below is typed `unknown` rather than `never`, which
//   * forces a `_ =>` arm on a call that provably cannot throw, and
//   * infects every caller: `aws.internal.build_request` describes as 7 throw
//     types, yet `catch_all` over a call to it reports EIGHT — the 7 plus
//     `unknown` — purely because of this one inner call.
//
// It is NOT explained by any of: being never-throwing (`baml.json.stringify`
// is fine), having inferred rather than declared throws (`ai.wire`'s
// `render_output_format` / `sanitize_for_client` are fine), recursion alone
// (a recursive same-package function is fine), or being cross-package
// (`aws.internal.encode_path_label` is fine). The distinguishing trait of
// `merge_request_body` is that it delegates to the directly-recursive
// `ai.wire._merge_json`; the throw-set summary for that pair appears not to
// reach `lookup_named_throw_summary`, so both `instantiated_callee_throws` and
// `named_callee_summary` return None.
//
// After the MIR guard fix this only degrades the catch binder's TYPE; it no
// longer ICEs. Uncomment to observe:
//
//     error[E0062]: non-exhaustive catch_all on type unknown; missing: unknown
//
// function repro_b(base: json, overrides: json?) -> json {
//     ai.wire.merge_request_body(base, overrides) catch_all (e) {
//         let never_reached: string => base,
//     }
// }
