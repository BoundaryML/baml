//! Tests for `<expr> is <pattern>` — Rust `matches!`-style pattern test.
//! Returns true if the scrutinee matches the pattern, false otherwise.
//!
//! Coverage groups (each block of tests stresses one axis of the feature):
//!   • basic type / boolean semantics
//!   • full pattern shape catalog (null, wildcard, string/float literal, enum
//!     variant, class destructure, array)
//!   • or-patterns, paren-grouped or-patterns
//!   • binding patterns — and the negative test that bindings do NOT escape
//!   • complex scrutinee expressions (calls, field access, optional chain,
//!     trailing-if-expression as scrutinee)
//!   • precedence interactions with `||`, `==`, `<`, and `is`-on-`is`
//!   • use in unusual positions (array literal, lambda body, match guard,
//!     return position)
//!   • single-evaluation of the scrutinee (side-effect smoke test)

use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

/// Collect the error messages a program produces in user files, returning
/// them as `[E0123] some message` strings. Used by the negative tests below
/// so we can assert on a specific error code/text rather than just "this
/// panicked somewhere."
fn collect_compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_project::{collect_diagnostics, testing::setup_test_db};

    let db = setup_test_db(source);
    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let user_file_ids: std::collections::HashSet<_> =
        all_files.iter().map(|f| f.file_id(&db)).collect();

    collect_diagnostics(&db, project, &all_files)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

/// Assert that compiling `source` produces *at least one* error whose
/// rendered form contains `needle`. Prints all collected errors on failure
/// so the test output is useful when diagnostic wording shifts.
#[track_caller]
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected a compile error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

// ── Group A: basic type & boolean semantics ─────────────────────────────────

