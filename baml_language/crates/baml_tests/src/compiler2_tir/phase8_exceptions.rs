//! Phase 8 tests: catch/throw/throws + match parity in compiler2 TIR.

use super::support::{make_db, render_tir};

#[test]
fn throw_expr_is_never_and_marks_following_code_dead() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f() -> int {
  let x = throw "boom"
  return 1
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains(r#"let x = throw "boom" : never"#),
        "expected throw expression to infer as never, got:\n{output}"
    );
    assert!(
        output.contains("unreachable code"),
        "expected dead-code diagnostic after throw, got:\n{output}"
    );
}

#[test]
fn throw_call_catch_binds_catch_to_call_payload() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class TimeoutError {
  retryAfterMs int
}

function make_err() -> int {
  throw TimeoutError { retryAfterMs: 25 }
}

function f() -> int {
  return throw make_err() catch (e) {
    TimeoutError => e.retryAfterMs
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("catch (throw make_err() : never)"),
        "expected parser/lowering shape for `throw f() catch (...)`, got:\n{output}"
    );
    assert!(
        output.contains("TimeoutError =>") && output.contains("e.retryAfterMs : int"),
        "expected catch arm rendering for payload catch, got:\n{output}"
    );
}

#[test]
fn throws_never_contract_violation_reports_error() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function explode() -> int {
  throw "boom"
}

function f() -> int throws never {
  return explode()
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected throws-contract violation, got:\n{output}"
    );
    assert!(
        output.contains("string"),
        "expected escaping throw type to include string, got:\n{output}"
    );
}

#[test]
fn extraneous_throws_declaration_is_warning() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f() -> int throws string {
  return 1
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("??"),
        "expected warning marker for extraneous throws declaration, got:\n{output}"
    );
    assert!(
        output.contains("extraneous throws declaration"),
        "expected extraneous throws diagnostic, got:\n{output}"
    );
}

#[test]
fn match_bare_type_arm_narrows_scrutinee_in_arm_scope() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class Ok {
  value int
}

class Err {
  message string
}

function f(r: Ok | Err) -> int {
  return match (r) {
    Ok => r.value
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("Ok =>"),
        "expected bare-type arm to parse as type-pattern arm, got:\n{output}"
    );
    assert!(
        output.contains("r.value : int"),
        "expected scrutinee narrowing in Ok arm, got:\n{output}"
    );
}

#[test]
fn bare_type_match_arm_is_not_variable_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class TimeoutError {
  retryAfterMs int
}

class OtherError {
  code int
}

function f(e: TimeoutError | OtherError) -> int {
  return match (e) {
    TimeoutError => e.retryAfterMs
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("unresolved name: TimeoutError"),
        "bare type arm should not be treated as a value binding, got:\n{output}"
    );
    assert!(
        output.contains("e.retryAfterMs : int"),
        "expected narrowing from bare-type sugar, got:\n{output}"
    );
}

#[test]
fn impossible_typed_match_binding_is_unreachable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int) -> string {
  return match (x) {
    let s: string => s,
    _ => "fallback"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("unreachable arm"),
        "expected `let s: string` against int scrutinee to be unreachable, got:\n{output}"
    );
    assert!(
        output.contains("s: string =>"),
        "expected diagnostic output to include the impossible string arm, got:\n{output}"
    );
}

#[test]
fn impossible_array_chain_match_arm_is_unreachable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(xs: int[]) -> string {
  return match (xs) {
    []: string[] => "bad",
    _ => "fallback"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("type mismatch"),
        "expected `[]: string[]` against int[] scrutinee to produce a type mismatch, got:\n{output}"
    );
    assert!(
        output.contains("string[]"),
        "expected type-mismatch diagnostic to mention string[], got:\n{output}"
    );
}

// `[]: int` ascribes a non-array type to an array pattern. The pattern's
// natural shape is "an array", so the ascription must itself be an array
// type — otherwise the pattern can never match any value of any scrut.
#[test]
fn array_pattern_with_non_array_ascription_is_rejected() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(xs: int[]) -> string {
  return match (xs) {
    []: int => "bad",
    _ => "fallback"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("type mismatch"),
        "expected `[]: int` against int[] scrutinee to produce a type mismatch, got:\n{output}"
    );
}

