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
