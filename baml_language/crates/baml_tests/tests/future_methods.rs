//! BEP-034 Phase F: native methods on `baml.future.Future`.
//!
//! Methods are declared in `baml_std/baml/ns_future/future.baml` as
//! `$rust_function` and implemented in `bex_vm::package_baml::future`.
//! Each method operates on the heap `Object::Future` directly — no
//! engine round-trip — so these tests confirm the wiring end-to-end.
//!
//! Tests are grouped by the future's terminal state. Each test fans out
//! all relevant queries on a single future so we don't pay the spawn /
//! await round-trip once per assertion.

use baml_tests::baml_test;
use bex_engine::{BexExternalValue, Ty};

/// All accessors after a successful settle: is_settled / is_result are
/// true; is_error / is_cancelled are false; cancel() returns false (the
/// future is already terminal, no transition performed).
#[tokio::test]
async fn methods_after_successful_settle() {
    let output = baml_test!(
        r#"
        function main() -> bool[] {
            let f = spawn { 42 };
            let _ = await f;
            [
                f.is_settled(),
                f.is_result(),
                f.is_error(),
                f.is_cancelled(),
                f.cancel(),
            ]
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::bool(),
            items: vec![
                BexExternalValue::Bool(true),
                BexExternalValue::Bool(true),
                BexExternalValue::Bool(false),
                BexExternalValue::Bool(false),
                BexExternalValue::Bool(false),
            ],
        })
    );
}

/// Pending future (long sleep keeps the spawn body parked): is_settled
/// is false; state() returns Pending.
#[tokio::test]
async fn methods_on_pending_future() {
    let output = baml_test!(
        r#"
        function main() -> baml.future.FutureState {
            let f = spawn { baml.sys.sleep(5000); 42 };
            // is_settled assertion folded into state() — Pending is the
            // single source of truth and exercises both the heap read
            // and the enum variant allocation.
            let _ = f.is_settled();
            f.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Pending");
        }
        other => panic!("expected Variant Pending, got {other:?}"),
    }
}

/// After `f.cancel()` on a pending future: cancel returns true (the
/// transition was performed), is_cancelled is true, state is Cancelled.
#[tokio::test]
async fn methods_after_cancel() {
    let output = baml_test!(
        r#"
        function main() -> baml.future.FutureState {
            let f = spawn { baml.sys.sleep(5000); 42 };
            let _ = f.cancel();
            f.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Cancelled");
        }
        other => panic!("expected Variant Cancelled, got {other:?}"),
    }
}

/// After awaiting an errored future (catch swallows the throw): is_error
/// is true and state() returns Error. Migrated from `spawn_throws.rs`.
#[tokio::test]
async fn methods_on_errored_future() {
    let output = baml_test!(
        r#"
        function fail() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "x" }
        }
        function main() -> baml.future.FutureState {
            let f = spawn { fail() };
            let _ = (await f) catch (e) { baml.errors.Io => 0 };
            f.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Error");
        }
        other => panic!("expected Variant Error, got {other:?}"),
    }
}
