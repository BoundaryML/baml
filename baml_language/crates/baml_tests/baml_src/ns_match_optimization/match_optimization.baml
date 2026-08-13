// Converted from crates/baml_tests/tests/match_optimization.rs.
//
// All tests assert runtime values (output.result) as well as bytecode shape.
// The bytecode snapshot is captured by the per-namespace snapshot; the
// inline insta::assert_snapshot! calls are dropped here.
//
// Tests that used a `function main()` have their body inlined in the test
// block. Tests that used a named `classify(x: int)` function with explicit
// args use a uniquely-named top-level helper so function names don't collide.

// ============================================================================
// Jump Table Tests (4+ dense arms trigger jump table optimization)
// ============================================================================

test "match_jump_table_first_arm" {
    let x = 0;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 100)
}

test "match_jump_table_middle_arm" {
    let x = 2;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 102)
}

test "match_jump_table_last_arm" {
    let x = 3;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 103)
}

test "match_jump_table_fallback" {
    let x = 10;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 999)
}

test "match_jump_table_negative_fallback" {
    let x = -1;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 999)
}

test "match_jump_table_with_holes_miss" {
    // 4 values in range of 7: density ~57% (above 50% threshold)
    // Hole at value 1 should fall through to default
    let x = 1;
    assert.equal(match (x) { 0 => 100, 2 => 102, 4 => 104, 6 => 106, _ => 999 }, 999)
}

test "match_jump_table_with_holes_hit" {
    let x = 4;
    assert.equal(match (x) { 0 => 100, 2 => 102, 4 => 104, 6 => 106, _ => 999 }, 104)
}

test "match_jump_table_offset_values" {
    // Jump table with non-zero base offset
    let x = 11;
    assert.equal(match (x) { 10 => 110, 11 => 111, 12 => 112, 13 => 113, _ => 999 }, 111)
}

test "match_jump_table_large" {
    // 8 consecutive values - should still use jump table
    let x = 5;
    assert.equal(
        match (x) { 0 => 1000, 1 => 1001, 2 => 1002, 3 => 1003, 4 => 1004, 5 => 1005, 6 => 1006, 7 => 1007, _ => 9999 },
        1005,
    )
}

// Tests that a match with 4 dense consecutive integer arms uses a jump table
// with a function parameter (non-constant scrutinee).
function classify_jump_table_dense_four_arms_param(x: int) -> int {
    match (x) {
        0 => 100,
        1 => 101,
        2 => 102,
        3 => 103,
        _ => 999
    }
}

test "match_jump_table_dense_four_arms_param" {
    assert.equal(classify_jump_table_dense_four_arms_param(2), 102)
}

// ============================================================================
// Binary Search Tests (4+ sparse arms trigger binary search optimization)
// ============================================================================

test "match_binary_search_first_arm" {
    let x = 0;
    assert.equal(match (x) { 0 => 100, 30 => 130, 60 => 160, 99 => 199, _ => 999 }, 100)
}

test "match_binary_search_middle_arm" {
    let x = 60;
    assert.equal(match (x) { 0 => 100, 30 => 130, 60 => 160, 99 => 199, _ => 999 }, 160)
}

test "match_binary_search_last_arm" {
    let x = 99;
    assert.equal(match (x) { 0 => 100, 30 => 130, 60 => 160, 99 => 199, _ => 999 }, 199)
}

test "match_binary_search_fallback" {
    let x = 50;
    assert.equal(match (x) { 0 => 100, 30 => 130, 60 => 160, 99 => 199, _ => 999 }, 999)
}

test "match_binary_search_very_sparse" {
    let x = 500;
    assert.equal(
        match (x) { 0 => 1000, 100 => 1100, 200 => 1200, 300 => 1300, 400 => 1400, 500 => 1500, _ => 9999 },
        1500,
    )
}

test "match_binary_search_large_values" {
    let x = 750;
    assert.equal(
        match (x) { 250 => 1250, 500 => 1500, 750 => 1750, 1000 => 2000, 1250 => 2250, _ => 9999 },
        1750,
    )
}

// Binary search with function parameter (non-constant scrutinee).
function classify_binary_search_sparse_four_arms_param(x: int) -> int {
    match (x) {
        0 => 100,
        30 => 130,
        60 => 160,
        99 => 199,
        _ => 999
    }
}

test "match_binary_search_sparse_four_arms_param" {
    assert.equal(classify_binary_search_sparse_four_arms_param(30), 130)
}

// ============================================================================
// If-Else Chain Tests (< 4 arms)
// ============================================================================

// Tests that a match with fewer than 4 integer arms uses if-else chain.
function classify_if_else_chain_three_arms(x: int) -> int {
    match (x) {
        0 => 100,
        1 => 101,
        _ => 999
    }
}

test "match_if_else_chain_three_arms" {
    assert.equal(classify_if_else_chain_three_arms(0), 100)
}

// ============================================================================
// Density Threshold Tests (boundary at 50%)
// ============================================================================

test "match_density_exactly_50_percent" {
    // 4 arms in range of 8: 0, 2, 4, 6 = 50% density (should use jump table)
    let x = 4;
    assert.equal(match (x) { 0 => 100, 2 => 102, 4 => 104, 6 => 106, _ => 999 }, 104)
}

