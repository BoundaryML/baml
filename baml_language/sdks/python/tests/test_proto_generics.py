"""Unit tests for the generic-aware encoder/decoder helpers added in 13b.

These tests exercise the pure-Python helpers in `baml_bridge.proto`
directly — they don't go through the Rust FFI, so they isolate the
serialization changes from the rest of the bridge.

Test matrix (per 13b §7):

1. `_base_class_for_fqn` strips Pydantic generic parameterization
2. Inbound encoder uses the *base* FQN for parameterized instances
3. `_ty_to_python_type` walks a wire `BamlTy` → Python type
4. `_parameterize_tys` applies positional type_args to a Pydantic generic class
5. `_decode_class` returns parameterized instance when args present
6. Graceful degradation when `type_args` is empty (rollout-safe)
7. Nested generics `Crate[List[Box[int]]]` round-trip layer-by-layer
"""

from __future__ import annotations

import sys
import typing
from pathlib import Path
from typing import Generic, List, TypeVar

import pydantic

from baml_bridge.proto import (  # noqa: E402
    _base_class_for_fqn,
    _ty_to_python_type,
    _parameterize_tys,
    _set_inbound_value,
    python_type_to_wire_ty,
)
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map
from baml_bridge.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2, baml_type_pb2


def _set_primitive(kind):
    """Return a setter that marks a wire `BamlTy` as the given primitive kind."""
    def setter(ty):
        ty.primitive.kind = kind
    return setter


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
    """13b §2.1 — `Box[int](item=5)` serializes with `value_type.class_ty.name` set to
    the base class's FQN, not the parameterized form. The runtime here is
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
# _ty_to_python_type — runtime mirror of translate_ty
# ---------------------------------------------------------------------------


def test_baml_ty_int_returns_int():
    ty = baml_type_pb2.BamlTy()
    ty.primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT
    assert _ty_to_python_type(ty, BamlTypeMap()) is int


def test_baml_ty_string_returns_str():
    ty = baml_type_pb2.BamlTy()
    ty.primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_STRING
    assert _ty_to_python_type(ty, BamlTypeMap()) is str


def test_baml_ty_list_int_returns_typing_list_int():
    ty = baml_type_pb2.BamlTy()
    ty.list.item.primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT
    result = _ty_to_python_type(ty, BamlTypeMap())
    # `List[int]` typing-form; `typing.get_args` extracts the parameter.
    assert typing.get_args(result) == (int,)


def test_baml_ty_optional_string_returns_optional_str():
    ty = baml_type_pb2.BamlTy()
    ty.optional.inner.primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_STRING
    result = _ty_to_python_type(ty, BamlTypeMap())
    # Optional[X] is Union[X, None].
    args = set(typing.get_args(result))
    assert str in args
    assert type(None) in args


def test_baml_ty_unknown_returns_typing_any():
    ty = baml_type_pb2.BamlTy()
    ty.unknown.SetInParent()
    assert _ty_to_python_type(ty, BamlTypeMap()) is typing.Any


def test_baml_ty_union_preserves_members():
    # A structural union is preserved as `typing.Union[...]` (not widened to a
    # wildcard) so unions survive the baml->host type translation.
    ty = baml_type_pb2.BamlTy()
    ty.union.options.add().primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT
    ty.union.options.add().primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_STRING
    result = _ty_to_python_type(ty, BamlTypeMap())
    assert typing.get_origin(result) is typing.Union
    assert set(typing.get_args(result)) == {int, str}


def test_baml_ty_union_single_member_unwraps():
    # `typing.Union[X]` collapses to `X`; a one-member wire union decodes to the
    # bare member type, not a one-element union.
    ty = baml_type_pb2.BamlTy()
    ty.union.options.add().primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT
    assert _ty_to_python_type(ty, BamlTypeMap()) is int


def test_baml_ty_union_with_unknown_member_keeps_any_arm():
    # An unbindable member (here: an unknown ty) decodes to `typing.Any` and
    # rides along as a `typing.Any` arm of the union.
    ty = baml_type_pb2.BamlTy()
    ty.union.options.add().primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT
    ty.union.options.add().unknown.SetInParent()
    result = _ty_to_python_type(ty, BamlTypeMap())
    assert typing.get_origin(result) is typing.Union
    assert set(typing.get_args(result)) == {int, typing.Any}


# ---------------------------------------------------------------------------
# Generic-over-union round trip: `Box[int | str]` survives the boundary
# ---------------------------------------------------------------------------


def test_generic_over_union_round_trips():
    """A generic instantiated over a union — `Box[int | str]` — must survive
    the host->wire->host type translation without the union collapsing to a
    wildcard. The global typemap is swapped so the encoder can resolve `Box`'s
    FQN (the wire `class_ty.name`) and the decoder can resolve it back."""
    py_type = Box[typing.Union[int, str]]
    tm = _fresh_typemap_with(Box)
    saved = get_type_map()
    set_type_map(tm)
    try:
        wire = python_type_to_wire_ty(py_type)
    finally:
        set_type_map(saved)

    # Wire shape: a `class_ty` for Box carrying a single union type-arg whose
    # members are the int/str primitives.
    assert wire.WhichOneof("ty") == "class_ty"
    assert wire.class_ty.name == "user.lorem.Box"
    assert len(wire.class_ty.type_args) == 1
    union_arg = wire.class_ty.type_args[0]
    assert union_arg.WhichOneof("ty") == "union"
    member_kinds = {opt.primitive.kind for opt in union_arg.union.options}
    assert member_kinds == {
        baml_type_pb2.BAML_TY_PRIMITIVE_INT,
        baml_type_pb2.BAML_TY_PRIMITIVE_STRING,
    }

    # Decode back: `Box[int | str]` reconstructed, union members intact.
    # Pydantic caches `cls[args]`, so the reconstructed class is identical, and
    # the bound arg lives in the pydantic generic metadata (not typing.get_args,
    # which is empty for a pydantic generic subclass).
    decoded = _ty_to_python_type(wire, tm)
    assert decoded is Box[typing.Union[int, str]]
    meta = decoded.__pydantic_generic_metadata__
    assert meta["origin"] is Box
    assert len(meta["args"]) == 1
    union_member = meta["args"][0]
    assert typing.get_origin(union_member) is typing.Union
    assert set(typing.get_args(union_member)) == {int, str}


# ---------------------------------------------------------------------------
# _parameterize_tys — applies positional type_args to a Pydantic generic class
# ---------------------------------------------------------------------------


def _wire_ty(ty_setter):
    """Build a wire `BamlTy` whose variant is set by the caller."""
    ty = baml_type_pb2.BamlTy()
    ty_setter(ty)
    return ty


def test_parameterize_no_args_returns_base():
    assert _parameterize_tys(Box, [], BamlTypeMap()) is Box


def test_parameterize_single_arg_int():
    args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_INT))]
    result = _parameterize_tys(Box, args, BamlTypeMap())
    # Pydantic v2 caches `cls[args]`, so two calls produce the same class.
    assert result is Box[int]


def test_parameterize_falls_back_for_non_generic_class():
    args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_INT))]
    # Plain has no TypeVars; parameterization is a no-op.
    assert _parameterize_tys(Plain, args, BamlTypeMap()) is Plain


def test_parameterize_applies_to_generic_type_alias():
    """Generic type aliases (codegen emits `StringList: TypeAlias = List[T]`)
    must parameterize the same way pydantic generics do — the symbol is
    not a class, but it's still subscriptable."""
    StringList = List[T]  # type: ignore[reportGeneralTypeIssues]  # TypeVar in function body — valid at runtime
    args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_INT))]
    assert _parameterize_tys(StringList, args, BamlTypeMap()) == List[int]


