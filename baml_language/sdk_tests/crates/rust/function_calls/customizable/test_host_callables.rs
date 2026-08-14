//! End-to-end tests for the host-callable round trip.
//!
//! The BAML fixture declares functions whose first parameter is a typed
//! `Callable`. The Rust binding takes it as a closure bound by
//! `HostCallback`; the bridge registers it and sends a
//! `Handle{HOST_VALUE_CALLABLE}` wire entry; the engine binds it to an
//! `Object::HostClosure`; when BAML invokes it the `call_host_value` sysop
//! fires the dispatch FFI, which runs the closure on the bridge's dispatch
//! runtime and encodes the result back to the engine.
//!
//! A callback with an inferred `throws` desugars to a generic error param
//! (`(int) -> string` ⇒ `<E>(cb: (int) -> string throws E) -> string throws
//! E`), which the Rust signature realizes as `Error<E>` with `E` inferred
//! from the closure: an infallible closure gives `Error<Infallible>`, a
//! closure erroring with a BAML class gives `Error<ThatClass>`, and one
//! erroring with an arbitrary `std::error::Error` gives `Error<HostCallable>`
//! — the opaque host error, rehydratable via `HostCallable::downcast_ref` on
//! the same host.

use baml_sdk::host_callable_tests::{
    Person, ValidationError, call_callback_with_optional_args_all_set,
    call_callback_with_optional_args_all_unset, call_callback_with_optional_args_partially_set,
    call_int_callback, call_repeatedly, call_with_callback, call_with_class_callback,
    call_with_throwing, call_with_two_args, call_with_typed_throws,
    call_with_typed_throws_propagating, make_adder, make_counter, make_pair_builder,
};

/// Stand-in for python's builtin `ValueError`: an arbitrary host error value
/// that must ride the bridge's host-value registry and come back out intact.
#[derive(Debug, Clone, PartialEq)]
struct ValueError(String);

impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ValueError {}

/// Stand-in for python's builtin `KeyError` — a second error type, to pin
/// that the rehydration path is class-agnostic.
#[derive(Debug, Clone, PartialEq)]
struct KeyError(String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for KeyError {}

#[test]
fn test_host_callables_simple_sync_callable_returns_string() {
    let cb = |x: i64| format!("got {x}");

    let result = call_with_callback(cb, 5).unwrap();
    assert_eq!(result, "got 5");
}

#[test]
fn test_host_callables_two_arg_callable_unpacks_positional_args() {
    let cb = |x: i64, prefix: String| format!("{prefix}:{x}");

    let result = call_with_two_args(cb, 7, "answer".to_string()).unwrap();
    assert_eq!(result, "answer:7");
}

#[test]
fn test_host_callables_int_return_callable_round_trip() {
    let cb = |x: i64| x * 2;

    let result = call_int_callback(cb, 21).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_baml_closure_is_a_native_callable_with_host_language_arguments() {
    let add_ten = make_adder(10).unwrap();
    assert_eq!(add_ten.call((5,)).unwrap(), 15);
    assert_eq!(add_ten.call((7,)).unwrap(), 17);
}

#[test]
fn test_baml_closure_decodes_multiple_args_and_structured_return_values() {
    let build = make_pair_builder(30).unwrap();
    assert_eq!(
        build.call((12, "Ada".to_string())).unwrap(),
        Person {
            name: "Ada".to_string(),
            age: 42,
        }
    );
    assert_eq!(
        build.call((5, "Grace".to_string())).unwrap(),
        Person {
            name: "Grace".to_string(),
            age: 35,
        }
    );
}

#[test]
fn test_baml_closure_is_reusable_and_retains_mutable_captures() {
    let next_value = make_counter(40).unwrap();
    assert_eq!(next_value.call(()).unwrap(), 41);
    assert_eq!(next_value.call(()).unwrap(), 42);
}

#[test]
fn test_host_callables_throwing_callable_round_trips_original_python_exception() {
    // A native Python exception raised inside a host callable surfaces back
    // to the caller as the *same* exception object (`raised is caught`
    // identity), not flattened into a `BamlError(HostCallable(...))` wrapper.
    // The bridge registers the exception in its host-value registry on the
    // inbound throw, BAML transports the `baml.errors.HostCallable` Instance
    // with the handle in `_handle`, and the outbound decoder looks the handle
    // up to re-raise the original.
    // DIVERGENCE(rust): python asserts `raised is caught` object identity;
    // Rust errors are values, so the round trip is pinned by value equality
    // of the downcast error instead.
    let raised = ValueError("nope".to_string());

    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, ValueError> { Err(raised.clone()) }
    };

    // The closure errors with an arbitrary `std::error::Error`, so the
    // callback's `Throws` — and thus `call_with_callback`'s `E` — is
    // `HostCallable`: the opaque host error, transported through BAML and
    // rehydrated to the original on the same host.
    let err = call_with_callback(cb, 1).expect_err("the host throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the opaque host throw as a HostCallable, got {err}");
    };
    let original = value
        .downcast_ref::<ValueError>()
        .expect("the original host error must round-trip");
    assert_eq!(original, &raised);
    assert_eq!(original.to_string(), "nope");
}

