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
def test_generic_wrapper_get_value_or_marker():
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
    from baml_sdk.generics import MakeWrapperMethods

    w = MakeWrapperMethods("hello")
    assert w.get_value_or_marker() == "hello"


def test_generic_wrapper_get_value():
    """`WrapperMethods<string>.get_value()` should round-trip a string.

    Equivalent BAML:

        class WrapperMethods<T> {
          value T
          function get_value(self) -> T { self.value }
        }

        function MakeWrapperMethods(text: string) -> WrapperMethods<string> {
          WrapperMethods<string> { value: text }
        }

    On the buggy path the lifted return type for `get_value` is
    `Ty::Void` (TypeVar `T` never gets substituted with `string`), so
    decoding the actual `"hello"` payload raises a BamlClientError
    with the "does not match any member of union [Void { ... }]"
    shape from the issue description.
    """
    from baml_sdk.generics import MakeWrapperMethods

    w = MakeWrapperMethods("hello")
    assert w.get_value() == "hello"
