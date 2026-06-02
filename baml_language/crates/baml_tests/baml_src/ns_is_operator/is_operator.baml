// Tests for `<expr> is <pattern>` — runtime pattern matching that returns bool.
//
// Converted from crates/baml_tests/tests/is_operator.rs. Each test's
// `function main()` body is lifted into a top-level helper function (rather
// than inlined into the test block) because the `is` operator and reflection
// inspect a value's runtime type — applied to a boxed test-block local they
// will misreport. Inside a function, locals are fine.
//
// SKIPPED (Group I — compile-error tests):
//   is_binding_does_not_leak_into_surrounding_scope
//   is_binding_does_not_leak_into_if_branch
//   is_or_pattern_with_bindings_does_not_leak
//   is_unresolved_class_head_in_destructure_anchors_at_the_type_name
//   is_unresolved_generic_arg_in_class_pattern_anchors_at_the_type_name
//   is_unresolved_type_in_pattern_anchors_at_the_type_name
//   is_in_parameter_type_position_is_a_syntax_error
//   is_in_return_type_position_is_a_syntax_error

// ── Types used across tests ───────────────────────────────────────────────────

enum Status { Active, Inactive }

class User { name string, age int }

class Wrap { value int | string }

class WrapOpt { value int? }

class Success { data string }
class Failure { reason string }

// ── Group A: basic type & boolean semantics ───────────────────────────────────

function is_type_match_true_fn() -> bool {
    let v: int | string = 42
    v is int
}

test "is_type_match_true" {
    assert.is_true(baml.deep_equals(is_type_match_true_fn(), true))
}

function is_type_match_false_fn() -> bool {
    let v: int | string = "hi"
    v is int
}

test "is_type_match_false" {
    assert.is_true(baml.deep_equals(is_type_match_false_fn(), false))
}

function is_pattern_disjoint_from_scrutinee_type_returns_false_fn() -> bool {
    let v: int = 42
    v is string
}

test "is_pattern_disjoint_from_scrutinee_type_returns_false" {
    assert.is_true(baml.deep_equals(is_pattern_disjoint_from_scrutinee_type_returns_false_fn(), false))
}

function is_with_not_fn() -> bool {
    let v: int | string = "hi"
    !(v is int)
}

test "is_with_not" {
    assert.is_true(baml.deep_equals(is_with_not_fn(), true))
}

// ── Group B: pattern shape catalog ───────────────────────────────────────────

function is_null_pattern_on_null_fn() -> bool {
    let v: int? = null
    v is null
}

test "is_null_pattern_on_null" {
    assert.is_true(baml.deep_equals(is_null_pattern_on_null_fn(), true))
}

function is_null_pattern_on_non_null_fn() -> bool {
    let v: int? = 7
    v is null
}

test "is_null_pattern_on_non_null" {
    assert.is_true(baml.deep_equals(is_null_pattern_on_non_null_fn(), false))
}

function is_wildcard_pattern_is_always_true_fn() -> bool {
    let v: int = 42
    v is _
}

test "is_wildcard_pattern_is_always_true" {
    assert.is_true(baml.deep_equals(is_wildcard_pattern_is_always_true_fn(), true))
}

function is_wildcard_on_null_is_true_fn() -> bool {
    let v: int? = null
    v is _
}

test "is_wildcard_on_null_is_true" {
    assert.is_true(baml.deep_equals(is_wildcard_on_null_is_true_fn(), true))
}

function is_string_literal_pattern_match_fn() -> bool {
    let s: string = "hello"
    s is "hello"
}

test "is_string_literal_pattern_match" {
    assert.is_true(baml.deep_equals(is_string_literal_pattern_match_fn(), true))
}

function is_string_literal_pattern_mismatch_fn() -> bool {
    let s: string = "world"
    s is "hello"
}

