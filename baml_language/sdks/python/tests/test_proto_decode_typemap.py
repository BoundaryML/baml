"""Outbound class/enum decode goes through the typemap (25a2 §4.1).

These directly assert the new code path. They build a `BamlTypeMap`
from lazy entries pointing at a test-local Pydantic model, hand-build
a `class_value` proto, and call `decode_value(holder, type_map)`. No
runtime, no sdk_root.
"""
from __future__ import annotations

import pydantic
import pytest

from baml_bridge import BamlError
from baml_bridge.typemap import BamlTypeMap
from baml_bridge.proto import decode_value
from baml_bridge.cffi.v1 import baml_outbound_pb2


class _Resume(pydantic.BaseModel):
    name: str


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
