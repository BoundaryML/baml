"""Unit tests for the generic-aware encoder/decoder helpers added in 13b.

These tests exercise the pure-Python helpers in `baml_core.proto`
directly — they don't go through the Rust FFI, so they isolate the
serialization changes from the rest of the bridge.

Test matrix (per 13b §7):

1. `_base_class_for_fqn` strips Pydantic generic parameterization
2. Inbound encoder uses the *base* FQN for parameterized instances
3. `_baml_ty_to_python_type` walks BamlTy → Python type
4. `_parameterize` applies generic args to a Pydantic generic class
5. `_decode_class` returns parameterized instance when args present
6. Graceful degradation when `generic_args` is empty (rollout-safe)
7. Nested generics `Crate[List[Box[int]]]` round-trip layer-by-layer
"""

from __future__ import annotations

import sys
import typing
from pathlib import Path
from typing import Generic, List, TypeVar

import pydantic

from baml_core.proto import (  # noqa: E402
    _base_class_for_fqn,
    _baml_ty_to_python_type,
    _parameterize,
    _set_inbound_value,
)
from baml_core.typemap import BamlTypeMap
from baml_core.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2


# Make sim sdk_root importable (needed indirectly for `_resolve_type`-
# free helpers). We don't initialize the runtime here — these tests
# operate on the proto helpers directly with hand-rolled messages.
SIM_ROOT = Path(__file__).parent / "sim"
if str(SIM_ROOT) not in sys.path:
    sys.path.insert(0, str(SIM_ROOT))

T = TypeVar("T")


class Box(pydantic.BaseModel, Generic[T]):
    model_config = pydantic.ConfigDict(extra="forbid")
    item: T


class Crate(pydantic.BaseModel, Generic[T]):
    model_config = pydantic.ConfigDict(extra="forbid")
    contents: List[T]


class Plain(pydantic.BaseModel):
    a: int


_FQN_BY_NAME = {
    "Box": "user.lorem.Box",
    "Crate": "user.lorem.Crate",
    "Plain": "user.lorem.Plain",
}


def _fresh_typemap_with(*classes: type) -> BamlTypeMap:
    """25b2: build a `BamlTypeMap` from lazy entries pointing at the
    test-local class definitions."""
    return BamlTypeMap.from_lazy_entries(
        classes={
            _FQN_BY_NAME[cls.__name__]: (cls.__module__, cls.__name__)
            for cls in classes
        },
        enums={},
        type_aliases={},
    )


# ---------------------------------------------------------------------------
# _base_class_for_fqn
# ---------------------------------------------------------------------------


def test_base_class_for_fqn_strips_parameterization():
    parameterized = Box[int]
    assert _base_class_for_fqn(parameterized) is Box


def test_base_class_for_fqn_passes_non_generic_through():
    assert _base_class_for_fqn(Plain) is Plain


# ---------------------------------------------------------------------------
# Inbound encoder uses base FQN
# ---------------------------------------------------------------------------


def test_inbound_class_value_carries_base_fqn():
    """13b §2.1 — `Box[int](item=5)` serializes with `cv.name` set to the
    base class's FQN, not the parameterized form. The runtime here is
    not initialized so `_derive_baml_fqn` returns ""; what matters is that
    the encoder reaches the Pydantic branch with the *base* class on the
    `_derive_baml_fqn` call. Easiest assertion: the encode succeeds and
    the resulting class_value's `fields` round-trip the data."""
    inbound = baml_inbound_pb2.InboundValue()
    box = Box[int](item=5)
    _set_inbound_value(inbound, box, kwarg_name="x")
    assert inbound.WhichOneof("value") == "class_value"
    cv = inbound.class_value
    # Without sdk_root set, FQN derivation returns "" — but it must not
    # raise, and the field encoding must still happen.
    assert len(cv.fields) == 1
    assert cv.fields[0].string_key == "item"
    assert cv.fields[0].value.int_value == 5


# ---------------------------------------------------------------------------
# _baml_ty_to_python_type — runtime mirror of translate_ty
# ---------------------------------------------------------------------------


def test_baml_ty_int_returns_int():
    ty = baml_outbound_pb2.BamlTy()
    ty.int_type.SetInParent()
    assert _baml_ty_to_python_type(ty, BamlTypeMap()) is int


def test_baml_ty_string_returns_str():
    ty = baml_outbound_pb2.BamlTy()
    ty.string_type.SetInParent()
    assert _baml_ty_to_python_type(ty, BamlTypeMap()) is str


def test_baml_ty_list_int_returns_typing_list_int():
    ty = baml_outbound_pb2.BamlTy()
    ty.list_type.item_type.int_type.SetInParent()
    result = _baml_ty_to_python_type(ty, BamlTypeMap())
    # `List[int]` typing-form; `typing.get_args` extracts the parameter.
    assert typing.get_args(result) == (int,)


def test_baml_ty_optional_string_returns_optional_str():
    ty = baml_outbound_pb2.BamlTy()
    ty.optional_type.value.string_type.SetInParent()
    result = _baml_ty_to_python_type(ty, BamlTypeMap())
    # Optional[X] is Union[X, None].
    args = set(typing.get_args(result))
    assert str in args
    assert type(None) in args


def test_baml_ty_any_returns_typing_any():
    ty = baml_outbound_pb2.BamlTy()
    ty.any_type.SetInParent()
    assert _baml_ty_to_python_type(ty, BamlTypeMap()) is typing.Any


# ---------------------------------------------------------------------------
# _parameterize — applies generic_args to a Pydantic generic class
# ---------------------------------------------------------------------------


def _generic_arg(name: str, ty_setter):
    """Build a `BamlTyGenericArg` whose inner ty is set by the caller."""
    arg = baml_outbound_pb2.BamlTyGenericArg()
    arg.name = name
    ty_setter(arg.ty)
    return arg


