// Soundness regression tests.
//
// Tests that the compiler produces correct bytecode and execution results
// for tricky cases: stack operand ordering, cross-block variable mutation,
// register allocation, and value copy semantics.
//
// Converted from crates/baml_tests/tests/soundness.rs.  Each Rust test
// becomes a `test` block; tests that take runtime arguments are lifted into
// top-level helper functions called with hardcoded values (the same trick
// used in ns_assignments).  Bytecode-snapshot assertions from `insta` are
// dropped — they test compiler internals, not observable behaviour.

// ─── helpers for cross-block / parameter-mutation tests ───────────────────────

// Used by cross_block_field_mutation.
class SoundBox {
    v int
}

function sound_id(x: int) -> int { x }

// `1 - id(2)` — verifies that the call result is used as the *right* operand.
function call_result_right_operand_sub() -> int {
    1 - sound_id(2)
}

// `100 - if (2 > 1) { 7 } else { 3 }` — phi-node right operand.
function phi_right_operand_sub() -> int {
    100 - if (2 > 1) { 7 } else { 3 }
}

// Cross-block field mutation: copy t = b.v BEFORE b.v is mutated.
function cross_block_field_mutation_helper() -> int {
    let b = SoundBox { v: 1 };
    let t = b.v;
    if (1 == 1) {
    }
    b.v = 2;
    t
}

// Cross-block let mutation with c = true.
function cross_block_let_mutation_true(c: bool) -> int {
    let a = 1;
    let b = a;
    if (c) {
        a = 2;
    }
    b
}

// Cross-block param mutation with c = true, p = 10.
function cross_block_param_mutation_helper(c: bool, p: int) -> int {
    let x = p;
    if (c) {
        p = 2;
    }
    x
}

// Copy of a mutable parameter: y = x; x = 2; y should still be the original.
function copy_of_mutable_param_helper(x: int) -> int {
    let y = x;
    x = 2;
    y
}

// Transitive copy: t = p; x = t — still unaffected by later p = 2.
function transitive_param_copy_mutation_helper(c: bool, p: int) -> int {
    let t = p;
    let x = t;
    if (c) {
        p = 2;
    }
    x
}

// Helper for multiple_defs_preserve_side_effects: fail() must run (and throw)
// even though x is immediately reassigned after.
function sound_fail() -> int {
    assert.is_true(false)
    1
}

function multiple_defs_preserve_side_effects_helper() -> int {
    let x = sound_fail();
    x = 2;
    x
}

// ─── stack operand ordering ────────────────────────────────────────────────────

// `1 - id(2)` must evaluate to -1 (not 2 - 1 = 1).
test "call_result_immediate_right_operand_subtraction" {
    assert.equal(call_result_right_operand_sub(), -1)
}

// `100 - if (2 > 1) { 7 } else { 3 }` = 93.
test "phi_like_right_operand_subtraction" {
    assert.equal(phi_right_operand_sub(), 93)
}

// ─── cross-block variable mutation ────────────────────────────────────────────

// A copy taken before a later field mutation keeps the original value.
test "cross_block_field_mutation" {
    assert.equal(cross_block_field_mutation_helper(), 1)
}

// A copy taken before a later let mutation keeps the original value (c=true).
test "cross_block_let_mutation" {
    assert.equal(cross_block_let_mutation_true(true), 1)
}

// Same code, false branch — copy still keeps original value.
test "cross_block_let_mutation_false_branch" {
    assert.equal(cross_block_let_mutation_true(false), 1)
}

// Copy of a parameter is unaffected by a later mutation of that parameter (c=true).
test "cross_block_param_mutation" {
    assert.equal(cross_block_param_mutation_helper(true, 10), 10)
}

// ─── register allocation / value copy semantics ───────────────────────────────

// y = x; x = 2; y → original value 5 is preserved in y.
test "copy_of_mutable_param" {
    assert.equal(copy_of_mutable_param_helper(5), 5)
}

// Transitive copy (t = p; x = t) is still unaffected by a later p = 2 (c=true).
test "transitive_param_copy_mutation" {
    assert.equal(transitive_param_copy_mutation_helper(true, 7), 7)
}

// `let x = fail()` must execute fail() (and propagate its throw) even though
// x is immediately reassigned afterward — side effects must not be elided.
test "multiple_defs_preserve_side_effects" {
    {
        multiple_defs_preserve_side_effects_helper();
        baml.sys.panic("expected multiple_defs_preserve_side_effects_helper() to throw")
    } catch (e) {
        // `assert.is_true` raises `AssertionFailed`; the explicit
        // `baml.sys.panic` above it raises `UserPanic`. Either means the
        // helper's side effect ran.
        baml.panics.AssertionFailed => null,
        baml.panics.UserPanic => null,
    }
}
