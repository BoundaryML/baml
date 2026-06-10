"""Tests for explicit generic type arguments ($types / BEP-039) crossing
the bridge as `CallFunctionArgs.type_args`.

Binding a TypeVar explicitly makes its positions behave exactly as if the
signature had been declared with the concrete type: it drives
behavior-dependent generics (`baml.json.from_string<T>`), runtime
reflection (`reflect.type_of<T>()`), boundary coercion (int→bigint
widening, untagged-map→class promotion), and the entry frame's type_args.

Run with:
    cd baml_language/sdks/python
    uv run maturin develop --uv
    uv run pytest tests/test_type_args.py -v
"""
from __future__ import annotations

import typing

import pytest

from baml_core import (
    BamlRuntime,
    call_function_sync,
)
from baml_core.errors import BamlError


TYPE_ARGS_BAML = """\
function TypeNameOf<T>() -> string {
    reflect.type_of<T>().to_string()
}

function ParseAs<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    baml.json.from_string<T>(s)
}

function Identity<T>(x: T) -> T {
    x
}

class Profile {
    name string
    age int
}
"""


def _make_runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": TYPE_ARGS_BAML})


def test_reflect_sees_explicit_type_arg():
    rt = _make_runtime()
    out = call_function_sync(rt, "TypeNameOf", {}, type_args=[str]).result()
    assert out == "string"


def test_reflect_unbound_defaults_to_unknown():
    rt = _make_runtime()
    out = call_function_sync(rt, "TypeNameOf", {}).result()
    assert out == "unknown"


def test_parse_as_int_binds_parse_target():
    rt = _make_runtime()
    out = call_function_sync(rt, "ParseAs", {"s": "42"}, type_args=[int]).result()
    assert out == 42


def test_parse_as_class_binds_parse_target():
    rt = _make_runtime()
    out = call_function_sync(
        rt,
        "ParseAs",
        {"s": '{"name": "ada", "age": 36}'},
        type_args=["Profile"],
    ).result()
    # Decoded as the generated-class shape: a pydantic-like object or dict
    # depending on typemap registration; both expose the fields.
    name = out.name if hasattr(out, "name") else out["name"]
    age = out.age if hasattr(out, "age") else out["age"]
    assert name == "ada"
    assert age == 36


def test_identity_with_bigint_binding_widens():
    rt = _make_runtime()
    out = call_function_sync(rt, "Identity", {"x": 5}, type_args=["bigint"]).result()
    assert out == 5
    assert isinstance(out, int)


def test_identity_with_class_binding_promotes_map():
    rt = _make_runtime()
    out = call_function_sync(
        rt,
        "Identity",
        {"x": {"name": "ada", "age": 36}},
        type_args=["Profile"],
    ).result()
    name = out.name if hasattr(out, "name") else out["name"]
    assert name == "ada"


def test_structured_tokens_list_of_int():
    rt = _make_runtime()
    out = call_function_sync(
        rt, "ParseAs", {"s": "[1, 2, 3]"}, type_args=[typing.List[int]]
    ).result()
    assert out == [1, 2, 3]


def test_unknown_type_name_errors():
    rt = _make_runtime()
    with pytest.raises(BaseException, match="unknown type"):
        call_function_sync(rt, "TypeNameOf", {}, type_args=["NoSuchType"]).result()
