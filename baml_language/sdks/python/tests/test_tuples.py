"""Tuples across the Python boundary: decoded native, sent back with the type
they carry, and stated as tuples (not lists) where no slot fixes a type."""

from __future__ import annotations

import copy
import json
import pickle
import typing

import pydantic

from baml_bridge._host_capture import capture
from baml_bridge.baml_py import _encode_inbound
from baml_bridge.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2, baml_type_pb2
from baml_bridge.proto import BamlTuple, _host_call_handle_keys, decode_value
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


def _prim(ty: baml_type_pb2.BamlTy, kind: int) -> baml_type_pb2.BamlTy:
    ty.primitive.kind = kind
    return ty


def _carried_pair() -> baml_outbound_pb2.BamlOutboundValue:
    """`(1, true)` of the carried type `(int | string, bool)`."""
    holder = baml_outbound_pb2.BamlOutboundValue()
    holder.tuple_value.items.add().int_value = 1
    holder.tuple_value.items.add().bool_value = True
    slot = holder.tuple_value.tuple_type.tuple.items.add()
    _prim(slot.union.options.add(), baml_type_pb2.BAML_TY_PRIMITIVE_INT)
    _prim(slot.union.options.add(), baml_type_pb2.BAML_TY_PRIMITIVE_STRING)
    _prim(
        holder.tuple_value.tuple_type.tuple.items.add(),
        baml_type_pb2.BAML_TY_PRIMITIVE_BOOL,
    )
    return holder


def _stated(value: typing.Any) -> baml_inbound_pb2.InboundValue:
    return baml_inbound_pb2.InboundValue.FromString(_encode_inbound(value, "value"))


def test_a_tuple_decodes_native_and_keeps_its_carried_type():
    decoded = decode_value(_carried_pair(), BamlTypeMap())
    assert isinstance(decoded, tuple)
    assert decoded == (1, True)
    assert repr(decoded) == "(1, True)"
    assert isinstance(decoded, BamlTuple)
    # Every tuple operation makes a plain tuple, which carries nothing.
    assert type(decoded[:]) is tuple
    assert type(tuple(decoded)) is tuple
    assert type(copy.copy(decoded)) is tuple
    assert type(pickle.loads(pickle.dumps(decoded))) is tuple


def test_a_tuple_without_a_carried_type_decodes_to_a_plain_tuple():
    holder = baml_outbound_pb2.BamlOutboundValue()
    holder.tuple_value.items.add().string_value = "a"
    decoded = decode_value(holder, BamlTypeMap())
    assert type(decoded) is tuple
    assert decoded == ("a",)


def test_a_received_tuple_goes_back_with_the_type_it_carries():
    decoded = decode_value(_carried_pair(), BamlTypeMap())
    inbound = _stated(decoded)
    assert inbound.WhichOneof("value") == "list_value"
    assert inbound.value_type == _carried_pair().tuple_value.tuple_type
    assert [item.WhichOneof("value") for item in inbound.list_value.values] == [
        "int_value",
        "bool_value",
    ]


def test_a_native_tuple_states_a_tuple_of_its_items_types_and_a_list_a_list():
    inbound = _stated((1, "a"))
    assert inbound.WhichOneof("value") == "list_value"
    slots = inbound.value_type.tuple.items
    assert [slot.primitive.kind for slot in slots] == [
        baml_type_pb2.BAML_TY_PRIMITIVE_INT,
        baml_type_pb2.BAML_TY_PRIMITIVE_STRING,
    ]
    assert _stated([1, "a"]).value_type.WhichOneof("ty") == "list"
    # A nested tuple is a tuple too, slot by slot.
    nested = _stated(((1, "a"), [True]))
    outer = nested.value_type.tuple.items
    assert outer[0].WhichOneof("ty") == "tuple"
    assert outer[1].WhichOneof("ty") == "list"


def test_cleanup_walks_tuple_items():
    holder = baml_outbound_pb2.BamlOutboundValue()
    item = holder.tuple_value.items.add()
    item.handle_value.key = 77
    item.handle_value.handle_type = 1
    assert list(_host_call_handle_keys(holder)) == [77]


T = typing.TypeVar("T")


class Pair(pydantic.BaseModel, typing.Generic[T]):
    value: T


def test_capture_copies_tuples_as_lists_and_states_tuple_type_arguments():
    # An observation has no tuple value of its own: the items, in order.
    assert json.loads(capture((1, "a"))) == ["list", [["number", 1], ["string", "a"]]]
    saved = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={"user.Pair": (Pair.__module__, Pair.__name__)},
            enums={},
            type_aliases={},
        )
    )
    try:
        observed = json.loads(capture(Pair[typing.Tuple[int, str]](value=(1, "a"))))
    finally:
        set_type_map(saved)
    assert observed[0] == "class"
    assert observed[3] == [["tuple", [["int"], ["string"]]]]