def test_parameterize_no_args_returns_base():
    assert _parameterize(Box, [], BamlTypeMap()) is Box


def test_parameterize_single_arg_int():
    args = [_generic_arg("T", lambda ty: ty.int_type.SetInParent())]
    result = _parameterize(Box, args, BamlTypeMap())
    # Pydantic v2 caches `cls[args]`, so two calls produce the same class.
    assert result is Box[int]


def test_parameterize_falls_back_for_non_generic_class():
    args = [_generic_arg("T", lambda ty: ty.int_type.SetInParent())]
    # Plain has no TypeVars; parameterization is a no-op.
    assert _parameterize(Plain, args, BamlTypeMap()) is Plain


def test_parameterize_applies_to_generic_type_alias():
    """Generic type aliases (codegen emits `StringList: TypeAlias = List[T]`)
    must parameterize the same way pydantic generics do — the symbol is
    not a class, but it's still subscriptable."""
    StringList = List[T]  # type: ignore[reportGeneralTypeIssues]  # TypeVar in function body — valid at runtime
    args = [_generic_arg("T", lambda ty: ty.int_type.SetInParent())]
    assert _parameterize(StringList, args, BamlTypeMap()) == List[int]


def test_parameterize_falls_back_for_fully_bound_alias():
    # Already-concrete alias has no TypeVars left to bind — try/except
    # catches the TypeError and returns the alias unchanged.
    Concrete = List[int]
    args = [_generic_arg("T", lambda ty: ty.string_type.SetInParent())]
    assert _parameterize(Concrete, args, BamlTypeMap()) is Concrete


# ---------------------------------------------------------------------------
# _decode_class behavior (without runtime — patches `_resolve_type`)
# ---------------------------------------------------------------------------


def _build_class_value(fqn: str, fields: list, generic_args: list):
    """Build a `BamlValueClass` from `(key, value)` field pairs. Outbound
    fields use `BamlOutboundMapEntry` whose `value` is a
    `BamlOutboundValue`."""
    cv = baml_outbound_pb2.BamlValueClass()
    cv.name.name = fqn
    for arg in generic_args:
        cv.name.generic_args.add().CopyFrom(arg)
    for k, v in fields:
        entry = cv.fields.add()
        entry.key = k
        if isinstance(v, bool):
            entry.value.bool_value = v
        elif isinstance(v, int):
            entry.value.int_value = v
        else:
            raise NotImplementedError(f"unsupported test field value: {v!r}")
    return cv


def test_decode_class_parameterizes_with_generic_args():
    """13b §3.1, §3.4 — a generic class with `generic_args = [int]` decodes
    to a `Box[int]` instance, not bare `Box`."""
    from baml_core import proto as proto_mod

    tm = _fresh_typemap_with(Box)

    args = [_generic_arg("T", lambda ty: ty.int_type.SetInParent())]
    cv = _build_class_value("user.lorem.Box", [("item", 5)], args)
    result = proto_mod._decode_class(cv, tm)

    # Parameterized instance: isinstance against the base still holds.
    assert isinstance(result, Box)
    assert result.item == 5
    # Type[result] is the parameterized subclass (`Box[int]`), so its
    # origin is Box.
    meta = type(result).__pydantic_generic_metadata__
    assert meta["origin"] is Box
    assert meta["args"] == (int,)


def test_decode_class_graceful_degradation_when_args_empty():
    """13b §3.5 — when the Rust producer hasn't been updated yet,
    `generic_args` is empty. Decode still produces a usable instance —
    just unparameterized."""
    from baml_core import proto as proto_mod

    tm = _fresh_typemap_with(Box)

    cv = _build_class_value("user.lorem.Box", [("item", 5)], [])
    result = proto_mod._decode_class(cv, tm)
    assert isinstance(result, Box)
    assert result.item == 5


def test_decode_class_nested_generic():
    """13b §4 — a generic class containing generic instances decodes
    layer-by-layer; the inner `Box[int]` is parameterized before the
    outer `Crate.model_validate` runs, and isinstance holds at each
    layer.

    Models the `Crate<Box<int>>` shape (Crate's T is bound to `Box<int>`,
    so `contents: List[T]` becomes `List[Box[int]]`).
    """
    from baml_core import proto as proto_mod

    tm = _fresh_typemap_with(Box, Crate)

    box_args = [_generic_arg("T", lambda ty: ty.int_type.SetInParent())]
    box_a = _build_class_value("user.lorem.Box", [("item", 5)], box_args)
    box_b = _build_class_value("user.lorem.Box", [("item", 6)], box_args)

    # Crate's T is the element type — `Crate<Box<int>>` means
    # `contents: List[Box[int]]`.
    def set_outer_arg(ty):
        ty.class_type.name.name = "user.lorem.Box"
        inner = ty.class_type.name.generic_args.add()
        inner.name = "T"
        inner.ty.int_type.SetInParent()

    crate_args = [_generic_arg("T", set_outer_arg)]
    crate_cv = baml_outbound_pb2.BamlValueClass()
    crate_cv.name.name = "user.lorem.Crate"
    for a in crate_args:
        crate_cv.name.generic_args.add().CopyFrom(a)
    contents_field = crate_cv.fields.add()
    contents_field.key = "contents"
    for box_cv in (box_a, box_b):
        contents_field.value.list_value.items.add().class_value.CopyFrom(box_cv)

    result = proto_mod._decode_class(crate_cv, tm)
    assert isinstance(result, Crate)
    assert len(result.contents) == 2
    for item in result.contents:
        assert isinstance(item, Box)
    assert [b.item for b in result.contents] == [5, 6]
