//! Minimum repro of the generic-method FFI plumbing bug.
//!
//! `test_streaming_e2e.rs::test_stream_next_reaches_finished` fails with:
//!
//!     Type mismatch: Value of type 'string'
//!     does not match any member of union [Void { ... },
//!     Class(TypeName { name: "Done", ... })]
//!
//! `ai.stream.Stream<T, S>.next() -> S | ai.stream.Done` is a
//! generic instance method whose return type mentions a class-level
//! TypeVar. The host-side lowering for that call (`tir2_to_template`)
//! doesn't substitute the instantiation's `S` into the lifted return
//! type, so the union still contains `Ty::TypeVar`, which collapses to
//! `Ty::Void`. The runtime then sees a concrete `string` arrive and
//! fails to find a member of `[Void, Done]` that accepts it.
//!
//! This test isolates the same pattern in a single-shot call, no LLM, no
//! streams, no `Done` union — just `WrapperMethods<T>.get_value(self)
//! -> T` invoked from Rust on a `WrapperMethods<string>` instance. If the
//! fix lands, this test goes green without touching the streaming path.

/// `WrapperMethods<string>.get_value_or_marker()` should still round-trip
/// a string when the declared return is `T | WrapperMarker`.
///
/// Equivalent BAML:
///
///     class WrapperMethods<T> {
///       value T
///       function get_value_or_marker(self) -> T | WrapperMarker {
///         self.value
///       }
///     }
///
/// Mirrors `Stream.next(self) -> S | ai.stream.Done`: a
/// class-level TypeVar fused into a union with a concrete class. If
/// the host-side lifting fails to substitute `T → string` for this
/// method's return type, `find_matching_member` will reject the
/// actual `"hello"` payload with "does not match any member of union
/// [Void { ... }, Class(... WrapperMarker ...)]" — the same shape as
/// the streaming smoke's error.
#[test]
fn test_generic_generic() {
    // ADAPTATION(rust): the anonymous `T | WrapperMarker` return union
    // synthesizes the arm-named generic enum `TOrWrapperMarker<T>`; with
    // `T = String` the returned string decodes into the `T` variant (decode
    // trial order is declaration order).
    use baml_sdk::generics::{TOrWrapperMarker, make_wrapper_methods};

    let w = make_wrapper_methods("hello".to_string()).unwrap();
    assert_eq!(
        w.get_value_or_marker().unwrap(),
        TOrWrapperMarker::T("hello".to_string())
    );
}

/// `WrapperMethods<string>.get_value()` should round-trip a string.
///
/// Equivalent BAML:
///
///     class WrapperMethods<T> {
///       value T
///       function get_value(self) -> T { self.value }
///     }
///
///     function make_wrapper_methods(text: string) -> WrapperMethods<string> {
///       WrapperMethods<string> { value: text }
///     }
///
/// The engine-side strict path (full-binding Gate A on instance methods)
/// requires the receiver to carry its concrete class type args on the wire.
/// Until outbound decoding preserves the generic parameterization of a
/// returned `WrapperMethods<string>`, the re-encoded receiver has empty
/// class args and the call is rejected at the inbound boundary.
#[test]
fn test_generic_generic_wrapper_get_value() {
    use baml_sdk::generics::make_wrapper_methods;

    let w = make_wrapper_methods("hello".to_string()).unwrap();
    assert_eq!(w.get_value().unwrap(), "hello");
}
