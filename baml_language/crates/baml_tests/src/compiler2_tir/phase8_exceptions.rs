//! Phase 8 tests: catch/throw/throws + match parity in compiler2 TIR.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

fn assert_declared_throws_violation(output: &str, declared: &str, thrown: &str, message: &str) {
    let expected =
        format!("declared throws is `{declared}`, but this function may also throw `{thrown}`");
    assert!(output.contains(&expected), "{message}, got:\n{output}");
}

#[test]
fn throw_expr_is_never_and_marks_following_code_dead() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
        "test.baml",
        r#"function explode() -> int {
  throw "boom"
}

function f() -> int throws never {
  return explode()
}"#,
    );

    let output = render_tir(&db, file);
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected escaping throw type to include string",
    );
}

#[test]
fn extraneous_throws_declaration_is_warning() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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

/// Ensures impossible typed bindings report a mismatch without bogus reachability errors.
#[test]
fn impossible_typed_match_binding_reports_mismatch() {
    let mut db = make_db();
    let file = db.file(
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
        output.contains("type mismatch: expected int, got string"),
        "expected `let s: string` against int scrutinee to report a type mismatch, got:\n{output}"
    );
    assert!(
        output.contains("s: string =>"),
        "expected diagnostic output to include the impossible string arm, got:\n{output}"
    );
    assert!(
        !output.contains("unreachable arm"),
        "invalid typed patterns should not emit secondary reachability diagnostics, got:\n{output}"
    );
}

#[test]
fn impossible_array_chain_match_arm_is_unreachable() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
            "The callback type does not say what it can throw. If `cb` is an infallible host callback, annotate it with `throws never`; otherwise catch the call or let the enclosing function declare/propagate the callback's throws."
        ),
        "expected direct callback violation to frame `throws never` as the infallible-host-callback case, got:\n{output}"
    );
}

#[test]
fn omitted_lambda_throws_inherits_direct_callback_context() {
    let mut db = make_db();
    let file = db.file(
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
    // The inline lambda's effective throws (`string`) binds the callee's
    // effect param, so the checker reports the precise violation — not the
    // older "may throw through callback" fallback that couldn't name the type.
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected omitted inline lambda throws to propagate through the callback param",
    );
}

#[test]
fn omitted_inline_lambda_covered_by_declared_throws_is_clean() {
    // Historically a "current limitation": the checker could not see that the
    // callback's throw was covered by `demo`'s declared `throws string` and
    // reported a callback-aware violation anyway. The lambda's effective
    // throws now binds the callee's effect param, so a covered throw is
    // accepted silently.
    let mut db = make_db();
    let file = db.file(
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
        !output.contains("declared throws"),
        "expected no throws violation when the callback throw is covered by the declared throws, got:\n{output}"
    );
}

#[test]
fn explicit_lambda_throws_annotation_is_checked_against_body() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let risky = (x: int) -> int throws never {
    throw "boom"
  }
  return risky(1)
}"#,
    );

    let output = render_tir(&db, file);
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected explicit lambda throws annotation to report the concrete escaping throw",
    );
}

#[test]
fn function_type_throws_local_alias_wrapper_uses_typed_callee_surface() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected omitted inline lambda to inherit optional function throws context",
    );
}

#[test]
fn function_type_throws_optional_call_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(cb: ((value: int) -> int throws string)?) -> int throws never {
  return cb?.(1) ?? 0
}"#,
    );

    let output = render_tir(&db, file);
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected optional call to propagate the callee throws surface into the contract check",
    );
}

#[test]
fn named_reordered_callback_arg_instantiates_throws_from_call_plan() {
    let mut db = make_db();
    let file = db.file(
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
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected reordered named callback arg to instantiate concrete throws",
    );
}

#[test]
fn callable_throws_uses_call_plan_for_named_reordered_args() {
    let mut db = make_db();
    let file = db.file(
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
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected callable throws summary to use reordered named call binding",
    );
}

#[test]
fn unbound_method_call_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.file(
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
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected unbound method call to report the concrete escaping throw",
    );
    assert!(
        !output.contains("missing unknown"),
        "expected unbound method call to avoid collapsing the propagated throw to unknown, got:\n{output}"
    );
}