#[tokio::test]
async fn is_type_match_true() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 42
            v is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_type_match_false() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = \"hi\"
            v is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_pattern_disjoint_from_scrutinee_type_returns_false() {
    // `v is string` for `v: int` should silently evaluate to false rather
    // than be a compile error — same as Rust's `matches!(some_int, "x")`.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int = 42
            v is string
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_with_not() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = \"hi\"
            !(v is int)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ── Group B: pattern shape catalog ──────────────────────────────────────────

#[tokio::test]
async fn is_null_pattern_on_null() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int? = null
            v is null
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_null_pattern_on_non_null() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int? = 7
            v is null
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_wildcard_pattern_is_always_true() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int = 42
            v is _
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_wildcard_on_null_is_true() {
    // `_` matches anything, including null.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int? = null
            v is _
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_string_literal_pattern_match() {
    let output = baml_test!(
        "
        function main() -> bool {
            let s: string = \"hello\"
            s is \"hello\"
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_string_literal_pattern_mismatch() {
    let output = baml_test!(
        "
        function main() -> bool {
            let s: string = \"world\"
            s is \"hello\"
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// (No float-literal pattern test: float-literal patterns are not yet
// supported by the language anywhere, including `match`. Once they land,
// `f is 1.5` should fall out of that work without touching `is` again.)

#[tokio::test]
async fn is_bool_literal_pattern_true() {
    let output = baml_test!(
        "
        function main() -> bool {
            let b: bool = true
            b is true
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_bool_literal_pattern_false() {
    let output = baml_test!(
        "
        function main() -> bool {
            let b: bool = true
            b is false
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_int_literal_pattern_zero() {
    let output = baml_test!(
        "
        function main() -> bool {
            let n: int = 0
            n is 0
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_int_literal_pattern_mismatch() {
    let output = baml_test!(
        "
        function main() -> bool {
            let n: int = 5
            n is 0
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_enum_variant_pattern_match() {
    let output = baml_test!(
        "
        enum Status { Active, Inactive }

        function main() -> bool {
            let s: Status = Status.Active
            s is Status.Active
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_enum_variant_pattern_mismatch() {
    let output = baml_test!(
        "
        enum Status { Active, Inactive }

        function main() -> bool {
            let s: Status = Status.Inactive
            s is Status.Active
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_class_destructure_pattern_match() {
    // A class-destructure pattern's bindings (here `name`) are scoped to the
    // pattern test itself — they don't escape the `is` expression. We're
    // confirming the destructure pattern's runtime shape test fires.
    let output = baml_test!(
        "
        class User { name string, age int }

        function main() -> bool {
            let u = User { name: \"Ada\", age: 36 }
            u is User { name, age }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_array_type_pattern() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | int[] = [1, 2, 3]
            v is int[]
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_array_type_pattern_mismatch() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | int[] = 1
            v is int[]
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// ── Group C: or-patterns ────────────────────────────────────────────────────

#[tokio::test]
async fn is_or_pattern() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string | bool = true
            v is int | bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_or_pattern_first_alternative() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string | bool = 7
            v is int | bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_or_pattern_none_match() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string | bool = \"x\"
            v is int | bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_or_pattern_with_literals() {
    let output = baml_test!(
        "
        function main() -> bool {
            let n: int = 2
            n is 1 | 2 | 3
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_or_pattern_parenthesised_alternatives() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 1
            v is (int) | (string)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ── Group D: binding patterns + the scope-leak negative test ────────────────

#[tokio::test]
async fn is_bare_binding_pattern_matches() {
    // `let x` is an irrefutable binding pattern — `v is let x` always matches.
    // We're verifying it parses and yields true.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int = 7
            v is let _
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_typed_binding_pattern_matches() {
    // The binding name `x` is local to the pattern — `is` returns bool and
    // `x` is NOT in scope after the expression.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 7
            v is let x: int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_typed_binding_pattern_mismatch() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = \"hi\"
            v is let x: int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// ── Group E: complex scrutinee expressions ──────────────────────────────────

#[tokio::test]
async fn is_with_call_scrutinee() {
    let output = baml_test!(
        "
        function get_value() -> int | string { 42 }

        function main() -> bool {
            get_value() is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_with_field_access_scrutinee() {
    let output = baml_test!(
        "
        class Wrap { value int | string }

        function main() -> bool {
            let w = Wrap { value: 7 }
            w.value is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_with_optional_chain_scrutinee() {
    // Semicolon after `null` is required so the parser doesn't read
    // `null(w?.value)` as a call on null — unrelated to `is`.
    let output = baml_test!(
        "
        class Wrap { value int? }

        function main() -> bool {
            let w: Wrap? = null;
            (w?.value) is null
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_with_parenthesised_arithmetic_scrutinee() {
    let output = baml_test!(
        "
        function main() -> bool {
            (1 + 2) is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ── Group F: precedence interactions ────────────────────────────────────────

#[tokio::test]
async fn is_chained_with_and() {
    let output = baml_test!(
        "
        function main() -> bool {
            let a: int | string = 1
            let b: int | string = 2
            a is int && b is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_chained_with_or() {
    let output = baml_test!(
        "
        function main() -> bool {
            let a: int | string = \"x\"
            let b: int | string = 2
            a is int || b is int
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_compared_with_eq_binds_tighter() {
    // `is` (BP 18/19) binds tighter than `==` (BP 16/17), so
    // `v is int == true` parses as `(v is int) == true`.
    // Semicolon after `1` keeps the parser from reading `1(v is int)` as a
    // call when the next line starts with `(`.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 1;
            (v is int) == true
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_chained_on_itself() {
    // `(v is int) is bool` — first `is` yields a bool, second tests that.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 1;
            (v is int) is bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_chained_on_itself_without_parens_is_left_associative() {
    // `v is int is bool` — both `is` operators share BP 18 and there's no
    // RHS expression to recurse on, so the Pratt loop builds them left to
    // right: `(v is int) is bool`. The first `is` returns `true` (int),
    // and `true` is a bool, so the outer `is` also returns `true`.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 1
            v is int is bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_chained_on_itself_without_parens_negative_outer() {
    // Same shape, but `v is string` is false (a `bool`), so the outer
    // `is bool` returns `true`. Useful to prove left-assoc holds: if it
    // were right-associative we'd be testing `v is (string is bool)` —
    // which doesn't even make sense as a pattern.
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 1
            v is string is bool
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_in_if_condition() {
    let output = baml_test!(
        "
        function main() -> string {
            let v: int | string = 7
            if (v is int) { \"number\" } else { \"text\" }
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("number".to_string()))
    );
}

// ── Group G: use in unusual positions ───────────────────────────────────────

#[tokio::test]
async fn is_inside_array_literal() {
    // Semicolon after `1` is required so the parser doesn't read
    // `1[v is int, v is string]` as an index — unrelated to `is`.
    let output = baml_test!(
        "
        function main() -> bool[] {
            let v: int | string = 1;
            [v is int, v is string]
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::bool(),
            items: vec![BexExternalValue::Bool(true), BexExternalValue::Bool(false),],
        })
    );
}

#[tokio::test]
async fn is_inside_lambda_body() {
    let output = baml_test!(
        "
        function main() -> bool {
            let check = (v: int | string) -> bool { v is int }
            check(42)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_as_return_value() {
    let output = baml_test!(
        "
        function is_int_param(v: int | string) -> bool {
            return v is int
        }

        function main() -> bool {
            is_int_param(7)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_in_match_guard() {
    // Match-guard expressions in BAML aren't wrapped in parens (`if x > 0
    // =>`, not `if (x > 0) =>`), so we write the `is` test bare.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = 5
            match (v) {
                let n: int if n is int => n + 1,
                _ => 0
            }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn is_in_let_initializer() {
    let output = baml_test!(
        "
        function main() -> bool {
            let v: int | string = 7
            let result: bool = v is int
            result
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_inside_while_condition() {
    // Exercises `is` as the controlling expression of a `while`.
    let output = baml_test!(
        "
        function main() -> int {
            let count = 0
            let v: int | string = 0
            while (v is int && count < 3) {
                count = count + 1
                if (count == 3) {
                    v = \"done\"
                }
            }
            count
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ── Group H: scrutinee evaluated exactly once ───────────────────────────────

#[tokio::test]
async fn is_evaluates_scrutinee_exactly_once() {
    // The scrutinee must be evaluated exactly once even when the pattern is
    // a multi-alternative or-pattern that has to test each alternative
    // against the value. A captured counter inside the scrutinee closure
    // tells us how many times we ran.
    let output = baml_test!(
        "
        function main() -> int {
            let count = 0
            let get = () -> int {
                count += 1
                42
            }
            let _ = get() is int | string | bool
            count
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ── Group J: type narrowing in if/else branches ─────────────────────────────
//
// `if (v is T) { ... }` narrows `v` to T inside the then-branch (and to the
// original type — not "the complement of T" — inside the else-branch, since
// we don't do union subtraction). Same shape as TypeScript's `typeof v ===
// "number"`.

#[tokio::test]
async fn narrowing_then_branch_picks_pattern_type() {
    // Inside the then-branch, `v` is narrowed to `int`, so `v + 1` is a
    // legal int operation. Without narrowing this would be a type error
    // because `int | string` doesn't support `+`.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = 7
            if (v is int) { v + 1 } else { 0 }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

#[tokio::test]
async fn narrowing_then_branch_picks_or_pattern_type() {
    // Or-patterns: `v is int | bool` narrows to `int | bool` in then.
    // Both arithmetic and boolean ops are defined on their respective
    // narrowed alternatives, so doing one of them per branch is fine.
    let output = baml_test!(
        "
        function describe(v: int | string | bool) -> string {
            if (v is int) { \"got int\" }
            else if (v is bool) { \"got bool\" }
            else { \"got string\" }
        }

        function main() -> string {
            describe(true)
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("got bool".to_string()))
    );
}

#[tokio::test]
async fn narrowing_else_branch_runs_for_non_matching_value() {
    // Plain runtime check: the else branch fires when the pattern
    // doesn't match. (Separate test below pins the type narrowing.)
    let output = baml_test!(
        "
        function main() -> string {
            let v: int | string = \"hi\"
            if (v is int) { \"number\" } else { \"text\" }
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("text".to_string()))
    );
}

#[tokio::test]
async fn narrowing_else_branch_subtracts_matched_type() {
    // Precise else-narrowing: `v: int | string`, `if (v is int)` —
    // inside the else, `v` is narrowed to `string`, so a `string`-only
    // operation type-checks. Without subtraction this would have been
    // `int | string` and `.length()` would fail since `int` has no
    // such method.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = \"hi\"
            if (v is int) { 0 } else { v.length() }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn narrowing_else_branch_subtracts_or_pattern() {
    // Or-pattern on the matched side: `is int | bool` subtracts both
    // `int` and `bool` from a `int | string | bool` scrutinee, leaving
    // `string` in the else.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string | bool = \"hello\"
            if (v is int | bool) { 0 } else { v.length() }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn narrowing_else_branch_with_class_types() {
    // Subtracting a class type from a `Foo | Bar` union narrows to the
    // other class in the else, so its specific field is reachable.
    let output = baml_test!(
        "
        class Success { data string }
        class Failure { reason string }

        function describe(v: Success | Failure) -> string {
            if (v is Success) { v.data } else { v.reason }
        }

        function main() -> string {
            describe(Failure { reason: \"oops\" })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("oops".to_string()))
    );
}

#[tokio::test]
async fn narrowing_else_branch_with_literal_pattern_keeps_original() {
    // Edge case: a literal pattern like `v is 0` doesn't subtract
    // anything meaningful from `int` (the pattern covers one value, not
    // a member of the union). We fall back to the original scrutinee
    // type in the else — the program still type-checks the same way it
    // would without narrowing.
    let output = baml_test!(
        "
        function main() -> int {
            let n: int = 5
            if (n is 0) { 0 } else { n + 1 }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn narrowing_negation_subtracts_in_then_branch() {
    // `!(v is int)` flips the narrowing: in the then-branch `v` is now
    // the *complement* (e.g. `string`), so its string-only operations
    // type-check.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = \"abc\"
            if (!(v is int)) { v.length() } else { 0 }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn narrowing_through_negation() {
    // `!(v is int)` flips the narrowing: in the then-branch, `v` keeps
    // the original type; in the else-branch, `v` is narrowed to int.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = 5
            if (!(v is int)) { 0 } else { v + 100 }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(105)));
}

#[tokio::test]
async fn narrowing_works_on_function_parameter() {
    // The most common case in practice: narrowing a function parameter,
    // not a `let` binding. Same code path but worth pinning explicitly.
    let output = baml_test!(
        "
        function process(v: int | string) -> int {
            if (v is int) { v * 3 } else { 0 }
        }

        function main() -> int {
            process(7)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn narrowing_picks_class_type_in_then_branch() {
    // `v is Success` narrows `v` to the class type `Success`, so the
    // then-branch can access `.data` (a field on Success) without first
    // dispatching on the union.
    let output = baml_test!(
        "
        class Success { data string }
        class Failure { reason string }

        function describe(v: Success | Failure) -> string {
            if (v is Success) { v.data } else { \"failed\" }
        }

        function main() -> string {
            describe(Success { data: \"ok\" })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string()))
    );
}

#[tokio::test]
async fn narrowing_persists_across_multiple_uses_in_then() {
    // The narrowing is applied to the local for the duration of the
    // then-branch, not just the first reference — so multiple uses of
    // `v` all see the narrowed type.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = 4
            if (v is int) {
                let a = v + 1
                let b = v + 2
                a + b
            } else {
                0
            }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn narrowing_skipped_for_non_path_scrutinee() {
    // The narrowing logic only fires when the scrutinee is a simple
    // local-variable reference. For complex scrutinees (calls, field
    // access, etc.) we don't narrow anything — the expression still
    // evaluates correctly as a runtime test, just no type refinement.
    let output = baml_test!(
        "
        class Wrap { value int | string }

        function main() -> string {
            let w = Wrap { value: 7 }
            if (w.value is int) { \"int\" } else { \"text\" }
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

#[tokio::test]
async fn narrowing_with_early_return() {
    // Early-return form: when the then-branch always diverges, the
    // else-narrowing applies to the rest of the enclosing block. So
    // after `if (v is string) { return ... }`, `v` is narrowed back to
    // its non-string alternatives — but our else keeps original today,
    // so `v` would still be `int | string` after. This test asserts
    // current behavior; tightening the else side would change it.
    let output = baml_test!(
        "
        function main() -> int {
            let v: int | string = 7
            if (v is string) { return 0 }
            if (v is int) { v * 2 } else { -1 }
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
}

// ── Group I: negative tests ─────────────────────────────────────────────────
//
// These programs must fail to compile. We assert on a substring of the
// diagnostic the compiler produces, so a regression that lets the program
// type-check would surface as "expected error containing X; got: <empty>".

#[test]
fn is_binding_does_not_leak_into_surrounding_scope() {
    // The binding `x` inside the pattern of `v is let x: int` is scoped to
    // the pattern test itself. Referring to `x` after the expression must
    // fail name resolution — that's the whole reason `is` is not `if let`.
    assert_compile_error_contains(
        "
        function main() -> int {
            let v: int | string = 7
            let _ = v is let x: int
            x
        }
        ",
        "unresolved name: x",
    );
}

#[test]
fn is_binding_does_not_leak_into_if_branch() {
    // Same property but specifically inside the `then` branch of an `if`,
    // which is where Rust's `if let` *would* bind. We must NOT mimic that.
    assert_compile_error_contains(
        "
        function main() -> int {
            let v: int | string = 7
            if (v is let x: int) { x } else { 0 }
        }
        ",
        "unresolved name: x",
    );
}

#[test]
fn is_or_pattern_with_bindings_does_not_leak() {
    // Or-patterns can introduce bindings on each side. Like the simple
    // binding case, none of them escape the pattern test.
    assert_compile_error_contains(
        "
        function main() -> int {
            let v: int | string = 7
            let _ = v is (let x: int) | (let x: string)
            x
        }
        ",
        "unresolved name: x",
    );
}

#[test]
fn is_unresolved_class_head_in_destructure_anchors_at_the_type_name() {
    // Same as the simple type-pattern case, but the unresolved name is the
    // *head* of a class-destructure pattern. Without the pat anchor for
    // `lower_class_pat`, the squiggle would land on the scrutinee.
    let errors = collect_compile_errors(
        "
        class Wrap { x int }
        function check(v: Wrap) -> bool {
            v is Frobnitz { x }
        }
        function main() -> bool { check(Wrap { x: 1 }) }
        ",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("unresolved name: Frobnitz")),
        "expected an unresolved-name diagnostic naming `Frobnitz`; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn is_unresolved_generic_arg_in_class_pattern_anchors_at_the_type_name() {
    // Class pattern with an unresolved generic arg: `v is Wrap<Missing>`
    // should anchor at the pattern, not the scrutinee.
    let errors = collect_compile_errors(
        "
        class Wrap<T> { val T }
        function check(v: Wrap<int>) -> bool {
            v is Wrap<Missing>
        }
        function main() -> bool { check(Wrap<int> { val: 1 }) }
        ",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("unresolved type: Missing")),
        "expected an unresolved-type diagnostic naming `Missing`; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn is_unresolved_type_in_pattern_anchors_at_the_type_name() {
    // Regression: `v is Frobnitz` used to anchor the "unresolved type"
    // diagnostic at the scrutinee (`v`), making the squiggle land on the
    // wrong token. After threading the pattern's `PatId` through
    // `lower_type_pat` it lands on the type name span.
    //
    // We can only check the diagnostic *text* via `collect_compile_errors`
    // — it includes `[E0002] unresolved type: Frobnitz` regardless of
    // where the squiggle points. The span check below is a structural
    // assertion: the diagnostic message must single out `Frobnitz`, not
    // anything else.
    let errors = collect_compile_errors(
        "
        function check(v: int | string) -> bool {
            v is Frobnitz
        }
        function main() -> bool { check(1) }
        ",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("unresolved type: Frobnitz")),
        "expected an unresolved-type diagnostic naming `Frobnitz`; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn is_in_parameter_type_position_is_a_syntax_error() {
    // `is` is an expression operator. Using it where a type is expected
    // (parameter type, return type, type alias…) must be rejected by the
    // parser. We assert any error appears — the exact wording isn't pinned
    // because the parser's recovery may report several stacked errors.
    let errors = collect_compile_errors(
        "
        function bad(v: v is int) -> bool { true }
        function main() -> bool { bad(1) }
        ",
    );
    assert!(
        !errors.is_empty(),
        "`is` in a parameter type position must not compile, but it did"
    );
}

#[test]
fn is_in_return_type_position_is_a_syntax_error() {
    let errors = collect_compile_errors(
        "
        function bad() -> v is int { true }
        function main() -> bool { bad() }
        ",
    );
    assert!(
        !errors.is_empty(),
        "`is` in a return type position must not compile, but it did"
    );
}

#[tokio::test]
async fn is_chained_negation_and_or() {
    // Combine `!`, `||`, and `is` to exercise the full precedence chain
    // around the operator. `!a is int` should parse as `!(a is int)` since
    // unary `!` is a prefix and `is` is infix.
    let output = baml_test!(
        "
        function main() -> bool {
            let a: int | string = \"x\"
            let b: int | string = 2
            !(a is int) || (b is int)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
