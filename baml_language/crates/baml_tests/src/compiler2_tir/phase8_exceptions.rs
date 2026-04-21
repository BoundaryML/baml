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
    AppError | DivisionByZero => e.code
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
    _: AppError | baml.panics.DivisionByZero => 2
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
        output.contains("throws contract violation: `never` is missing callback"),
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
        output.contains("throws contract violation: `never` is missing callback"),
        "expected omitted inline lambda to inherit callback throws context, got:\n{output}"
    );
    assert!(
        !output.contains("missing string"),
        "expected contextual callback typing rather than a closed local-lambda violation, got:\n{output}"
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
        output.contains("throws contract violation: `never` is missing callback"),
        "expected typed call through local wrapper alias to propagate callback throws, got:\n{output}"
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
