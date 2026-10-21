"""A declaration the runtime creates crosses to Python and back as itself.

A type value naming a runtime class leaves the engine with each runtime
declaration referenced by a row the host holds, and comes back, as a value or
as an explicit type argument, referencing that same declaration. A runtime
class spelled like a program class never stands in for it: names are never
identities.
"""

from __future__ import annotations

import gc

from baml_bridge import BamlRuntime, BamlType, FunctionResult, call_function_sync
from baml_bridge.baml_py import _live_handle_count, new_function_call
from baml_bridge.proto import decode_call_result, encode_call_args

SOURCE = """
class Foo {
    y string
}

function MakeFoo() -> reflect.Type {
    reflect.Class.new("Foo", { "x": int.type() }).as_type()
}

function SameType(a: reflect.Type, b: reflect.Type) -> bool {
    a == b
}

function IsProgramFoo(t: reflect.Type) -> bool {
    t == Foo.type()
}

function BoundIsProgramFoo<T>() -> bool {
    reflect.Type.of<T>() == Foo.type()
}

function BoundSame<T>(t: reflect.Type) -> bool {
    reflect.Type.of<T>() == t
}
"""


def _runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})


def _call_with_types(rt: BamlRuntime, name: str, kwargs: dict, types: list) -> object:
    encoded = encode_call_args(kwargs, new_function_call(), types, function_name=name)
    return FunctionResult(
        decode_call_result(rt.call_function_sync(encoded, encoded.kwargs, encoded.cancel))
    ).result()


def test_a_runtime_type_value_round_trips_as_the_same_declaration():
    rt = _runtime()
    made = call_function_sync(rt, "MakeFoo", {}).result()
    assert isinstance(made, BamlType)
    assert call_function_sync(rt, "SameType", {"a": made, "b": made}).result() is True
    assert call_function_sync(rt, "SameType", {"a": made, "b": made.array()}).result() is False


def test_a_runtime_class_spelled_like_a_program_class_is_not_it():
    rt = _runtime()
    made = call_function_sync(rt, "MakeFoo", {}).result()
    assert call_function_sync(rt, "IsProgramFoo", {"t": made}).result() is False
    assert _call_with_types(rt, "BoundIsProgramFoo", {}, [("T", made)]) is False


def test_an_explicit_type_argument_references_the_declaration_it_names():
    rt = _runtime()
    made = call_function_sync(rt, "MakeFoo", {}).result()
    assert _call_with_types(rt, "BoundSame", {"t": made}, [("T", made)]) is True


def test_sending_a_type_keeps_the_hosts_rows_and_releases_the_calls():
    rt = _runtime()
    made = call_function_sync(rt, "MakeFoo", {}).result()
    gc.collect()
    held = _live_handle_count()
    for _ in range(3):
        _call_with_types(rt, "BoundSame", {"t": made}, [("T", made)])
    gc.collect()
    # Every call stated the declaration with references of its own; the engine
    # drained those, and the wrapper still holds exactly the rows it had.
    assert _live_handle_count() == held
    del made
    gc.collect()
    assert _live_handle_count() < held