test "match_density_below_50_percent" {
    // 4 arms in range of 10: 0, 3, 6, 9 = 40% density (should use binary search)
    let x = 6;
    assert.equal(match (x) { 0 => 100, 3 => 103, 6 => 106, 9 => 109, _ => 999 }, 106)
}

test "match_density_above_50_percent" {
    // 5 arms in range of 6: 0, 1, 2, 4, 5 = 83% density (should use jump table)
    let x = 4;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 4 => 104, 5 => 105, _ => 999 }, 104)
}

// ============================================================================
// Large Range Integer Values
// ============================================================================

test "match_large_range_dense" {
    // Dense values starting from offset (100-103)
    let x = 102;
    assert.equal(match (x) { 100 => 1000, 101 => 1001, 102 => 1002, 103 => 1003, _ => 9999 }, 1002)
}

test "match_large_range_sparse" {
    // Sparse values with large gaps
    let x = 1000;
    assert.equal(match (x) { 0 => 1, 500 => 2, 1000 => 3, 1500 => 4, _ => 9999 }, 3)
}

test "match_zero_in_range" {
    let x = 0;
    assert.equal(match (x) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 100)
}

// ============================================================================
// Negative Integer Jump Table Tests
// ============================================================================

function classify_negative_jump_table(x: int) -> string {
    match (x) {
        -3 => "neg three",
        -2 => "neg two",
        -1 => "neg one",
        0 => "zero",
        _ => "other"
    }
}

test "match_negative_jump_table" {
    // Dense negative range should use jump table: -3, -2, -1, 0
    assert.equal(classify_negative_jump_table(-2), "neg two")
}

test "match_negative_jump_table_fallback" {
    assert.equal(classify_negative_jump_table(5), "other")
}

// ============================================================================
// Mixed Positive/Negative Spanning Zero Tests
// ============================================================================

function classify_spanning_zero(x: int) -> string {
    match (x) {
        -2 => "neg two",
        -1 => "neg one",
        0 => "zero",
        1 => "one",
        2 => "two",
        _ => "other"
    }
}

test "match_spanning_zero_jump_table" {
    assert.equal(classify_spanning_zero(1), "one")
}

test "match_spanning_zero_negative_hit" {
    assert.equal(classify_spanning_zero(-1), "neg one")
}

// ============================================================================
// Binary Search with Negative Values
// ============================================================================

function classify_binary_search_negative_sparse(x: int) -> string {
    match (x) {
        -100 => "a",
        -50 => "b",
        -10 => "c",
        -1 => "d",
        _ => "other"
    }
}

test "match_binary_search_negative_sparse" {
    assert.equal(classify_binary_search_negative_sparse(-50), "b")
}

function classify_binary_search_spanning_zero_sparse(x: int) -> string {
    match (x) {
        -100 => "neg hundred",
        -1 => "neg one",
        1 => "one",
        100 => "hundred",
        _ => "other"
    }
}

test "match_binary_search_spanning_zero_sparse" {
    assert.equal(classify_binary_search_spanning_zero_sparse(1), "one")
}

// ============================================================================
// Union Patterns Aggregating to 4+ Arms Tests
// ============================================================================

// Union patterns that aggregate to 4+ total values should use jump table.
function classify_union_aggregated_jump_table(x: int) -> string {
    match (x) {
        0 | 1 => "a",
        2 | 3 => "b",
        4 | 5 => "c",
        6 | 7 => "d",
        _ => "other"
    }
}

test "match_union_aggregated_jump_table" {
    assert.equal(classify_union_aggregated_jump_table(5), "c")
}

// ============================================================================
// Range Limit Boundary Tests (256)
// ============================================================================

// Dense 4 values at high range (252-255) should use jump table.
function classify_range_at_limit_uses_jump_table(x: int) -> int {
    match (x) {
        252 => 1,
        253 => 2,
        254 => 3,
        255 => 4,
        _ => 0
    }
}

test "match_range_at_limit_uses_jump_table" {
    assert.equal(classify_range_at_limit_uses_jump_table(254), 3)
}

// Values spanning more than 256 range should NOT use jump table.
function classify_range_exceeds_limit_uses_binary_search(x: int) -> int {
    match (x) {
        0 => 1,
        100 => 2,
        200 => 3,
        300 => 4,
        _ => 0
    }
}

test "match_range_exceeds_limit_uses_binary_search" {
    assert.equal(classify_range_exceeds_limit_uses_binary_search(200), 3)
}

// ============================================================================
// Large Jump Table Tests (10+ arms)
// ============================================================================

// Large dense integer match with 10 arms should use jump table.
function classify_large_jump_table(x: int) -> int {
    match (x) {
        0 => 100,
        1 => 101,
        2 => 102,
        3 => 103,
        4 => 104,
        5 => 105,
        6 => 106,
        7 => 107,
        8 => 108,
        9 => 109,
        _ => 999
    }
}

test "match_large_jump_table" {
    assert.equal(classify_large_jump_table(7), 107)
}

// ============================================================================
// Large Binary Search Tree (8 sparse arms)
// ============================================================================

function classify_binary_search_eight_arms(x: int) -> int {
    match (x) {
        0 => 0,
        10 => 1,
        20 => 2,
        30 => 3,
        40 => 4,
        50 => 5,
        60 => 6,
        70 => 7,
        _ => 99
    }
}

test "match_binary_search_eight_arms" {
    assert.equal(classify_binary_search_eight_arms(50), 5)
}
