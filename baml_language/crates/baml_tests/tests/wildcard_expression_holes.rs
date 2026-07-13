//! A `_` inference hole in an *expression-context* type position (call
//! turbofish `race<T, _>`, object construction `Box<_> { … }`, a bare
//! generic-apply value `id<_>`, an upcast target `.as<Show<_>>`).
//!
//! `_` type inference is not supported: every `_` is a hard error
//! (`CannotInferType`, E0147) at type lowering. The active tests here pin the
//! clean rejection — a diagnostic, never a raw inference hole reaching
//! structural normalization or runtime lowering and panicking. The ignored
//! tests document the inference behavior to enable if `_` expression-position
//! inference is ever implemented.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const TIMEOUT_HELPERS: &str = r#"
function with_timeout<T>(millis: int, f: () -> T) -> T {
    let work = spawn { f() };
    let timer = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(millis));
        work.cancel();
        throw TimeoutError { message: `Operation timed out after ${millis} milliseconds` };
    };
    let result = baml.future.race<T, _>([work, timer]);
    await result
}

class TimeoutError {
    message: string,

    implements baml.ToString {
        function to_string(self) -> string throws never {
            return self.message;
        }
    }
}
"#;

/// The exact reported repro: `race<T, _>` compiles and the fast work future
/// wins, returning its value.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_turbofish_race_work_wins() {
    let src = format!(
        "{TIMEOUT_HELPERS}\nfunction main() -> int {{\n  with_timeout(1000, () -> {{ 42 }})\n}}\n"
    );
    let out = baml_test!(&src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(42))),
        "expected 42, got {:?}",
        out.result
    );
}

/// The timer branch actually fires when the work outlasts the timeout: the
/// inferred `_` error type (`TimeoutError | …`) is thrown and surfaces.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_turbofish_race_timer_fires() {
    let src = format!(
        "{TIMEOUT_HELPERS}\nfunction main() -> int {{\n  with_timeout(1, () -> {{ baml.sys.sleep(baml.time.Duration.from_milliseconds(1000)); 7 }})\n}}\n"
    );
    let out = baml_test!(&src);
    assert!(
        out.result.is_err(),
        "expected the timeout error to surface, got {:?}",
        out.result
    );
    let msg = format!("{:?}", out.result);
    assert!(
        msg.contains("timed out"),
        "expected the TimeoutError message, got {msg}"
    );
}

/// A `_` turbofish hole is a clean `type inference failed` diagnostic
/// (E0147), never a runtime-lowering panic.
#[tokio::test]
#[should_panic(expected = "type inference failed")]
async fn wildcard_turbofish_uninferable_is_rejected() {
    // `pick<int, _>(5)` — `U` appears in neither an argument nor the return
    // type, so the `_` hole has nothing to be solved from.
    let src = r#"
function pick<T, U>(x: T) -> T { x }
function main() -> int {
    pick<int, _>(5)
}
"#;
    let _ = baml_test!(src);
}

// ===========================================================================
// Object construction: `Foo<_> { … }` solves the hole from field values.
// ===========================================================================

/// `Box<_> { v: 5 }` infers `T = int` from the field value, exactly like the
/// bare `Box { v: 5 }` form.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_object_ctor_infers_from_field() {
    let src = r#"
class Box<T> { v T }
function main() -> int { let b = Box<_> { v: 5 }; b.v }
"#;
    let out = baml_test!(src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(5))),
        "expected 5, got {:?}",
        out.result
    );
}

/// A partial `Pair<int, _>` pins the first arg and infers the second from a
/// field.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_object_ctor_mixed_partial() {
    let src = r#"
class Pair<A, B> { a A  b B }
function main() -> string { let p = Pair<int, _> { a: 1, b: "hi" }; p.b }
"#;
    let out = baml_test!(src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::String(ref s)) if s == "hi"),
        "expected \"hi\", got {:?}",
        out.result
    );
}

/// A nested hole (`Box<Box<_>>`) is filled structurally from the field value.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_object_ctor_nested_hole() {
    let src = r#"
class Box<T> { v T }
function main() -> int { let b = Box<Box<_>> { v: Box<int> { v: 5 } }; b.v.v }
"#;
    let out = baml_test!(src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(5))),
        "expected 5, got {:?}",
        out.result
    );
}

/// A phantom param (used by no field) can never be determined by construction;
/// it is recovered silently rather than crashing.
#[tokio::test]
#[ignore = "`_` type inference is not supported — every `_` is a hard error (E0147). Un-ignore if `_` expression-position inference is implemented."]
async fn wildcard_object_ctor_phantom_param_recovers() {
    let src = r#"
class Phantom<T> { label string }
function main() -> string { let p = Phantom<_> { label: "hi" }; p.label }
"#;
    let out = baml_test!(src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::String(ref s)) if s == "hi"),
        "expected \"hi\", got {:?}",
        out.result
    );
}

// ===========================================================================
// Generic-apply value: `id<_>` has nothing to infer from → clean diagnostic.
// ===========================================================================

/// A `_` in a bare generic instantiation value (not immediately called) is a
/// clean diagnostic, never a normalization panic.
#[tokio::test]
#[should_panic(expected = "type inference failed")]
async fn wildcard_generic_apply_value_is_rejected() {
    let src = r#"
function id<T>(x: T) -> T { x }
function main() -> int { let f = id<_>; f(5) }
"#;
    let _ = baml_test!(src);
}

// ===========================================================================
// Upcast target: `.as<Show<_>>` is an explicit ascription with no local source
// for the hole → clean diagnostic (mirrors the `is Show<_>` pattern).
// ===========================================================================

/// A `_` in an interface upcast target is rejected cleanly, never a
/// normalization panic.
#[tokio::test]
#[should_panic(expected = "type inference failed")]
async fn wildcard_upcast_target_is_rejected() {
    let src = r#"
interface Show<T> { function show(self) -> T throws never }
class C {
  v int
  implements Show<int> { function show(self) -> int { self.v } }
}
function main() -> int { let c = C { v: 5 }; let s = c.as<Show<_>>; 0 }
"#;
    let _ = baml_test!(src);
}