#[test]
fn typed_pattern_without_widening_does_not_make_union_match_exhaustive() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(v: int | string) -> string {
  return match (v) {
    let x: int => "int"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("non-exhaustive match"),
        "expected plain int arm to leave string branch uncovered, got:\n{output}"
    );
    assert!(
        output.contains("missing:"),
        "expected non-exhaustive diagnostic to include missing case details, got:\n{output}"
    );
}

#[test]
fn catch_binding_is_narrowed_per_arm() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class TimeoutError {
  retryAfterMs int
}

class OtherError {
  code int
}

function fail(which: int) -> int {
  if (which == 0) {
    throw TimeoutError { retryAfterMs: 5 }
  }
  throw OtherError { code: 9 }
}

function f(which: int) -> int {
  return fail(which) catch (e) {
    TimeoutError => e.retryAfterMs
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("TimeoutError =>"),
        "expected typed catch arm to lower correctly, got:\n{output}"
    );
    assert!(
        output.contains("e.retryAfterMs : int"),
        "expected per-arm catch-binding narrowing, got:\n{output}"
    );
}

#[test]
#[ignore = "catch bindings are now bare identifiers; typed `catch (e: any)` no longer parses"]
fn typed_any_and_unknown_catch_bindings_are_rejected() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f() -> int {
  return 1 catch (e: any) {
    _ => 0
  }
}

function g() -> int {
  return 1 catch (e: unknown) {
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("invalid catch binding type `any`"),
        "expected `any` catch-binding diagnostic, got:\n{output}"
    );
    assert!(
        output.contains("invalid catch binding type `unknown`"),
        "expected `unknown` catch-binding diagnostic, got:\n{output}"
    );
}

#[test]
fn unreachable_catch_arm_is_warning() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class TimeoutError {
  retryAfterMs int
}

function fail() -> int {
  throw TimeoutError { retryAfterMs: 5 }
}

function f() -> int {
  return fail() catch (e) {
    _ => 1
    TimeoutError => 2
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("??") && output.contains("unreachable arm"),
        "expected unreachable catch arm warning, got:\n{output}"
    );
}

#[test]
fn mixed_panic_union_catch_binding_requires_further_narrowing() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class AppError {
  code int
}

function fail(which: int) -> int {
  if (which == 0) {
    throw AppError { code: 7 }
  }
  1 / 0
}

function f(which: int) -> int {
  return fail(which) catch (e) {
    AppError | baml.panics.DivisionByZero => e.code
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    // Multi-variant union bindings require instanceof narrowing before field
    // access, so `e.code` should produce a "has no member" error.
    assert!(
        output.contains("has no member"),
        "mixed union catch bindings should require further narrowing before field access, got:\n{output}"
    );
}

#[test]
fn panic_containing_union_after_wildcard_is_not_unreachable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class AppError {
  code int
}

function fail() -> int {
  throw AppError { code: 7 }
}

function f() -> int {
  return fail() catch (e) {
    _ => 1
    AppError | baml.panics.DivisionByZero => 2
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("unreachable arm"),
        "mixed panic union arm should stay reachable because panics may still occur, got:\n{output}"
    );
}

#[test]
fn single_panic_catch_binding_allows_field_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function fail() -> int {
  1 / 0
}

function f() -> int {
  return fail() catch (e) {
    DivisionByZero => e.dividend
    _ => 0
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("unresolved member") && !output.contains("cannot access field"),
        "single-type catch binding should allow field access, got:\n{output}"
    );
}

#[test]
fn function_type_throws_direct_callback_violation_is_humanized() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function forward(cb: (value: int) -> int) -> int throws never {
  return cb(1)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output
            .contains("this body may throw through callback `cb`, but declared throws is `never`."),
        "expected direct callback violation to mention the callback explicitly, got:\n{output}"
    );
    assert!(
        output.contains(
            "Add an explicit `throws` to the callback, catch the call, or make the callback non-throwing."
        ),
        "expected direct callback violation to use callback-oriented wording, got:\n{output}"
    );
}

#[test]
fn omitted_lambda_throws_inherits_direct_callback_context() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function forward(cb: (value: int) -> int) -> int {
  return cb(1)
}