#[test]
fn test_host_callables_throwing_callable_keyerror_round_trips_with_identity() {
    // The native-exception rehydration path is class-agnostic: any
    // `BaseException` subclass round-trips by reference, not just
    // `ValueError`. Different exception classes should not collide in the
    // bridge's host-value registry.
    let raised = KeyError("missing".to_string());

    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, KeyError> { Err(raised.clone()) }
    };

    let err = call_with_callback(cb, 1).expect_err("the host throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the opaque host throw, got {err}");
    };
    assert_eq!(value.downcast_ref::<KeyError>(), Some(&raised));
}

#[test]
fn test_host_callables_throwing_callable_custom_python_exception_round_trips_with_identity() {
    // A user-defined Python exception subclass also round-trips by identity —
    // the bridge doesn't care about the concrete type.
    #[derive(Debug, Clone, PartialEq)]
    struct MyDomainError {
        message: String,
        code: i64,
    }

    impl std::fmt::Display for MyDomainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for MyDomainError {}

    let raised = MyDomainError {
        message: "custom domain failure".to_string(),
        code: 42,
    };

    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, MyDomainError> { Err(raised.clone()) }
    };

    let err = call_with_callback(cb, 1).expect_err("the host throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the opaque host throw, got {err}");
    };
    let original = value
        .downcast_ref::<MyDomainError>()
        .expect("the original host error must round-trip");
    assert_eq!(original, &raised);
    assert_eq!(original.code, 42);
}

#[test]
fn test_host_callables_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml() {
    // When the host raises `BamlError(value=<codegenned BAML class>)`, the
    // bridge unwraps the inner pydantic model and emits it as that real BAML
    // class on the wire — not as an opaque `HostCallable` with a stringified
    // `class_name`. The BAML side's typed `catch (e: ValidationError) { ... }`
    // matches structurally and reads `e.code` / `e.message` / `e.fields` as
    // real fields, demonstrating the typed-error contract holds end-to-end
    // across the host boundary.
    // DIVERGENCE(rust): there is no `BamlError` wrapper to unwrap — the
    // callback's declared throws class IS the closure's error type, so
    // returning `Err(ValidationError { .. })` is the typed-throw surface.
    let cb = |_x: i64| -> Result<String, ValidationError> {
        Err(ValidationError {
            code: 4,
            message: "bad shape".to_string(),
            fields: vec![
                "name".to_string(),
                "age".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ],
        })
    };

    let result = call_with_typed_throws(cb, 1).unwrap();
    assert_eq!(result, "caught: bad shape");
}

#[test]
fn test_host_callables_throwing_callable_bamlerror_propagates_back_with_typed_fields() {
    // The same `BamlError(ValidationError(...))` raised by the host, when
    // *not* caught in BAML, propagates back out to Python with all typed
    // fields preserved on the pydantic instance — the engine transports it as
    // a real `ValidationError` class instance both directions, never
    // collapsing to a stringified HostCallable metadata blob.
    let raised = ValidationError {
        code: 7,
        message: "propagated through".to_string(),
        fields: vec!["x".to_string(), "y".to_string()],
    };

    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, ValidationError> { Err(raised.clone()) }
    };

    let err = call_with_typed_throws_propagating(cb, 1)
        .expect_err("the uncaught typed throw must propagate to the caller");
    // The callback's declared throws is a real BAML class, so it propagates as
    // that class: `E = ValidationError`, surfaced in `Error::Thrown`.
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the typed throw, got {err}");
    };
    assert_eq!(value.code, 7);
    assert_eq!(value.message, "propagated through");
    assert_eq!(value.fields, ["x", "y"]);
}

