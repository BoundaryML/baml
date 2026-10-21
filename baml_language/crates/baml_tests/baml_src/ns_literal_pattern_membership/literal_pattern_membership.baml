// End-to-end tests for literal patterns as *membership* tests (B-1073).
//
// Matching `1` asks whether the value inhabits the singleton type `{1}`. The
// type system holds `int`, `float` and `bigint` disjoint (TYPE_SYSTEM.md,
// "Concrete Types":
// no concrete type is a subtype of another), so no float and no bigint can
// inhabit an int literal. The `==` operator deliberately widens across all
// three -- that is what makes `1 + 1.0` work -- and literal patterns used to
// lower onto it, so `x is 1` fired for `1.0`.
//
// The same construct has four lowerings, and the bug was that they disagreed:
//
//   * the `is` operator and any guarded match arm -- `BinOp::Eq`, which widened
//   * a sparse int match -- the `SwitchKind::Integer` if-else chain, correct
//   * a dense int match (>=4 arms) -- a `JumpTable`, which read the scrutinee
//     as a raw int and raised a VM type error instead of falling through
//   * `x is One` for `type One = 1` -- `IsType` against the literal's *base*
//     type tag, which admitted every int
//
// All four now go through one relation, so each is exercised below against the
// same value.

// --- 1. the `is` operator ----------------------------------------------------

function via_is(x: int | float) -> bool { x is 1 }

test "is_matches_int_one" { assert.is_true(via_is(1)) }
test "is_rejects_float_one" { assert.equal(via_is(1.0), false) }
test "is_rejects_other_int" { assert.equal(via_is(2), false) }

// --- 2. a guarded match arm (any guard disqualifies the jump table) ----------

function via_guarded_match(x: int | float) -> bool {
    match (x) {
        1 if true => true
        let _ => false
    }
}

test "guarded_match_matches_int_one" { assert.is_true(via_guarded_match(1)) }
test "guarded_match_rejects_float_one" { assert.equal(via_guarded_match(1.0), false) }

// --- 3. a sparse int match (the if-else chain) -------------------------------

function via_sparse_match(x: int | float) -> int {
    match (x) {
        1 => 1
        2 => 2
        let _ => 0
    }
}

test "sparse_match_matches_int_one" { assert.equal(via_sparse_match(1), 1) }
test "sparse_match_rejects_float_one" { assert.equal(via_sparse_match(1.0), 0) }

// --- 4. a dense int match (the jump table) -----------------------------------
//
// A non-int scrutinee reaching an integer switch is a match *failure*, not a
// broken invariant: it belongs to no arm, so it falls to `otherwise`. Before
// the int-tag guard this aborted the whole match with a VM type error.

function via_jump_table(x: int | float) -> int {
    match (x) {
        1 => 1
        2 => 2
        3 => 3
        4 => 4
        let _ => 0
    }
}

test "jump_table_matches_int_one" { assert.equal(via_jump_table(1), 1) }
test "jump_table_matches_int_four" { assert.equal(via_jump_table(4), 4) }
test "jump_table_falls_through_on_float" { assert.equal(via_jump_table(1.0), 0) }
test "jump_table_falls_through_on_unlisted_int" { assert.equal(via_jump_table(9), 0) }

// --- 5. a literal reached through an alias (the `IsType` path) ---------------

type One = 1

function via_alias(x: int) -> bool { x is One }

test "alias_matches_int_one" { assert.is_true(via_alias(1)) }
test "alias_rejects_other_int" { assert.equal(via_alias(2), false) }

// --- 6. the rest of the tower, and the non-numeric literals ------------------
//
// bigint and int are disjoint in both directions; bool is not an int however
// it is represented; string literals compare by contents.

function bigint_pattern(x: int | bigint) -> bool { x is 1n }
function int_pattern_over_bigint(x: int | bigint) -> bool { x is 1 }
function bool_pattern(x: int | bool) -> bool { x is true }
function int_pattern_over_bool(x: int | bool) -> bool { x is 1 }

function string_pattern(x: string) -> int {
    match (x) {
        "go" => 1
        "stop" => 2
        let _ => 0
    }
}

test "bigint_literal_matches_bigint" { assert.is_true(bigint_pattern(1n)) }
test "bigint_literal_rejects_int" { assert.equal(bigint_pattern(1), false) }
test "int_literal_matches_int" { assert.is_true(int_pattern_over_bigint(1)) }
test "int_literal_rejects_bigint" { assert.equal(int_pattern_over_bigint(1n), false) }
test "bool_literal_matches_bool" { assert.is_true(bool_pattern(true)) }
test "bool_literal_rejects_int" { assert.equal(bool_pattern(1), false) }
test "int_literal_rejects_bool" { assert.equal(int_pattern_over_bool(true), false) }
test "string_literal_matches_contents" { assert.equal(string_pattern("go"), 1) }
test "string_literal_rejects_other" { assert.equal(string_pattern("nope"), 0) }