test "is_string_literal_pattern_mismatch" {
    assert.is_true(baml.deep_equals(is_string_literal_pattern_mismatch_fn(), false))
}

function is_bool_literal_pattern_true_fn() -> bool {
    let b: bool = true
    b is true
}

test "is_bool_literal_pattern_true" {
    assert.is_true(baml.deep_equals(is_bool_literal_pattern_true_fn(), true))
}

function is_bool_literal_pattern_false_fn() -> bool {
    let b: bool = true
    b is false
}

test "is_bool_literal_pattern_false" {
    assert.is_true(baml.deep_equals(is_bool_literal_pattern_false_fn(), false))
}

function is_int_literal_pattern_zero_fn() -> bool {
    let n: int = 0
    n is 0
}

test "is_int_literal_pattern_zero" {
    assert.is_true(baml.deep_equals(is_int_literal_pattern_zero_fn(), true))
}

function is_int_literal_pattern_mismatch_fn() -> bool {
    let n: int = 5
    n is 0
}

test "is_int_literal_pattern_mismatch" {
    assert.is_true(baml.deep_equals(is_int_literal_pattern_mismatch_fn(), false))
}

function is_enum_variant_pattern_match_fn() -> bool {
    let s: Status = Status.Active
    s is Status.Active
}

test "is_enum_variant_pattern_match" {
    assert.is_true(baml.deep_equals(is_enum_variant_pattern_match_fn(), true))
}

function is_enum_variant_pattern_mismatch_fn() -> bool {
    let s: Status = Status.Inactive
    s is Status.Active
}

test "is_enum_variant_pattern_mismatch" {
    assert.is_true(baml.deep_equals(is_enum_variant_pattern_mismatch_fn(), false))
}

function is_class_destructure_pattern_match_fn() -> bool {
    let u = User { name: "Ada", age: 36 }
    u is User { name, age }
}

test "is_class_destructure_pattern_match" {
    assert.is_true(baml.deep_equals(is_class_destructure_pattern_match_fn(), true))
}

function is_array_type_pattern_fn() -> bool {
    let v: int | int[] = [1, 2, 3]
    v is int[]
}

test "is_array_type_pattern" {
    assert.is_true(baml.deep_equals(is_array_type_pattern_fn(), true))
}

function is_array_type_pattern_mismatch_fn() -> bool {
    let v: int | int[] = 1
    v is int[]
}

test "is_array_type_pattern_mismatch" {
    assert.is_true(baml.deep_equals(is_array_type_pattern_mismatch_fn(), false))
}

// ── Group C: or-patterns ──────────────────────────────────────────────────────

function is_or_pattern_fn() -> bool {
    let v: int | string | bool = true
    v is int | bool
}

test "is_or_pattern" {
    assert.is_true(baml.deep_equals(is_or_pattern_fn(), true))
}

function is_or_pattern_first_alternative_fn() -> bool {
    let v: int | string | bool = 7
    v is int | bool
}

test "is_or_pattern_first_alternative" {
    assert.is_true(baml.deep_equals(is_or_pattern_first_alternative_fn(), true))
}

function is_or_pattern_none_match_fn() -> bool {
    let v: int | string | bool = "x"
    v is int | bool
}

test "is_or_pattern_none_match" {
    assert.is_true(baml.deep_equals(is_or_pattern_none_match_fn(), false))
}

function is_or_pattern_with_literals_fn() -> bool {
    let n: int = 2
    n is 1 | 2 | 3
}

test "is_or_pattern_with_literals" {
    assert.is_true(baml.deep_equals(is_or_pattern_with_literals_fn(), true))
}

function is_or_pattern_parenthesised_alternatives_fn() -> bool {
    let v: int | string = 1
    v is (int) | (string)
}

test "is_or_pattern_parenthesised_alternatives" {
    assert.is_true(baml.deep_equals(is_or_pattern_parenthesised_alternatives_fn(), true))
}

// ── Group D: binding patterns ─────────────────────────────────────────────────