#[test]
fn test_host_callables_throwing_async_callable_round_trips_original_python_exception() {
    // Async callbacks are driven to completion on the bridge's dispatch
    // runtime even on the sync call path; an opaque error raised inside the
    // future round-trips by value just like the sync case.
    let raised = ValueError("async nope".to_string());

    // A callback that returns a future (`|x| async { … }`) — the form
    // `HostCallback` accepts; the future is driven on the dispatch runtime.
    let cb = {
        let raised = raised.clone();
        move |_x: i64| {
            let raised = raised.clone();
            async move { Err::<String, ValueError>(raised) }
        }
    };

    let err = call_with_callback(cb, 1).expect_err("the host throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the opaque host throw, got {err}");
    };
    assert_eq!(value.downcast_ref::<ValueError>(), Some(&raised));
}

#[test]
fn test_host_callables_multiple_throws_in_flight_do_not_collide_in_registry() {
    // Each host throw mints a fresh host-value key; calls in quick succession
    // must not see the wrong original exception. Exercises the `next_key()`
    // minting + per-call cleanup invariant of the host-value registry.
    let raised_first = ValueError("first".to_string());
    let raised_second = ValueError("second".to_string());

    let cb_first = {
        let raised = raised_first.clone();
        move |_x: i64| -> Result<String, ValueError> { Err(raised.clone()) }
    };
    let cb_second = {
        let raised = raised_second.clone();
        move |_x: i64| -> Result<String, ValueError> { Err(raised.clone()) }
    };

    let err_first =
        call_with_callback(cb_first, 1).expect_err("the host throw must surface to the caller");
    let err_second =
        call_with_callback(cb_second, 2).expect_err("the host throw must surface to the caller");
    let (
        baml_bridge::Error::Thrown { value: first, .. },
        baml_bridge::Error::Thrown { value: second, .. },
    ) = (err_first, err_second)
    else {
        panic!("expected both opaque host throws");
    };
    assert_eq!(first.downcast_ref::<ValueError>(), Some(&raised_first));
    assert_eq!(second.downcast_ref::<ValueError>(), Some(&raised_second));
    // DIVERGENCE(rust): python's `ei1.value is not ei2.value` identity check
    // becomes value inequality of the two round-tripped errors.
    assert_ne!(
        first.downcast_ref::<ValueError>(),
        second.downcast_ref::<ValueError>()
    );
}

// python marks this xfail (strict=False); ported as #[ignore] with the same
// reason.
#[test]
#[ignore = "host-callable release fires only when the engine GCs the Object::HostClosure on its heap; one BAML call rarely triggers the GC heuristic, so for now the callable leaks until the engine collects."]
fn test_host_callables_release_fires_on_drop_of_callable() {
    // After BAML finishes invoking the callable and the engine GCs the
    // `Object::HostClosure` it allocated, the registered release callback
    // removes the Python callable from the bridge's host-value table.
    // Dropping the user's last reference then leaves the object unreachable
    // for the cycle collector.
    // DIVERGENCE(rust): no weakref/gc.collect() — an `Arc` captured by the
    // closure plays the part: the closure moves into the call, so once the
    // bridge's host-value table drops it, the `Weak` can no longer upgrade.
    let state = std::sync::Arc::new(());
    let wr = std::sync::Arc::downgrade(&state);
    let cb = move |x: i64| {
        let _ = &state;
        x.to_string()
    };
    let result = call_with_callback(cb, 3).unwrap();
    assert_eq!(result, "3");
    assert!(
        wr.upgrade().is_none(),
        "host callable should be released after BAML drops it"
    );
}

