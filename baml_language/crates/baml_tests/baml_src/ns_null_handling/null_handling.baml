// Tests for null handling — optional chaining, safe assignment, null coalescing.
//
// Converted from crates/baml_tests/tests/null_handling.rs.
//
// All test bodies use helper functions with parameters (the Rust tests do the
// same, to prevent the compiler from narrowing nullable types at compile time).
// Results are declared as optional types.

class NhProfile {
    name string
    address string?
    scores int[]?
    settings map<string, string>?
}

class NhUser {
    id int
    profile NhProfile?
}

class NhNode {
    value int
    next NhNode?
    children NhNode[]?
    labels map<string, NhNode>?
}

// ============================================================================
// 1. LHS safe assignment: skip-vs-write
// ============================================================================

function nh_safe_assign_null(user: NhUser?) -> string? {
    user?.profile?.name = "updated";
    user?.profile?.name
}

function nh_run_safe_assign_null() -> string? {
    nh_safe_assign_null(null)
}

test "safe_assign_null_skips" {
    assert.equal(nh_run_safe_assign_null(), null)
}

function nh_safe_assign_non_null(user: NhUser?) -> string? {
    user?.profile?.name = "updated";
    user?.profile?.name
}

function nh_run_safe_assign_non_null() -> string? {
    nh_safe_assign_non_null(NhUser {
        id: 1,
        profile: NhProfile { name: "old", address: null, scores: null, settings: null },
    })
}

test "safe_assign_non_null_writes" {
    assert.equal(nh_run_safe_assign_non_null(), "updated")
}

function nh_compound_assign_null(user: NhUser?) -> int? {
    user?.id += 1;
    user?.id
}

function nh_run_compound_assign_null() -> int? {
    nh_compound_assign_null(null)
}

test "compound_assign_null_skips" {
    assert.equal(nh_run_compound_assign_null(), null)
}

function nh_compound_assign_non_null(user: NhUser?) -> int? {
    user?.id += 1;
    user?.id
}

function nh_run_compound_assign_non_null() -> int? {
    nh_compound_assign_non_null(NhUser { id: 10, profile: null })
}

test "compound_assign_non_null_increments" {
    assert.equal(nh_run_compound_assign_non_null(), 11)
}

function nh_safe_assign_index_null(items: int[]?) -> int? {
    items?.[0] = 42;
    items?.[0]
}

function nh_run_safe_assign_index_null() -> int? {
    nh_safe_assign_index_null(null)
}

test "safe_assign_index_null_skips" {
    assert.equal(nh_run_safe_assign_index_null(), null)
}

function nh_safe_assign_index_non_null(items: int[]?) -> int? {
    items?.[0] = 42;
    items?.[0]
}

function nh_run_safe_assign_index_non_null() -> int? {
    nh_safe_assign_index_non_null([0, 1, 2])
}

test "safe_assign_index_non_null_writes" {
    assert.equal(nh_run_safe_assign_index_non_null(), 42)
}

// ============================================================================
// 2. ?? lazy RHS evaluation
// ============================================================================

function nh_null_coalesce_lazy_non_null(value: int?) -> int {
    let counter = 0;
    let bump = () -> int { counter += 1; counter };
    let result = value ?? bump();
    // result should be 42 (LHS), counter should still be 0 (RHS never called)
    result * 10 + counter
}

function nh_run_null_coalesce_lazy_non_null() -> int {
    nh_null_coalesce_lazy_non_null(42)
}

test "null_coalesce_lazy_rhs_skips_side_effects" {
    assert.equal(nh_run_null_coalesce_lazy_non_null(), 420)
}

function nh_null_coalesce_null_lhs(value: int?) -> int {
    let counter = 0;
    let bump = () -> int { counter += 1; counter };
    let result = value ?? bump();
    // result should be 1 (from bump), counter should be 1
    result * 10 + counter
}

function nh_run_null_coalesce_null_lhs() -> int {
    nh_null_coalesce_null_lhs(null)
}

test "null_coalesce_null_lhs_evaluates_rhs" {
    assert.equal(nh_run_null_coalesce_null_lhs(), 11)
}

// ============================================================================
// 3. Combined: compound assign through mixed chain with ??-provided fallback
// ============================================================================

function nh_combined_rhs_null(node: NhNode?, items: int[]?) -> int? {
    node?.value += items?.[0] ?? 1;
    node?.value
}

function nh_run_combined_rhs_null() -> int? {
    nh_combined_rhs_null(
        NhNode { value: 10, next: null, children: null, labels: null },
        null,
    )
}

test "combined_compound_assign_rhs_null_uses_fallback" {
    assert.equal(nh_run_combined_rhs_null(), 11)
}

function nh_combined_lhs_null(node: NhNode?, items: int[]?) -> int? {
    node?.value += items?.[0] ?? 1;
    node?.value
}

function nh_run_combined_lhs_null() -> int? {
    nh_combined_lhs_null(null, [5])
}

test "combined_compound_assign_lhs_null_skips" {
    assert.equal(nh_run_combined_lhs_null(), null)
}

// Regression (CodeRabbit): optional member access on a MULTI-member nullable
// union `(A | B)?.field` must strip null from the receiver and route through
// the union (not a single class).
// A shared member accessed on a union (`(NhCat | NhDog).sound`) is reachable
// only through an interface both arms implement.
interface NhHasSound { sound string }

class NhCat {
    sound string
    implements NhHasSound {}
}

class NhDog {
    sound string
    implements NhHasSound {}
}

function nh_multi_union_optional_access(animal: (NhCat | NhDog)?) -> string? {
    animal?.sound
}

function nh_run_multi_union_optional_access() -> string? {
    nh_multi_union_optional_access(NhCat { sound: "meow" })
}

test "multi_union_optional_access" {
    assert.equal(nh_run_multi_union_optional_access(), "meow")
}
