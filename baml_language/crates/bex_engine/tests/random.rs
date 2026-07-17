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

// IGNORED: native (`$rust_function`) builtins don't yet substitute parameter
// defaults. The `seed` default-arg prologue is only generated for bytecode
// function bodies (`lower_default_parameter_prologue`), so `new()` reaches the
// native constructor with `seed = OmittedArg`. Re-enable once native builtins
// support parameter defaults (needed for `int.random(rng: Rng = ...)` too).
#[ignore = "native builtins don't substitute parameter defaults yet"]
#[tokio::test]
async fn new_without_seed_uses_system_entropy() {
    // No explicit seed → the default arg `Rng.random(SystemRandom.get(), 32)`
    // runs, exercising the IO seeding path through interface dispatch.
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

// IGNORED: a concrete generator in the user package can't be upcast to the
// stdlib interface `baml.random.Rng` — `package_implements_registry` is
// per-package, so the user package doesn't see the `baml` package's
// `Xoshiro256PlusPlus implements Rng` entry. Re-enable once class→interface
// upcast resolves implementors across packages.
#[ignore = "cross-package class->interface upcast not yet supported"]
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
