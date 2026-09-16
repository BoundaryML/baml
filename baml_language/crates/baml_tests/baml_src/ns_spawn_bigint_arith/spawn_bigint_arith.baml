// Bigint arithmetic evaluated inline inside an anonymous `spawn { ... }`
// closure body.
//
// The emitter deliberately de-specializes binary ops whose operands derive
// from spawn captures (`binary_operands_can_use_specialized_op` in
// baml_compiler2_emit), so these expressions run through the VM's *generic*
// `BinOp` path, not the `AddBigint`/`SubBigint`/... opcodes. Before the fix,
// that generic path had no bigint arm: `+` fell into string concatenation
// ("VM internal error: type error: expected string, got bigint") and every
// other operator died with `CannotApplyBinOp`.

class PageConfig {
    page_budget_ms: bigint,
}

// The original issue repro: captured class field, inline `+` in the spawn body.
function calculate_deadline(config: PageConfig) -> bigint {
    let pending = spawn { config.page_budget_ms + 1n };
    await pending
}

test "computes_deadline_for_captured_bigint" {
    assert.equal(calculate_deadline(PageConfig { page_budget_ms: 10n }), 11n)
}

// Hoisting the field read into a local before the spawn does not change the
// capture-derived classification; it must work the same way.
function calculate_deadline_with_hoisted_local(config: PageConfig) -> bigint {
    let budget = config.page_budget_ms;
    let pending = spawn { budget + 1n };
    await pending
}

test "computes_deadline_with_hoisted_local" {
    assert.equal(calculate_deadline_with_hoisted_local(PageConfig { page_budget_ms: 10n }), 11n)
}

// Every arithmetic / bitwise / shift operator on a spawn-captured bigint.
// Pre-fix, all of these hit the generic path's `CannotApplyBinOp` (or, for
// `+`, the string-concat arm).
function spawn_bigint_sub(b: bigint) -> bigint {
    let pending = spawn { b - 3n };
    await pending
}

test "spawn_captured_bigint_sub" {
    assert.equal(spawn_bigint_sub(10n), 7n)
}

function spawn_bigint_mul(b: bigint) -> bigint {
    let pending = spawn { b * 4n };
    await pending
}

test "spawn_captured_bigint_mul" {
    assert.equal(spawn_bigint_mul(10n), 40n)
}

function spawn_bigint_div(b: bigint) -> bigint {
    let pending = spawn { b / 3n };
    await pending
}

test "spawn_captured_bigint_div" {
    assert.equal(spawn_bigint_div(10n), 3n)
}

function spawn_bigint_mod(b: bigint) -> bigint {
    let pending = spawn { b % 3n };
    await pending
}

test "spawn_captured_bigint_mod" {
    assert.equal(spawn_bigint_mod(10n), 1n)
}

function spawn_bigint_bitand(b: bigint) -> bigint {
    let pending = spawn { b & 6n };
    await pending
}

test "spawn_captured_bigint_bitand" {
    assert.equal(spawn_bigint_bitand(10n), 2n)
}

function spawn_bigint_bitor(b: bigint) -> bigint {
    let pending = spawn { b | 5n };
    await pending
}

test "spawn_captured_bigint_bitor" {
    assert.equal(spawn_bigint_bitor(10n), 15n)
}

function spawn_bigint_bitxor(b: bigint) -> bigint {
    let pending = spawn { b ^ 6n };
    await pending
}

test "spawn_captured_bigint_bitxor" {
    assert.equal(spawn_bigint_bitxor(10n), 12n)
}

function spawn_bigint_shl(b: bigint) -> bigint {
    let pending = spawn { b << 2n };
    await pending
}

test "spawn_captured_bigint_shl" {
    assert.equal(spawn_bigint_shl(10n), 40n)
}

function spawn_bigint_shr(b: bigint) -> bigint {
    let pending = spawn { b >> 1n };
    await pending
}

test "spawn_captured_bigint_shr" {
    assert.equal(spawn_bigint_shr(10n), 5n)
}

// Comparison on a spawn-captured bigint already worked (exec_cmpop grew its
// erased-bigint arm with the original bigint commit); keep it guarded.
function spawn_bigint_cmp(b: bigint) -> bool {
    let pending = spawn { b > 1n };
    await pending
}

test "spawn_captured_bigint_cmp" {
    assert.equal(spawn_bigint_cmp(10n), true)
}

// A bigint/int mix inside the spawn body: the generic path must widen the
// lone `int` operand exactly like the specialized opcodes do.
function spawn_bigint_int_mix(b: bigint) -> bigint {
    let bump = 5;
    let pending = spawn { b + bump };
    await pending
}

test "spawn_captured_bigint_int_mix" {
    assert.equal(spawn_bigint_int_mix(10n), 15n)
}

// Division by zero inside the spawn body must surface as the same catchable
// DivisionByZero panic the specialized DivBigint opcode throws.
function spawn_bigint_div_by_zero(b: bigint) -> int {
    let zero = 0n;
    let pending = spawn { b / zero };
    {
        await pending;
        baml.sys.panic("expected bigint division by zero to throw")
    } catch (e) {
        baml.panics.DivisionByZero => -1
    }
}

test "spawn_captured_bigint_div_by_zero_is_catchable" {
    assert.equal(spawn_bigint_div_by_zero(10n), -1)
}

// The workaround from the issue — arithmetic in a named function called from
// the spawn block — must keep working (the named function's parameter is not
// capture-derived, so it keeps the specialized AddBigint opcode).
function calculate_deadline_via_named_worker(config: PageConfig) -> bigint {
    config.page_budget_ms + 1n
}

function calculate_deadline_with_named_spawn(config: PageConfig) -> bigint {
    let pending = spawn { calculate_deadline_via_named_worker(config) };
    await pending
}

test "computes_deadline_with_named_worker" {
    assert.equal(calculate_deadline_with_named_spawn(PageConfig { page_budget_ms: 10n }), 11n)
}
