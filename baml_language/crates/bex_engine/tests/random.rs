//! End-to-end tests for the `baml.random` generators, run through the engine
//! with the real native `SysOps` (so `SystemRandom` draws real OS entropy and
//! the `$rust_io_function` dispatch path is exercised).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

/// A fixed 32-byte seed passed as the entry function's `uint8array` argument.
fn seed_input() -> Vec<BexExternalValue> {
    vec![BexExternalValue::Uint8Array((0u8..32).collect())]
}

#[tokio::test]
async fn xoshiro_is_deterministic_and_advances() {
    // Two generators seeded identically agree draw-for-draw, and successive
    // draws from one generator differ (the state advances under the mutex).
    let source = r#"
function main(seed: uint8array) -> bool {
    let a = baml.random.Xoshiro256PlusPlus.new(seed = seed);
    let b = baml.random.Xoshiro256PlusPlus.new(seed = seed);
    let a1 = a.random_int();
    let b1 = b.random_int();
    let a2 = a.random_int();
    a1 == b1 && a1 != a2
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        inputs: seed_input(),
        expected: Ok(BexExternalValue::Bool(true)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn chacha_is_deterministic() {
    let source = r#"
function main(seed: uint8array) -> bool {
    let a = baml.random.ChaCha20.new(seed = seed);
    let b = baml.random.ChaCha20.new(seed = seed);
    a.random_int() == b.random_int()
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        inputs: seed_input(),
        expected: Ok(BexExternalValue::Bool(true)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn random_returns_requested_byte_count() {
    let source = r#"
function main(seed: uint8array) -> int {
    baml.random.Xoshiro256PlusPlus.new(seed = seed).random(16).length()
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        inputs: seed_input(),
        expected: Ok(BexExternalValue::Int(16)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn system_random_draws_through_native_sysops() {
    // Exercises the `$rust_io_function` path end-to-end: `SystemRandom.random`
    // is dispatched to `sys_native` and must return the requested bytes.
    let source = r#"
function main() -> int {
    baml.random.SystemRandom.get().random(24).length()
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(BexExternalValue::Int(24)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn new_without_seed_uses_system_entropy() {
    // No explicit seed → `ChaCha20.new`'s BAML wrapper fills the default arg
    // `Rng.random(SystemRandom.get(), 32)` (the native `$rust_function` `_new`
    // it forwards to takes a required seed), exercising the IO seeding path
    // through interface dispatch.
    let source = r#"
function main() -> int {
    baml.random.ChaCha20.new().random(8).length()
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(BexExternalValue::Int(8)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn default_random_int_spans_full_signed_range_without_overflow() {
    // The `Rng.random_int` default body — inherited by any user implementor that
    // provides only `random`. All-`0xFF` bytes assemble the maximum magnitude
    // with the sign bit set: exactly the input that overflowed the old
    // `(bytes[0] & 127) << 56` form. It must land on `int.min_value()`, and
    // all-zero bytes on `0`, confirming the full signed range with no overflow.
    let source = r#"
class AllOnes {
    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array throws never {
            let data = b"\xff\xff\xff\xff\xff\xff\xff\xff";
            data
        }
    }
}
class AllZeros {
    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array throws never {
            let data = b"\x00\x00\x00\x00\x00\x00\x00\x00";
            data
        }
    }
}
function main() -> bool {
    (AllOnes {}).random_int() == baml.Int.min_value()
        && (AllZeros {}).random_int() == 0
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(BexExternalValue::Bool(true)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn rng_dispatches_through_interface() {
    // A concrete generator assigned to a `Rng`-typed parameter dispatches to
    // the right implementor — the same value as calling it directly.
    let source = r#"
function draw(rng: baml.random.Rng) -> int {
    rng.random_int()
}
function main(seed: uint8array) -> bool {
    let direct = baml.random.Xoshiro256PlusPlus.new(seed = seed).random_int();
    let via_iface = draw(baml.random.Xoshiro256PlusPlus.new(seed = seed).as<baml.random.Rng>);
    direct == via_iface
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        inputs: seed_input(),
        expected: Ok(BexExternalValue::Bool(true)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn sysop_dispatches_through_interface_typed_param() {
    // A `$rust_io_function` impl method reached through an `Rng`-typed
    // parameter: the `VirtualCall` resolves `SystemRandom`'s impl at runtime
    // and the call funnel yields the sys-op to the engine — the analogue of
    // `rng_dispatches_through_interface` for the IO (sys-op) dispatch kind.
    let source = r#"
function draw(rng: baml.random.Rng) -> int {
    rng.random(24).length()
}
function main() -> int {
    draw(baml.random.SystemRandom.get().as<baml.random.Rng>)
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(BexExternalValue::Int(24)),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn sysop_bound_method_value_invokes() {
    // Tearing a `$rust_io_function` impl method off its receiver as a value
    // and calling it indirectly routes the same call funnel into the sys-op
    // yield — the value analogue of the virtual call above.
    let source = r#"
function main() -> int {
    let f = baml.random.SystemRandom.get().random;
    f(16).length()
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(BexExternalValue::Int(16)),
        ..Default::default()
    })
    .await
    .unwrap();
}