def test_parameterize_falls_back_for_fully_bound_alias():
    # Already-concrete alias has no TypeVars left to bind — try/except
    # catches the TypeError and returns the alias unchanged.
    Concrete = List[int]
    args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_STRING))]
    assert _parameterize_tys(Concrete, args, BamlTypeMap()) is Concrete


# ---------------------------------------------------------------------------
# _decode_class behavior (without runtime — patches `_resolve_type`)
# ---------------------------------------------------------------------------


def _build_class_value(fqn: str, fields: list, type_args: list):
    """Build a `BamlValueClass` from `(key, value)` field pairs. Outbound
    fields use `BamlOutboundMapEntry` whose `value` is a
    `BamlOutboundValue`. `type_args` is a list of wire `BamlTy`."""
    cv = baml_outbound_pb2.BamlValueClass()
    cv.name = fqn
    for ty in type_args:
        cv.type_args.add().CopyFrom(ty)
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
    """13b §3.1, §3.4 — a generic class with `type_args = [int]` decodes
    to a `Box[int]` instance, not bare `Box`."""
    from baml_bridge import proto as proto_mod

    tm = _fresh_typemap_with(Box)

    args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_INT))]
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
    `type_args` is empty. Decode still produces a usable instance —
    just unparameterized."""
    from baml_bridge import proto as proto_mod

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
    from baml_bridge import proto as proto_mod

    tm = _fresh_typemap_with(Box, Crate)

    box_args = [_wire_ty(_set_primitive(baml_type_pb2.BAML_TY_PRIMITIVE_INT))]
    box_a = _build_class_value("user.lorem.Box", [("item", 5)], box_args)
    box_b = _build_class_value("user.lorem.Box", [("item", 6)], box_args)

    # Crate's T is the element type — `Crate<Box<int>>` means
    # `contents: List[Box[int]]`.
    def set_outer_arg(ty):
        ty.class_ty.name = "user.lorem.Box"
        inner = ty.class_ty.type_args.add()
        inner.primitive.kind = baml_type_pb2.BAML_TY_PRIMITIVE_INT

    crate_cv = _build_class_value("user.lorem.Crate", [], [_wire_ty(set_outer_arg)])
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


# ---------------------------------------------------------------------------
# Phase 2/4: generic instance args carry sparse `value_type`; `_types=` is dict-only
# ---------------------------------------------------------------------------

import pytest  # noqa: E402

from baml_bridge import _resolve_types_kwarg  # noqa: E402
from baml_bridge.cffi.v1 import baml_type_pb2  # noqa: E402


def test_generic_instance_carries_sparse_value_type():
    """A generic instance argument (`Box[int]`) carries its concrete class type
    args in the node-level `value_type` channel."""
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, Box[int](item=5), kwarg_name="x")
    assert inbound.HasField("value_type")
    assert len(inbound.value_type.class_ty.type_args) == 1
    assert inbound.value_type.class_ty.type_args[0].primitive.kind == baml_type_pb2.BAML_TY_PRIMITIVE_INT


def test_unbound_generic_instance_carries_nominal_sparse_value_type():
    """An erased generic keeps nominal identity while omitting unknown args."""
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, Box(item=5), kwarg_name="x")
    assert inbound.WhichOneof("value") == "class_value"
    assert inbound.HasField("value_type")
    assert len(inbound.value_type.class_ty.type_args) == 0


def test_non_generic_instance_value_type_has_no_type_args():
    """A non-generic instance still binds its class via `value_type` (the FQN
    channel, now the sole class-name source) but carries no type args."""
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, Plain(a=1), kwarg_name="x")
    assert inbound.HasField("value_type")
    assert len(inbound.value_type.class_ty.type_args) == 0


def test_resolve_types_requires_dict_for_generic():
    with pytest.raises(TypeError):
        _resolve_types_kwarg(None, ["T"])  # required
    with pytest.raises(TypeError):
        _resolve_types_kwarg(int, ["T"])  # single-type form gone
    with pytest.raises(TypeError):
        _resolve_types_kwarg((int, str), ["A", "B"])  # positional form gone


def test_resolve_types_missing_and_extra_keys():
    with pytest.raises(TypeError):
        _resolve_types_kwarg({"A": int}, ["A", "B"])  # missing B
    with pytest.raises(TypeError):
        _resolve_types_kwarg({"A": int, "Z": str}, ["A"])  # unknown Z


def test_resolve_types_dict_maps_by_name_in_declaration_order():
    # Keyed by name, returned in the callee's declaration order.
    assert _resolve_types_kwarg({"B": str, "A": int}, ["A", "B"]) == [int, str]


def test_resolve_types_empty_params_rejects_types_kwarg():
    assert _resolve_types_kwarg(None, []) == []
    with pytest.raises(TypeError):
        _resolve_types_kwarg({"T": int}, [])  # no own params to bind


# ---------------------------------------------------------------------------
# Phase 6: `fn[...]` subscript desugars to the `_types={...}` dict form
# ---------------------------------------------------------------------------

from baml_bridge import _GenericCallable  # noqa: E402


def test_generic_callable_subscript_desugars_to_types_dict():
    captured = {}

    def fake_call(*args, **kwargs):
        captured["args"] = args
        captured["kwargs"] = kwargs
        return "ok"

    fn = _GenericCallable(fake_call, ["A", "B"])
    # Multiple type args bind positionally by declaration order.
    assert fn[int, str]() == "ok"
    assert captured["kwargs"]["_types"] == {"A": int, "B": str}

    # Single type arg (non-tuple subscript).
    one = _GenericCallable(fake_call, ["T"])
    one[bool]()
    assert captured["kwargs"]["_types"] == {"T": bool}


def test_generic_callable_subscript_arity_mismatch_raises():
    fn = _GenericCallable(lambda **k: None, ["A", "B"])
    import pytest

    with pytest.raises(TypeError):
        fn[int]()  # needs two


def test_generic_callable_explicit_types_still_works():
    captured = {}

    def fake_call(**kwargs):
        captured.update(kwargs)

    fn = _GenericCallable(fake_call, ["T"])
    fn(_types={"T": int})
    assert captured["_types"] == {"T": int}