function is_bare_binding_pattern_matches_fn() -> bool {
    let v: int = 7
    v is let _
}

test "is_bare_binding_pattern_matches" {
    assert.is_true(baml.deep_equals(is_bare_binding_pattern_matches_fn(), true))
}

function is_typed_binding_pattern_matches_fn() -> bool {
    let v: int | string = 7
    v is let x: int
}

test "is_typed_binding_pattern_matches" {
    assert.is_true(baml.deep_equals(is_typed_binding_pattern_matches_fn(), true))
}

function is_typed_binding_pattern_mismatch_fn() -> bool {
    let v: int | string = "hi"
    v is let x: int
}

test "is_typed_binding_pattern_mismatch" {
    assert.is_true(baml.deep_equals(is_typed_binding_pattern_mismatch_fn(), false))
}

// ── Group E: complex scrutinee expressions ────────────────────────────────────

function get_value() -> int | string { 42 }

function is_with_call_scrutinee_fn() -> bool {
    get_value() is int
}

test "is_with_call_scrutinee" {
    assert.is_true(baml.deep_equals(is_with_call_scrutinee_fn(), true))
}

function is_with_field_access_scrutinee_fn() -> bool {
    let w = Wrap { value: 7 }
    w.value is int
}

test "is_with_field_access_scrutinee" {
    assert.is_true(baml.deep_equals(is_with_field_access_scrutinee_fn(), true))
}

function is_with_optional_chain_scrutinee_fn() -> bool {
    let w: WrapOpt? = null;
    (w?.value) is null
}

test "is_with_optional_chain_scrutinee" {
    assert.is_true(baml.deep_equals(is_with_optional_chain_scrutinee_fn(), true))
}

function is_with_parenthesised_arithmetic_scrutinee_fn() -> bool {
    (1 + 2) is int
}

test "is_with_parenthesised_arithmetic_scrutinee" {
    assert.is_true(baml.deep_equals(is_with_parenthesised_arithmetic_scrutinee_fn(), true))
}

// ── Group F: precedence interactions ──────────────────────────────────────────

function is_chained_with_and_fn() -> bool {
    let a: int | string = 1
    let b: int | string = 2
    a is int && b is int
}

test "is_chained_with_and" {
    assert.is_true(baml.deep_equals(is_chained_with_and_fn(), true))
}

function is_chained_with_or_fn() -> bool {
    let a: int | string = "x"
    let b: int | string = 2
    a is int || b is int
}

test "is_chained_with_or" {
    assert.is_true(baml.deep_equals(is_chained_with_or_fn(), true))
}

function is_compared_with_eq_binds_tighter_fn() -> bool {
    let v: int | string = 1;
    (v is int) == true
}

test "is_compared_with_eq_binds_tighter" {
    assert.is_true(baml.deep_equals(is_compared_with_eq_binds_tighter_fn(), true))
}

function is_chained_on_itself_fn() -> bool {
    let v: int | string = 1;
    (v is int) is bool
}

test "is_chained_on_itself" {
    assert.is_true(baml.deep_equals(is_chained_on_itself_fn(), true))
}

function is_chained_on_itself_without_parens_is_left_associative_fn() -> bool {
    let v: int | string = 1
    v is int is bool
}

test "is_chained_on_itself_without_parens_is_left_associative" {
    assert.is_true(baml.deep_equals(is_chained_on_itself_without_parens_is_left_associative_fn(), true))
}

function is_chained_on_itself_without_parens_negative_outer_fn() -> bool {
    let v: int | string = 1
    v is string is bool
}

test "is_chained_on_itself_without_parens_negative_outer" {
    assert.is_true(baml.deep_equals(is_chained_on_itself_without_parens_negative_outer_fn(), true))
}

function is_in_if_condition_fn() -> string {
    let v: int | string = 7
    if (v is int) { "number" } else { "text" }
}

