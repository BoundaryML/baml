"""Tests for opaque host-only values bound to generic positions
(bridge generics, HOST_VALUE_OPAQUE).

A Python object with no BAML representation — not a primitive, container,
pydantic model, callable, or media value — is registered in the host-value
table and crosses the bridge as an opaque handle. BAML treats it as sealed
(`==`/`!=` are host-object identity); the outbound decoder rehydrates the
*original* Python object by key.

Run with:
    cd baml_language/sdks/python
    uv run maturin develop --uv
    uv run pytest tests/test_host_opaque.py -v
"""
from __future__ import annotations

import pytest

from baml_core import (
    BamlRuntime,
    call_function_sync,
)
from baml_core.errors import BamlPanic


GENERICS_BAML = """\
function Identity<T>(x: T) -> T {
    x
}

function Eq<T>(a: T, b: T) -> bool {
    a == b
}

function First<T>(items: T[]) -> T {
    items[0]
}

function WrapUnwrap<T>(x: T) -> T {
    let w = Wrapper { value: x };
    w.value
}

class Wrapper<T> {
    value T
}

function WantsString(x: string) -> string {
    x
}
"""


class MyHostOnly:
    """An arbitrary host class with no BAML counterpart."""

    def __init__(self, tag: str) -> None:
        self.tag = tag


def _make_runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": GENERICS_BAML})


def test_opaque_identity_roundtrip_is_same_object():
    rt = _make_runtime()
    obj = MyHostOnly("a")
    result = call_function_sync(rt, "Identity", {"x": obj})
    out = result.result()
    assert out is obj  # identity, not a copy


def test_opaque_identity_preserves_python_type():
    rt = _make_runtime()
    obj = MyHostOnly("b")
    out = call_function_sync(rt, "Identity", {"x": obj}).result()
    assert isinstance(out, MyHostOnly)
    assert out.tag == "b"


def test_primitives_still_roundtrip_through_generic():
    rt = _make_runtime()
    assert call_function_sync(rt, "Identity", {"x": 5}).result() == 5
    assert call_function_sync(rt, "Identity", {"x": "hi"}).result() == "hi"
    assert call_function_sync(rt, "Identity", {"x": [1, 2]}).result() == [1, 2]


def test_opaque_equality_is_host_identity():
    rt = _make_runtime()
    obj = MyHostOnly("c")
    other = MyHostOnly("c")
    assert call_function_sync(rt, "Eq", {"a": obj, "b": obj}).result() is True
    assert call_function_sync(rt, "Eq", {"a": obj, "b": other}).result() is False


def test_opaque_in_list_roundtrips():
    rt = _make_runtime()
    first = MyHostOnly("first")
    second = MyHostOnly("second")
    out = call_function_sync(rt, "First", {"items": [first, second]}).result()
    assert out is first


def test_opaque_through_generic_class_field():
    rt = _make_runtime()
    obj = MyHostOnly("field")
    out = call_function_sync(rt, "WrapUnwrap", {"x": obj}).result()
    assert out is obj


def test_opaque_rejected_at_concrete_string_param():
    rt = _make_runtime()
    obj = MyHostOnly("nope")
    # Boundary type mismatches surface as BamlPanic (a BaseException, like
    # SystemExit) — same as any other host-boundary type error today.
    with pytest.raises(BamlPanic, match="host-only value"):
        call_function_sync(rt, "WantsString", {"x": obj}).result()