function f() -> int throws never {
  return forward((value: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output
            .contains("this body may throw through callback `cb`, but declared throws is `never`."),
        "expected omitted inline lambda to inherit callback throws context, got:\n{output}"
    );
    assert!(
        output.contains(
            "Add `throws string` to the callback, catch the call, or make the callback non-throwing."
        ),
        "expected actionable callback-specific fix text, got:\n{output}"
    );
}

#[test]
fn omitted_inline_lambda_current_limitation_gets_callback_aware_primary_message() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function forward(cb: (x: int) -> int) -> int {
  return cb(1)
}

function demo() -> int throws string {
  return forward((x: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains(
            "this body may throw through callback `cb`, but declared throws is `string`."
        ),
        "expected callback-aware primary message for current limitation, got:\n{output}"
    );
    assert!(
        output.contains("Add `throws string` to the callback, catch the call, or make the callback non-throwing."),
        "expected callback-specific fix text for current limitation, got:\n{output}"
    );
}

#[test]
fn explicit_lambda_throws_annotation_is_checked_against_body() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f() -> int {
  let risky = (x: int) -> int throws never {
    throw "boom"
  }
  return risky(1)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected explicit lambda throws annotation to be validated, got:\n{output}"
    );
    assert!(
        output.contains("missing string"),
        "expected lambda throws validation to report the concrete escaping throw, got:\n{output}"
    );
}

#[test]
fn function_type_throws_local_alias_wrapper_uses_typed_callee_surface() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function outer(cb: (value: int) -> int) -> int {
  return cb(1)
}

function f(cb: (value: int) -> int) -> int throws never {
  let h = outer
  return h(cb)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output
            .contains("this body may throw through callback `cb`, but declared throws is `never`."),
        "expected typed call through local wrapper alias to propagate callback throws with callback-aware wording, got:\n{output}"
    );
    assert!(
        !output.contains("missing unknown"),
        "expected typed call path to avoid collapsing wrapper propagation to unknown, got:\n{output}"
    );
}

#[test]
fn omitted_lambda_throws_inherits_optional_function_context() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function maybe_call(cb: ((value: int) -> int throws string)?) -> int {
  return cb?.(1) ?? 0
}

function f() -> int throws never {
  return maybe_call((value: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation: `never` is missing string"),
        "expected omitted inline lambda to inherit optional function throws context, got:\n{output}"
    );
}

#[test]
fn function_type_throws_optional_call_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(cb: ((value: int) -> int throws string)?) -> int throws never {
  return cb?.(1) ?? 0
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation: `never` is missing string"),
        "expected optional call to propagate the callee throws surface into the contract check, got:\n{output}"
    );
}

#[test]
fn named_reordered_callback_arg_instantiates_throws_from_call_plan() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function invoke(cb: (value: int) -> int, value: int = 1) -> int {
  cb(value)
}

function risky(value: int) -> int throws string {
  throw "boom"
}

function f() -> int throws never {
  invoke(value = 1, cb = risky)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation: `never` is missing string"),
        "expected reordered named callback arg to instantiate concrete throws, got:\n{output}"
    );
}

#[test]
fn callable_throws_uses_call_plan_for_named_reordered_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function invoke(cb: (value: int) -> int, value: int = 1) -> int {
  cb(value)
}

function forward(cb: (value: int) -> int) -> int {
  invoke(value = 1, cb = cb)
}

function risky(value: int) -> int throws string {
  throw "boom"
}

function f() -> int throws never {
  forward(risky)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation: `never` is missing string"),
        "expected callable throws summary to use reordered named call binding, got:\n{output}"
    );
}

#[test]
fn unbound_method_call_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class Box {
  function call<E>(self, cb: () -> int throws E) -> int throws E {
    return cb()
  }
}

function risky() -> int throws string {
  throw "boom"
}

function f(box: Box) -> int throws never {
  return Box.call(box, risky)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected unbound method call to propagate callback throws into the contract check, got:\n{output}"
    );
    assert!(
        output.contains("missing string"),
        "expected unbound method call to report the concrete escaping throw, got:\n{output}"
    );
    assert!(
        !output.contains("missing unknown"),
        "expected unbound method call to avoid collapsing the propagated throw to unknown, got:\n{output}"
    );
}

