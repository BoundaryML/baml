"""Minimum repro of the generic-method FFI plumbing bug.

`test_streaming_e2e.py::test_stream_next_reaches_finished` fails with:

    baml_py.BamlClientError: Type mismatch: Value of type 'string'
    does not match any member of union [Void { ... },
    Class(TypeName { name: "StreamFinished", ... })]

`baml.llm.Stream<T, S>.next() -> S | baml.stream.StreamFinished` is a
generic instance method whose return type mentions a class-level
TypeVar. The host-side lowering for that call (`tir2_to_template`)
doesn't substitute the instantiation's `S` into the lifted return
type, so the union still contains `Ty::TypeVar`, which collapses to
`Ty::Void`. The runtime then sees a concrete `string` arrive and
fails to find a member of `[Void, StreamFinished]` that accepts it.

This test isolates the same pattern in a single-shot call, no LLM, no
streams, no `StreamFinished` union — just `WrapperMethods<T>.get_value(self)
-> T` invoked from Python on a `WrapperMethods<string>` instance. If the
fix lands, this test goes green without touching the streaming path.
"""

import pytest


@pytest.mark.skip(
    reason="Phase 4 (engine boundary substitution) not yet landed — "
    "WrapperMethods<T>.get_value_or_marker's `T | WrapperMarker` return type "
    "still lowers `T` to `Ty::Void`, so a concrete `string` payload "
    "fails the union-member check. Tracked in 23a §'Engine boundary "
    "substitution' / 22f. Flip back to enabled when Ty::TypeVar lands."
)
def test_generic():
    """`WrapperMethods<string>.get_value_or_marker()` should still round-trip
    a string when the declared return is `T | WrapperMarker`.

    Equivalent BAML:

        class WrapperMethods<T> {
          value T
          function get_value_or_marker(self) -> T | WrapperMarker {
            self.value
          }
        }

    Mirrors `Stream.next(self) -> S | baml.stream.StreamFinished`: a
    class-level TypeVar fused into a union with a concrete class. If
    the host-side lifting fails to substitute `T → string` for this
    method's return type, `find_matching_member` will reject the
    actual `"hello"` payload with "does not match any member of union
    [Void { ... }, Class(... WrapperMarker ...)]" — the same shape as
    the streaming smoke's error.
    """
    from baml_sdk.generics import make_wrapper_methods

    w = make_wrapper_methods("hello")
    assert w.get_value_or_marker() == "hello"


@pytest.mark.skip(
    reason="Engine now strictly enforces full TypeVar binding on inbound "
    "generic instance-method calls (Gate A). The receiver here comes from a "
    "BAML return value (`make_wrapper_methods`); outbound generic decoding "
    "does not yet preserve the `WrapperMethods<string>` parameterization "
    "(deferred per 00b), so the re-encoded receiver arrives with empty class "
    "type args and the engine correctly rejects the unbound class `T` with "
    "'missing a type binding for type parameter `T`'. A receiver that can't "
    "supply its class type args is a host/SDK gap, not a language fallback to "
    "paper over. Re-enable once outbound decoding sends receiver type args."
)
def test_generic_wrapper_get_value():
    """`WrapperMethods<string>.get_value()` should round-trip a string.

    Equivalent BAML:

        class WrapperMethods<T> {
          value T
          function get_value(self) -> T { self.value }
        }

        function make_wrapper_methods(text: string) -> WrapperMethods<string> {
          WrapperMethods<string> { value: text }
        }

    The engine-side strict path (full-binding Gate A on instance methods)
    requires the receiver to carry its concrete class type args on the wire.
    Until outbound decoding preserves the generic parameterization of a
    returned `WrapperMethods<string>`, the re-encoded receiver has empty
    class args and the call is rejected at the inbound boundary.
    """
    from baml_sdk.generics import make_wrapper_methods

    w = make_wrapper_methods("hello")
    assert w.get_value() == "hello"