test "is_in_if_condition" {
    assert.is_true(baml.deep_equals(is_in_if_condition_fn(), "number"))
}

// ── Group G: use in unusual positions ────────────────────────────────────────

function is_inside_array_literal_fn() -> bool[] {
    let v: int | string = 1;
    [v is int, v is string]
}

test "is_inside_array_literal" {
    assert.is_true(baml.deep_equals(is_inside_array_literal_fn(), [true, false]))
}

function is_inside_lambda_body_fn() -> bool {
    let check = (v: int | string) -> bool { v is int }
    check(42)
}

test "is_inside_lambda_body" {
    assert.is_true(baml.deep_equals(is_inside_lambda_body_fn(), true))
}

function is_int_param(v: int | string) -> bool {
    return v is int
}

function is_as_return_value_fn() -> bool {
    is_int_param(7)
}

test "is_as_return_value" {
    assert.is_true(baml.deep_equals(is_as_return_value_fn(), true))
}

function is_in_match_guard_fn() -> int {
    let v: int | string = 5
    match (v) {
        let n: int if n is int => n + 1,
        _ => 0
    }
}

test "is_in_match_guard" {
    assert.is_true(baml.deep_equals(is_in_match_guard_fn(), 6))
}

function is_in_let_initializer_fn() -> bool {
    let v: int | string = 7
    let result: bool = v is int
    result
}

test "is_in_let_initializer" {
    assert.is_true(baml.deep_equals(is_in_let_initializer_fn(), true))
}

function is_inside_while_condition_fn() -> int {
    let count = 0
    let v: int | string = 0
    while (v is int && count < 3) {
        count = count + 1
        if (count == 3) {
            v = "done"
        }
    }
    count
}

test "is_inside_while_condition" {
    assert.is_true(baml.deep_equals(is_inside_while_condition_fn(), 3))
}

// ── Group H: scrutinee evaluated exactly once ─────────────────────────────────

function is_evaluates_scrutinee_exactly_once_fn() -> int {
    let count = 0
    let get = () -> int {
        count += 1
        42
    }
    let _ = get() is int | string | bool
    count
}

test "is_evaluates_scrutinee_exactly_once" {
    assert.is_true(baml.deep_equals(is_evaluates_scrutinee_exactly_once_fn(), 1))
}

// ── Group J: type narrowing in if/else branches ───────────────────────────────

function narrowing_then_branch_picks_pattern_type_fn() -> int {
    let v: int | string = 7
    if (v is int) { v + 1 } else { 0 }
}

test "narrowing_then_branch_picks_pattern_type" {
    assert.is_true(baml.deep_equals(narrowing_then_branch_picks_pattern_type_fn(), 8))
}

function describe_int_string_bool(v: int | string | bool) -> string {
    if (v is int) { "got int" }
    else if (v is bool) { "got bool" }
    else { "got string" }
}

function narrowing_then_branch_picks_or_pattern_type_fn() -> string {
    describe_int_string_bool(true)
}

test "narrowing_then_branch_picks_or_pattern_type" {
    assert.is_true(baml.deep_equals(narrowing_then_branch_picks_or_pattern_type_fn(), "got bool"))
}

function narrowing_else_branch_runs_for_non_matching_value_fn() -> string {
    let v: int | string = "hi"
    if (v is int) { "number" } else { "text" }
}

test "narrowing_else_branch_runs_for_non_matching_value" {
    assert.is_true(baml.deep_equals(narrowing_else_branch_runs_for_non_matching_value_fn(), "text"))
}

function narrowing_else_branch_subtracts_matched_type_fn() -> int {
    let v: int | string = "hi"
    if (v is int) { 0 } else { v.length() }
}

test "narrowing_else_branch_subtracts_matched_type" {
    assert.is_true(baml.deep_equals(narrowing_else_branch_subtracts_matched_type_fn(), 2))
}