#[test]
fn omitted_lambda_throws_inherits_builtin_map_callback_context() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(values: int[]) -> int[] throws never {
  return values.map((value: int) -> int {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    // The lambda's EFFECTIVE throws (`string`, from its body) feeds `map`'s
    // throws generic, so the violation names the concrete type rather than a
    // symbolic `E` or a collapsed `unknown`.
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected builtin map to propagate omitted inline lambda throws",
    );
    assert!(
        !output.contains("missing unknown"),
        "expected builtin map omitted lambda path to stay concrete rather than collapsing to unknown, got:\n{output}"
    );
}

#[test]
fn function_type_throws_builtin_map_propagates_callback_surface() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(values: int[]) -> int[] throws never {
  return values.map((value: int) -> int throws string {
    throw "boom"
  })
}"#,
    );

    let output = render_tir(&db, file);
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected builtin map to propagate callback throws into the enclosing contract check",
    );
}

#[test]
fn generic_bound_associated_error_is_reused_by_throws_analysis() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class Boom {}

interface Runner<Input> {
  type Output
  type Error

  function run(self, input: Input) -> Self.Output throws Self.Error
}

class Task<T> {
  function run<Output, Error, R extends Runner<Task<T>, Output = Output, Error = Error>>(
    self,
    runner: R,
  ) -> Output throws Error {
    runner.run(self)
  }
}

class ConcreteRunner {
  implements Runner<Task<int>> {
    type Output = int
    type Error = Boom

    function run(self, input: Task<int>) -> int throws Boom {
      throw Boom {}
    }
  }
}

function caller(task: Task<int>) -> int throws Boom {
  task.run(runner = ConcreteRunner {})
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("declared throws"),
        "expected the runner's concrete associated Error to satisfy the caller, got:\n{output}"
    );
    assert!(
        !output.contains("extraneous throws declaration"),
        "expected the concrete Boom throw to remain visible, got:\n{output}"
    );
}

#[test]
fn stored_lambda_with_omitted_throws_is_inferred_not_violation() {
    let mut db = make_db();
    let file = db.file(
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
    // The unannotated lambda's surface is INFERRED (`throws string`), so the
    // throw inside it is not a local violation — it becomes part of the
    // lambda's type and is checked wherever the lambda is invoked.
    assert!(
        !output.contains("declared throws"),
        "expected no local violation for an unannotated lambda (throws are inferred), got:\n{output}"
    );
}

#[test]
fn defining_a_throwing_lambda_does_not_charge_the_enclosing_functions_throw_set() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function defines(value: int) -> int throws never {
  let risky = (n: int) -> int {
    throw "boom"
  }
  return value
}

function calls(value: int) -> int throws never {
  return defines(value)
}"#,
    );

    // `defines` never invokes `risky`, so `boom` is the lambda's effect and not
    // its definer's. The distinction is carried by walking the body structurally
    // and stopping at `Expr::Lambda`; a flat scan of the expression arena would
    // see the `throw` as though `defines` wrote it, give `defines` a
    // package-level throw set of `string`, and propagate that to every caller.
    let output = render_tir(&db, file);
    assert!(
        !output.contains("declared throws"),
        "neither the definer nor its caller may violate `throws never`, got:\n{output}"
    );
    // The effect is not lost, just attributed to the right place: the lambda's
    // own inferred type carries it. FLIPPED to literal grain: the spec's
    // callback_effect_param_flows_through fixture pins inferred surfaces
    // keeping the thrown literal's type (TIR widened here).
    assert!(
        output.contains("(n: int) -> int throws \"boom\""),
        "the throw must land on the lambda's inferred type, got:\n{output}"
    );
}

#[test]
fn alias_hidden_omitted_lambda_reports_local_violation() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"type HiddenHandler = (value: int) -> int throws never

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
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected alias-hidden local violation to report the escaping throw",
    );
}

#[test]
fn returned_omitted_lambda_reports_local_violation() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function make() -> (value: int) -> int {
  return (value: int) -> int {
    throw "boom"
  }
}"#,
    );

    let output = render_tir(&db, file);
    assert_declared_throws_violation(
        &output,
        "never",
        "string",
        "expected returned omitted lambda violation to report the escaping throw",
    );
}

