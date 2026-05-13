//! BEP-034 Phase F: native methods on `baml.future.Future`.
//!
//! Methods are declared in `baml_std/baml/ns_future/future.baml` as
//! `$rust_function` and implemented in `bex_vm::package_baml::future`.
//! Each method operates on the heap `Object::Future` directly — no
//! engine round-trip — so these tests confirm the wiring end-to-end.
//!
//! The "Pending" variants of these checks (e.g. `is_settled() == false`
//! before await) are harder to express here because the only stdlib
//! sleep — `baml.sys.sleep(int)` — throws `baml.errors.Io`, and the
//! compiler's throws-inference for spawn bodies currently rejects an
//! Io-throwing body inside `spawn { … }` unless the outer function is
//! also declared to throw Io. See `spawn_parallel.rs` for the same
//! workaround. Once that's relaxed (separate issue), add the
//! Pending-state tests.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn is_settled_true_after_await() {
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> bool {
            let f = spawn { payload() };
            let _ = await f;
            f.is_settled()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_result_true_after_successful_settle() {
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> bool {
            let f = spawn { payload() };
            let _ = await f;
            f.is_result()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn is_cancelled_false_after_normal_settle() {
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> bool {
            let f = spawn { payload() };
            let _ = await f;
            f.is_cancelled()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn is_error_false_after_successful_settle() {
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> bool {
            let f = spawn { payload() };
            let _ = await f;
            f.is_error()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn cancel_settled_future_returns_false() {
    // After await, the future is Ready, so cancel() observes that and
    // returns false (no transition performed).
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> bool {
            let f = spawn { payload() };
            let _ = await f;
            f.cancel()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn state_ready_after_await() {
    let output = baml_test!(
        r#"
        function payload() -> int { 42 }
        function main() -> baml.future.FutureState {
            let f = spawn { payload() };
            let _ = await f;
            f.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Ready");
        }
        other => panic!("expected Variant Ready, got {other:?}"),
    }
}