#[test]
fn test_host_callables_lambda_round_trip() {
    // Lambdas are callable and not pydantic models, so they hit the
    // callable-encoding branch in `_set_inbound_value`.
    let result = call_with_callback(|x: i64| format!("lambda-{x}"), 99).unwrap();
    assert_eq!(result, "lambda-99");
}

#[test]
fn test_host_callables_async_callable_runs_to_completion() {
    // An async callback runs to completion on the bridge's dispatch runtime,
    // even when invoked through the sync call path.
    let cb = |x: i64| async move {
        // Minimal awaitable body — exercises the coroutine path.
        std::future::ready(()).await;
        format!("async-{x}")
    };

    let result = call_with_callback(cb, 4).unwrap();
    assert_eq!(result, "async-4");
}

#[test]
fn test_host_callables_multiple_callable_keys_are_distinct() {
    // Two separately-registered callables must produce two distinct keys;
    // invoking one must not call the other.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    let counter_a = Arc::new(AtomicI64::new(0));
    let counter_b = Arc::new(AtomicI64::new(0));

    let cb_a = {
        let counter = Arc::clone(&counter_a);
        move |x: i64| {
            counter.fetch_add(1, Ordering::SeqCst);
            format!("a:{x}")
        }
    };
    let cb_b = {
        let counter = Arc::clone(&counter_b);
        move |x: i64| {
            counter.fetch_add(1, Ordering::SeqCst);
            format!("b:{x}")
        }
    };

    assert_eq!(call_with_callback(cb_a, 1).unwrap(), "a:1");
    assert_eq!(call_with_callback(cb_b, 2).unwrap(), "b:2");
    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

#[test]
fn test_host_callables_class_callback_round_trips_pydantic_model() {
    // A user-defined `Person` class round-trips through the callable
    // boundary: BAML encodes the `Person` for the engine→host call; the
    // Python dispatcher decodes it into the codegen-emitted pydantic model;
    // the user callback receives a `Person` instance.
    let cb = |p: Person| format!("{} is {}", p.name, p.age);

    let person = Person {
        name: "Ada".to_string(),
        age: 37,
    };
    let result = call_with_class_callback(cb, person).unwrap();
    assert_eq!(result, "Ada is 37");
}

#[test]
fn test_host_callables_call_repeatedly_invokes_callback_n_times() {
    // Exercises N round-trips through `SysOp::BamlHostCallHostValue`: BAML's
    // `for` loop invokes the callable for each iteration; the result list
    // collects every callback return value.
    use std::sync::{Arc, Mutex};

    let invocations = Arc::new(Mutex::new(Vec::<i64>::new()));
    let cb = {
        let invocations = Arc::clone(&invocations);
        move |x: i64| {
            invocations.lock().unwrap().push(x);
            format!("item-{x}")
        }
    };

    let results = call_repeatedly(cb, 5).unwrap();
    assert_eq!(
        results,
        (0..5).map(|i| format!("item-{i}")).collect::<Vec<_>>()
    );
    assert_eq!(*invocations.lock().unwrap(), (0..5).collect::<Vec<i64>>());
}

#[test]
fn test_host_callables_call_repeatedly_with_zero_n_returns_empty_list() {
    // N == 0 should produce no callback invocations and an empty result list
    // — covers the loop's zero-iteration edge case.
    use std::sync::{Arc, Mutex};

    let invocations = Arc::new(Mutex::new(Vec::<i64>::new()));
    let cb = {
        let invocations = Arc::clone(&invocations);
        move |x: i64| {
            invocations.lock().unwrap().push(x);
            String::new()
        }
    };

    let results = call_repeatedly(cb, 0).unwrap();
    assert_eq!(results, Vec::<String>::new());
    assert_eq!(*invocations.lock().unwrap(), Vec::<i64>::new());
}

#[test]
fn test_host_callables_call_with_throwing_in_baml_catches_host_callable_error() {
    // The BAML `catch (e)` clause around a host-callable invocation now
    // intercepts a host-thrown `baml.errors.HostCallable` and returns the
    // recovery branch.
    //
    // The fixture (`call_with_throwing`) declares the callback's throws
    // contract as `baml.errors.HostCallable` and wraps the call site in
    // `catch (e) { _ => "caught:" + e.class_name }`. The host raises
    // `RuntimeError`; the bridge wraps it as a `HostCallable` Instance with
    // `class_name="RuntimeError"`; the engine's throws-contract check accepts
    // it (since the declared `E` is `HostCallable`); the throw is injected
    // through the VM's unwinder and caught by the BAML `catch`; the recovery
    // expression returns the string. Earlier in the branch this surfaced as
    // an unhandled Python exception — the engine now threads sysop throws
    // through the same unwinder a `throw` opcode uses, so an in-BAML `catch`
    // can intercept them like any other throw.

    /// Named to mirror python's builtin `RuntimeError`. The bridge surfaces
    /// the host error type's `type_name` as the `HostCallable`'s `class_name`,
    /// which the BAML `catch` reads.
    #[derive(Debug)]
    struct RuntimeError(String);

    impl std::fmt::Display for RuntimeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for RuntimeError {}

    let cb = |_x: i64| -> Result<String, RuntimeError> {
        Err(RuntimeError("boom from host".to_string()))
    };

    let result = call_with_throwing(cb, 1).unwrap();
    // DIVERGENCE(rust): python's `class_name` is `type(e).__name__` (bare
    // `RuntimeError`); Rust's is the full, compiler-dependent `type_name` (a
    // module path), so the catch returns `"caught:" + <full path>` — assert
    // the shape, not the exact string.
    assert!(
        result.starts_with("caught:") && result.contains("RuntimeError"),
        "{result}"
    );
}

// ---------------------------------------------------------------------------
// Optional args × host callables (the combination).
//
// A host callable whose *own* type carries optional parameters
// (`(x: int, y?: int, z?: int) -> int`). Defaults aren't allowed inside a
// callable type — only the `?` optional marker — so the host's own
// language-level default is the only source of a value when BAML omits the arg.
// `y` and `z` cross the boundary by name, so each can be supplied or omitted
// independently; an omitted optional is dropped before dispatch and the host's
// default fills it. The callback returns `x*100 + y*10 + z` so each test can
// read off exactly which optionals were delivered.
//
// An optional the BAML side omits is delivered to the Rust callback as `None`
// (an optional callable parameter is `Option<T>` in the closure's argument
// tuple); the host's own default (python's `y: int = 8`) becomes an
// `unwrap_or`.
// ---------------------------------------------------------------------------

fn optional_args_cb(x: i64, y: Option<i64>, z: Option<i64>) -> i64 {
    x * 100 + y.unwrap_or(8) * 10 + z.unwrap_or(9)
}

#[test]
fn test_host_callables_optional_args_all_unset_apply_host_defaults() {
    // `callback(x)` supplies neither optional. Both are dropped before
    // dispatch, so the Python callback runs with only `x` and its own
    // defaults fill `y`/`z` (8 and 9), yielding `5*100 + 8*10 + 9 = 589`.
    assert_eq!(
        call_callback_with_optional_args_all_unset(optional_args_cb, 5).unwrap(),
        [589]
    );
}

#[test]
fn test_host_callables_optional_args_partially_set_deliver_by_name() {
    // Two calls each supplying exactly one optional by name:
    // `callback(x, y = 2)` (→ `500 + 20 + 9 = 529`) then `callback(x, z = 3)`
    // (→ `500 + 80 + 3 = 583`). Optionals cross by name, so each supplied
    // value is delivered as a keyword and the omitted one falls back to the
    // host default (`y`→8, `z`→9) — including the case where the *leading*
    // optional `y` is skipped while `z` is supplied.
    assert_eq!(
        call_callback_with_optional_args_partially_set(optional_args_cb, 5).unwrap(),
        [529, 583]
    );
}

#[test]
fn test_host_callables_optional_args_all_set_deliver_both() {
    // `callback(x, y = 2, z = 3)` supplies both optionals; both arrive by
    // name and override the host defaults, yielding `500 + 20 + 3 = 523`.
    assert_eq!(
        call_callback_with_optional_args_all_set(optional_args_cb, 5).unwrap(),
        [523]
    );
}