#[test]
fn function_type_throws_alias_hidden_callback_rejects_throwing_value() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"type HiddenHandler = (value: int) -> int throws never

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
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file(
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
    // hir_ty keeps LITERAL grain on cross-function throw surfaces (the
    // ratified S13 rule; TIR widened facts at the call boundary), so the
    // one fact here is `42`: the literal arm handles it completely and
    // BOTH later arms are provably unreachable.
    assert_eq!(
        unreachable_count, 2,
        "under literal-grain facts the typed and wildcard arms are dead, got:
{output}"
    );
}

#[test]
fn enum_variant_catch_arm_does_not_consume_entire_enum_from_residual() {
    let mut db = make_db();
    let file = db.file(
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

// ── BEP-034 `spawn ... with` middleware diagnostics ──────────────────────
// Every wrong shape must produce a CONCRETE, actionable message (the chain's
// actual input type, never `T, E` placeholders or silence).

#[test]
fn spawn_with_non_callable_reports_concrete_mismatch() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int { let x = spawn with 42 { 1 }; await x }"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains(
            "expected (baml.spawn.Params<int, never>) -> baml.spawn.Params<unknown, unknown> throws unknown, got 42"
        ),
        "non-callable `with` must report the concrete transformer shape, got:\n{output}"
    );
}

#[test]
fn spawn_with_wrong_shape_fn_names_the_contract() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function g(n: int) -> int { n }
function f() -> int { let x = spawn with g { 1 }; await x }"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("must return a `baml.spawn.Params`")
            && output.contains("got `(n: int) -> int throws never`"),
        "wrong-shape `with` must name the middleware contract, got:\n{output}"
    );
}

#[test]
fn spawn_with_wrong_return_reports_link_input() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function h<T, E>() -> (baml.spawn.Params<T, E>) -> int throws never { (p) -> { 7 } }
function f() -> int { let x = spawn with h() { 1 }; await x }"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("this link receives `baml.spawn.Params<int, never>`")
            && output.contains("must return a `baml.spawn.Params`"),
        "wrong-return transformer must report the link's concrete input, got:\n{output}"
    );
}

#[test]
fn spawn_with_chain_input_mismatch_is_concrete() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function fix() -> (baml.spawn.Params<string, never>) -> baml.spawn.Params<string, never> throws never { (p) -> { p } }
function f() -> int { let x = spawn with fix() { 1 }; await x }"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("got (baml.spawn.Params<string, never>)")
            && output.contains("expected (baml.spawn.Params<int, never>)"),
        "chain input mismatch must show both concrete Params types, got:\n{output}"
    );
}

#[test]
fn spawn_with_non_fn_variable_reports() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let nope = 7;
  let x = spawn with nope { 1 };
  await x
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("takes middleware transformer functions") && output.contains("got `int`"),
        "non-function variable in `with` must report, got:\n{output}"
    );
}

#[test]
fn spawn_with_wrong_param_variable_reports() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function g(n: int) -> int { n }
function f() -> int {
  let t = g;
  let x = spawn with t { 1 };
  await x
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("this link receives `baml.spawn.Params<int, never>`"),
        "wrong-param variable transformer must report the link input, got:\n{output}"
    );
}

#[test]
fn defer_return_escape_reports_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  defer { return 1 }
  0
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("`return` cannot leave a `defer` body"),
        "expected DeferControlFlowEscape error for return, got:\n{output}"
    );
}

#[test]
fn defer_break_escape_reports_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  for (let i in [1, 2]) {
    defer { break }
  }
  0
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("`break` cannot leave a `defer` body"),
        "expected DeferControlFlowEscape error for break escaping to the outer loop, got:\n{output}"
    );
}

#[test]
fn defer_inner_loop_break_is_allowed() {
    // BEP-042 loop-aware rule: a break targeting a loop declared INSIDE the
    // defer body does not escape the defer and must be accepted.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  defer {
    for (let x in [1, 2]) {
      break
    }
  }
  0
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        !output.contains("cannot leave a `defer` body"),
        "break targeting a loop inside the defer should be allowed, got:\n{output}"
    );
}

#[test]
fn defer_throw_is_allowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  defer { throw "x" }
  0
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        !output.contains("cannot leave a `defer` body"),
        "throw inside a defer should be allowed, got:\n{output}"
    );
}