function narrowing_else_branch_subtracts_or_pattern_fn() -> int {
    let v: int | string | bool = "hello"
    if (v is int | bool) { 0 } else { v.length() }
}

test "narrowing_else_branch_subtracts_or_pattern" {
    assert.is_true(baml.deep_equals(narrowing_else_branch_subtracts_or_pattern_fn(), 5))
}

function describe_success_failure(v: Success | Failure) -> string {
    if (v is Success) { v.data } else { v.reason }
}

function narrowing_else_branch_with_class_types_fn() -> string {
    describe_success_failure(Failure { reason: "oops" })
}

test "narrowing_else_branch_with_class_types" {
    assert.is_true(baml.deep_equals(narrowing_else_branch_with_class_types_fn(), "oops"))
}

function narrowing_else_branch_with_literal_pattern_keeps_original_fn() -> int {
    let n: int = 5
    if (n is 0) { 0 } else { n + 1 }
}

test "narrowing_else_branch_with_literal_pattern_keeps_original" {
    assert.is_true(baml.deep_equals(narrowing_else_branch_with_literal_pattern_keeps_original_fn(), 6))
}

function narrowing_negation_subtracts_in_then_branch_fn() -> int {
    let v: int | string = "abc"
    if (!(v is int)) { v.length() } else { 0 }
}

test "narrowing_negation_subtracts_in_then_branch" {
    assert.is_true(baml.deep_equals(narrowing_negation_subtracts_in_then_branch_fn(), 3))
}

function narrowing_through_negation_fn() -> int {
    let v: int | string = 5
    if (!(v is int)) { 0 } else { v + 100 }
}

test "narrowing_through_negation" {
    assert.is_true(baml.deep_equals(narrowing_through_negation_fn(), 105))
}

function process(v: int | string) -> int {
    if (v is int) { v * 3 } else { 0 }
}

function narrowing_works_on_function_parameter_fn() -> int {
    process(7)
}

test "narrowing_works_on_function_parameter" {
    assert.is_true(baml.deep_equals(narrowing_works_on_function_parameter_fn(), 21))
}

function describe_success_or_failed(v: Success | Failure) -> string {
    if (v is Success) { v.data } else { "failed" }
}

function narrowing_picks_class_type_in_then_branch_fn() -> string {
    describe_success_or_failed(Success { data: "ok" })
}

test "narrowing_picks_class_type_in_then_branch" {
    assert.is_true(baml.deep_equals(narrowing_picks_class_type_in_then_branch_fn(), "ok"))
}

function narrowing_persists_across_multiple_uses_in_then_fn() -> int {
    let v: int | string = 4
    if (v is int) {
        let a = v + 1
        let b = v + 2
        a + b
    } else {
        0
    }
}

test "narrowing_persists_across_multiple_uses_in_then" {
    assert.is_true(baml.deep_equals(narrowing_persists_across_multiple_uses_in_then_fn(), 11))
}

function narrowing_skipped_for_non_path_scrutinee_fn() -> string {
    let w = Wrap { value: 7 }
    if (w.value is int) { "int" } else { "text" }
}

test "narrowing_skipped_for_non_path_scrutinee" {
    assert.is_true(baml.deep_equals(narrowing_skipped_for_non_path_scrutinee_fn(), "int"))
}

function narrowing_with_early_return_fn() -> int {
    let v: int | string = 7
    if (v is string) { return 0 }
    if (v is int) { v * 2 } else { -1 }
}

test "narrowing_with_early_return" {
    assert.is_true(baml.deep_equals(narrowing_with_early_return_fn(), 14))
}

function is_chained_negation_and_or_fn() -> bool {
    let a: int | string = "x"
    let b: int | string = 2
    !(a is int) || (b is int)
}

test "is_chained_negation_and_or" {
    assert.is_true(baml.deep_equals(is_chained_negation_and_or_fn(), true))
}
