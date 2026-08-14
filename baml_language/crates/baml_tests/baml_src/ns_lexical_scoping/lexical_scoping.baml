// Unified tests for lexical block scope and local shadowing.
//
// Converted from crates/baml_tests/tests/lexical_scoping.rs.
//
// The Rust test `lexical_scoping_runtime_regressions` is a single test that
// calls each helper with `assert_lexical_scope_result`. Here each call becomes
// its own `test` block. All helper functions are top-level (the locals inside
// functions are not boxed, unlike test-block locals).
//
// The `declared_type_restored_across_scope`, `lambdas_capture_match_and_catch_pattern_bindings`,
// `match_and_catch_pattern_bindings_restore_outer_locals`, and
// `multi_clause_catch_uses_clause_local_binding` tests have inline BAML bodies
// also used as-is.

// ─── LEXICAL_SCOPE_RUNTIME_REGRESSIONS helpers ───────────────────────────────

function same_scope_shadow() -> int {
    let x = 1
    let x = 2
    x
}

function outer_restored() -> int {
    let x = 1
    {
        let x = 2
    }
    x
}

function initializer_uses_previous() -> int {
    let x = 1
    let x = x + 1
    x
}

function shadow_param(x: int) -> int {
    let x = x + 1
    x
}

function for_loop_restores_outer() -> int {
    let x = 1
    for (let x in [2, 3]) {
        x
    }
    x
}

function nested_outer_restored() -> int {
    let x = 1
    {
        let x = 2
        {
            let x = 3
            x
        }
        x
    }
    x
}

function capture_before_after_shadow() -> int {
    let x = 1
    let g = () -> int { x }
    let x = 2
    let f = () -> int { x }
    g() * 10 + f()
}

function rule1_while_no_leakage() -> int {
    let x = 1
    let once = true
    while (once) {
        let x = 99
        once = false
    }
    x
}

function rule1_while_observed_inner_shadow() -> int {
    let x = 1
    let once = true
    let observed = 0
    while (once) {
        let x = 99
        observed = observed + x
        once = false
    }
    x + observed
}

function rule2_block_outer_mutation_escapes() -> int {
    let x = 1
    {
        x = 2
    }
    x
}

function rule2_for_outer_mutation_escapes() -> int {
    let x = 1
    for (let _ in [1]) {
        x = 2
    }
    x
}

function rule3_block_shadow_then_assign_inner_does_not_escape() -> int {
    let x = 1
    {
        let x = 2
        x = 3
    }
    x
}

function rule2_pre_shadow_mutation_escapes_post_shadow_does_not() -> int {
    let x = 1
    {
        x = 2
        let x = 3
        x = 4
    }
    x
}

// ─── lexical_scoping_runtime_regressions (individual sub-tests) ──────────────

test "same_scope_shadow" {
    assert.equal(same_scope_shadow(), 2)
}

test "outer_restored" {
    assert.equal(outer_restored(), 1)
}

test "initializer_uses_previous" {
    assert.equal(initializer_uses_previous(), 2)
}

test "shadow_param" {
    assert.equal(shadow_param(4), 5)
}

test "for_loop_restores_outer" {
    assert.equal(for_loop_restores_outer(), 1)
}

test "nested_outer_restored" {
    assert.equal(nested_outer_restored(), 1)
}

test "capture_before_after_shadow" {
    assert.equal(capture_before_after_shadow(), 12)
}

test "rule1_while_no_leakage" {
    assert.equal(rule1_while_no_leakage(), 1)
}

test "rule1_while_observed_inner_shadow" {
    assert.equal(rule1_while_observed_inner_shadow(), 100)
}

test "rule2_block_outer_mutation_escapes" {
    assert.equal(rule2_block_outer_mutation_escapes(), 2)
}

test "rule2_for_outer_mutation_escapes" {
    assert.equal(rule2_for_outer_mutation_escapes(), 2)
}

test "rule3_block_shadow_then_assign_inner_does_not_escape" {
    assert.equal(rule3_block_shadow_then_assign_inner_does_not_escape(), 1)
}

test "rule2_pre_shadow_mutation_escapes_post_shadow_does_not" {
    assert.equal(rule2_pre_shadow_mutation_escapes_post_shadow_does_not(), 2)
}

// ─── declared_type_restored_across_scope ─────────────────────────────────────

function declared_type_restored_outer() -> string {
    let x: string = "outer"
    {
        let x: int = 1
    }
    x
}

test "declared_type_restored_across_scope" {
    assert.equal(declared_type_restored_outer(), "outer")
}

// ─── lambdas_capture_match_and_catch_pattern_bindings ────────────────────────

function throw_string() -> string {
    throw "caught"
}

function capture_match_arm() -> string {
    match ("matched") {
        let s: string => {
            let f = () -> string { s }
            f()
        }
    }
}

function capture_catch_clause_binding() -> string {
    throw_string() catch (e) {
        string => {
            let f = () -> string { e }
            f()
        }
    }
}

function capture_catch_arm_binding() -> string {
    throw_string() catch (e) {
        let s: string => {
            let f = () -> string { s }
            f()
        }
    }
}

function lambdas_capture_main() -> string {
    capture_match_arm() + ":" + capture_catch_clause_binding() + ":" + capture_catch_arm_binding()
}

test "lambdas_capture_match_and_catch_pattern_bindings" {
    assert.equal(lambdas_capture_main(), "matched:caught:caught")
}

// ─── match_and_catch_pattern_bindings_restore_outer_locals ───────────────────

function throw_string_arg(s: string) -> string {
    throw s
}

function match_post_match_restores_outer() -> int {
    let x = 10
    let _matched = match (1) {
        let x: int => x
    }
    x
}

function match_later_arm_uses_outer() -> int {
    let x = 20
    let v: string = "value"
    match (v) {
        let x: "specific" => 0,
        _ => x
    }
}

function catch_base_uses_outer_lambda() -> int {
    let e = () -> string { "base" }
    let caught = throw_string_arg(e()) catch (e) {
        string => e
    }
    match (caught) {
        "base" => 3,
        _ => 0
    }
}

function match_and_catch_main() -> int {
    match_post_match_restores_outer()
        + match_later_arm_uses_outer() * 100
        + catch_base_uses_outer_lambda() * 10000
}

test "match_and_catch_pattern_bindings_restore_outer_locals" {
    assert.equal(match_and_catch_main(), 32010)
}

// ─── multi_clause_catch_uses_clause_local_binding ────────────────────────────

function fail() -> int {
    throw 7
}

function multi_clause_catch_main() -> int {
    fail() catch (first) {
        string => 1
    } catch (second) {
        int => second
    }
}

test "multi_clause_catch_uses_clause_local_binding" {
    assert.equal(multi_clause_catch_main(), 7)
}

function closure_parameter_in_for_iterable() -> int {
    let sum = 0
    for (let n in [1, 2, 3, 4].filter((x: int) -> bool { x > 2 })) {
        sum += n
    }
    sum
}

test "closure_parameter_resolves_in_for_iterable" {
    assert.equal(closure_parameter_in_for_iterable(), 7)
}

function closure_parameter_in_while_condition() -> int {
    let i = 0
    while (((x: int) -> bool { x < 3 })(i)) {
        i += 1
    }
    i
}

test "closure_parameter_resolves_in_while_condition" {
    assert.equal(closure_parameter_in_while_condition(), 3)
}