#[test]
fn omitted_lambda_throws_inherits_builtin_map_callback_context() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(values: int[]) -> int[] throws never {
  return values.map((value: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected builtin map to propagate omitted inline lambda throws, got:\n{output}"
    );
    assert!(
        !output.contains("missing unknown"),
        "expected builtin map omitted lambda path to stay symbolic/concrete rather than collapsing to unknown, got:\n{output}"
    );
}

#[test]
fn function_type_throws_builtin_map_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(values: int[]) -> int[] throws never {
  return values.map((value: int) -> int throws string {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation: `never` is missing string"),
        "expected builtin map to propagate callback throws into the enclosing contract check, got:\n{output}"
    );
}

#[test]
fn stored_lambda_with_omitted_throws_reports_local_violation() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f() -> int {
  let risky = (value: int) -> int {
    if (value < 0) { throw "boom" }
    value
  }
  return 0
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected stored lambda with omitted throws to fail locally, got:\n{output}"
    );
    assert!(
        output.contains("missing string"),
        "expected local closed-lambda violation to report the concrete escaping throw, got:\n{output}"
    );
}

#[test]
fn alias_hidden_omitted_lambda_reports_local_violation() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"type HiddenHandler = (value: int) -> int

function store(handler: HiddenHandler) -> int throws never {
  return handler(1)
}

function f() -> int {
  return store((value: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected alias-hidden omitted lambda to fail locally, got:\n{output}"
    );
    assert!(
        output.contains("missing string"),
        "expected alias-hidden local violation to report the escaping throw, got:\n{output}"
    );
}

#[test]
fn returned_omitted_lambda_reports_local_violation() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function make() -> (value: int) -> int {
  return (value: int) -> int {
    throw "boom"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("throws contract violation"),
        "expected returned omitted lambda to fail locally, got:\n{output}"
    );
    assert!(
        output.contains("missing string"),
        "expected returned omitted lambda violation to report the escaping throw, got:\n{output}"
    );
}

#[test]
fn function_type_throws_alias_hidden_callback_rejects_throwing_value() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"type HiddenHandler = (value: int) -> int

function store(handler: HiddenHandler) -> int throws never {
  return handler(1)
}

function risky(value: int) -> int throws string {
  throw "boom"
}

function f() -> int {
  return store(risky)
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("type mismatch")
            && output.contains("HiddenHandler")
            && output.contains("throws string"),
        "expected alias-hidden callback surface to stay explicit-only, got:\n{output}"
    );
}

#[test]
fn function_type_throws_stored_callback_rejects_throwing_value() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class Holder {
  cb (value: int) -> int
}

function risky(value: int) -> int throws string {
  throw "boom"
}

function store() -> Holder {
  return Holder { cb: risky }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("type mismatch")
            && output.contains("(value: int) -> int")
            && output.contains("throws string"),
        "expected stored callback surfaces to stay explicit-only, got:\n{output}"
    );
}

#[test]
fn function_type_throws_returned_closure_rejects_throwing_value() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function make() -> (value: int) -> int {
  let risky = (value: int) -> int throws string {
    throw "boom"
  }
  return risky
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("type mismatch")
            && output.contains("(value: int) -> int")
            && output.contains("throws string"),
        "expected returned closure surface to stay explicit-only, got:\n{output}"
    );
}

#[test]
fn literal_catch_arm_does_not_consume_entire_type_from_residual() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function fail() -> int {
  throw 42
}

function f() -> int {
  return fail() catch (e) {
    42 => 1,
    int => 2,
    _ => 3
  }
}"#,
    );

    let output = render_tir(&db, file);
    let unreachable_count = output.matches("unreachable arm").count();
    // Only the trailing wildcard `_ => 3` should be unreachable (int is fully
    // handled by the literal + typed arms). The `int` arm must stay reachable.
    assert!(
        unreachable_count <= 1,
        "typed int arm after literal 42 arm should NOT be unreachable, got:\n{output}"
    );
}

#[test]
fn enum_variant_catch_arm_does_not_consume_entire_enum_from_residual() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"enum Status {
  Active
  Inactive
  Pending
}

function fail(which: int) -> int {
  if (which == 0) { throw Status.Active }
  if (which == 1) { throw Status.Inactive }
  throw Status.Pending
}

function f() -> int {
  return fail(0) catch (e) {
    Status.Active => 1,
    Status => 2,
    _ => 3
  }
}"#,
    );

    let output = render_tir(&db, file);
    let unreachable_count = output.matches("unreachable arm").count();
    // Only the trailing wildcard should be unreachable. The `Status` arm
    // must stay reachable since Status.Active doesn't cover all variants.
    assert!(
        unreachable_count <= 1,
        "typed Status arm after Status.Active arm should NOT be unreachable, got:\n{output}"
    );
}
