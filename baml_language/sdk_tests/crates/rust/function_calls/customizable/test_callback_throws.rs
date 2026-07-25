//! Callback-throws inference — typed error surfaces unique to Rust.
//!
//! A callback with an omitted `throws` gets an inferred effect param `E`
//! (`() -> int` ⇒ `<E>(cb: () -> int throws E) -> …`). What the *outer*
//! function throws then depends on the body, and the Rust SDK reflects each
//! shape as a distinct `Error<…>`:
//!
//! - caught & replaced with a value → `Error<Infallible>` (the effect param
//!   lives only in the callback bound);
//! - caught & a different value thrown → the replacement's union, no effect
//!   param;
//! - re-thrown → a generic union that carries the effect param.
//!
//! Python/TS erase `throws`, so these cases have no cross-language counterpart
//! — hence a Rust-only file. (Callback return types are concrete `int`, not
//! `void`: the engine validates the host's returned value against them.)

use baml_bridge::Error;
use baml_sdk::host_callable_tests::{
    IntOrCbError, IntOrString, callback_error_caught, callback_error_replaced,
    callback_error_rethrown,
};

/// An arbitrary host error with no BAML representation.
#[derive(Debug, Clone, PartialEq)]
struct Boom(String);

impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Boom {}

// SDK_PARITY_LINT(skip): validates Rust-specific inferred callback error unions
#[test]
fn test_callback_throws_caught_and_replaced_makes_the_function_infallible() {
    // `callback_error_caught` catches the callback's error and yields `0`, so
    // it `throws never` — `E = Infallible` no matter what the closure throws.
    // The effect param exists only in the callback bound.
    let cb = || Err::<i64, Boom>(Boom("boom".to_string()));
    let _: Result<i64, Error<std::convert::Infallible>> = callback_error_caught(cb);
    assert_eq!(callback_error_caught(cb).unwrap(), 0);
}

// SDK_PARITY_LINT(skip): validates Rust-specific inferred callback error unions
#[test]
fn test_callback_throws_caught_then_rethrown_value_is_the_replacement_union() {
    // The body catches the callback's error and throws `"oops"`, else throws
    // `1`, so `E = string | int` — a union with no effect param.

    // Callback succeeds → the trailing `throw 1` fires.
    let err = callback_error_replaced(|| 0).expect_err("throws 1");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the int throw, got {err}");
    };
    assert!(matches!(*value, IntOrString::Int(1)), "{value:?}");

    // Callback throws → the catch fires `throw "oops"`.
    let cb = || Err::<i64, Boom>(Boom("ignored".to_string()));
    let err = callback_error_replaced(cb).expect_err("catch throws \"oops\"");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the string throw, got {err}");
    };
    match &*value {
        IntOrString::String(s) => assert_eq!(s, "oops"),
        other => panic!("expected the string arm, got {other:?}"),
    }
}

// SDK_PARITY_LINT(skip): validates Rust-specific inferred callback error unions
#[test]
fn test_callback_throws_rethrown_carries_the_effect_param_into_the_error_union() {
    // The body re-throws the callback's error (`throw e`), so the effect param
    // escapes: `E = int | CbError`, a generic union. `CbError` is inferred
    // from the closure.

    // Callback succeeds → `throw 1`; here `CbError` is `Infallible`.
    let err = callback_error_rethrown(|| 0).expect_err("throws 1");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the int throw, got {err}");
    };
    assert!(matches!(*value, IntOrCbError::Int(1)), "{value:?}");

    // Callback throws an opaque host error → re-thrown as `CbError`, which is
    // inferred to `HostCallable`; the original round-trips.
    let raised = Boom("re-thrown".to_string());
    let cb = {
        let raised = raised.clone();
        move || Err::<i64, Boom>(raised.clone())
    };
    let err = callback_error_rethrown(cb).expect_err("catch re-throws e");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the re-thrown host error, got {err}");
    };
    let IntOrCbError::CbError(host) = &*value else {
        panic!("expected the CbError arm, got {value:?}");
    };
    assert_eq!(host.downcast_ref::<Boom>(), Some(&raised));
}
