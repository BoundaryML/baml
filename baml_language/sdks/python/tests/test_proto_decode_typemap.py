"""Outbound class/enum decode goes through the typemap (25a2 §4.1).

These directly assert the new code path. They build a `BamlTypeMap`
from lazy entries pointing at a test-local Pydantic model, hand-build
a `class_value` proto, and call `decode_value(holder, type_map)`. No
runtime, no sdk_root.
"""

from __future__ import annotations

import pydantic
import pytest

from baml_bridge import BamlError, BamlPyHandle
from baml_bridge.baml_py import _seed_generic_media_handle
from baml_bridge.typemap import BamlTypeMap
from baml_bridge.proto import _try_rehydrate_host_value, decode_value
from baml_bridge.cffi.v1 import baml_outbound_pb2


class _Resume(pydantic.BaseModel):
    name: str


class _AliasedHandle(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(
        arbitrary_types_allowed=True,
        populate_by_name=True,
    )
    field_handle: BamlPyHandle = pydantic.Field(alias="_handle")


def test_decode_value_class_uses_typemap_get_class():
    holder = baml_outbound_pb2.BamlOutboundValue()
    cv = holder.class_value
    cv.name = "user.lorem.Resume"
    f = cv.fields.add()
    f.key = "name"
    f.value.string_value = "Alice"

    tm = BamlTypeMap.from_lazy_entries(
        classes={"user.lorem.Resume": (_Resume.__module__, _Resume.__qualname__)},
        enums={},
        type_aliases={},
    )

    result = decode_value(holder, tm)
    assert isinstance(result, _Resume)
    assert result.name == "Alice"


def test_decode_value_class_unregistered_fqn_raises():
    holder = baml_outbound_pb2.BamlOutboundValue()
    cv = holder.class_value
    cv.name = "user.lorem.Mystery"
    tm = BamlTypeMap()

    with pytest.raises(BamlError, match="Unknown class FQN"):
        decode_value(holder, tm)


def test_decode_value_class_keeps_projected_handle_alias_as_model_field():
    key, handle_type = _seed_generic_media_handle()
    holder = baml_outbound_pb2.BamlOutboundValue()
    cv = holder.class_value
    cv.name = "user.lorem.AliasedHandle"
    field = cv.fields.add()
    field.key = "_handle"
    field.value.handle_value.key = key
    field.value.handle_value.handle_type = handle_type

    tm = BamlTypeMap.from_lazy_entries(
        classes={
            "user.lorem.AliasedHandle": (
                _AliasedHandle.__module__,
                _AliasedHandle.__qualname__,
            )
        },
        enums={},
        type_aliases={},
    )

    result = decode_value(holder, tm)
    assert isinstance(result, _AliasedHandle)
    assert isinstance(result.field_handle, BamlPyHandle)


def test_rehydrate_host_value_reads_projected_handle_alias(monkeypatch):
    handle = object()
    decoded = _AliasedHandle.model_construct(field_handle=handle)
    original = ValueError("original")
    monkeypatch.setattr(
        "baml_bridge.baml_py.lookup_host_value",
        lambda candidate: original if candidate is handle else None,
    )

    assert _try_rehydrate_host_value(decoded) is original
