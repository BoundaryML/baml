"""Outbound class/enum decode goes through the typemap (25a2 §4.1).

These directly assert the new code path. They build a `BamlTypeMap`
from lazy entries pointing at a test-local Pydantic model, hand-build
a `class_value` proto, and call `decode_value(holder, type_map)`. No
runtime, no sdk_root.
"""

from __future__ import annotations

import pydantic
import pytest

from baml_bridge import BamlError, BamlPanic, BamlPyHandle
from baml_bridge.baml_py import _seed_generic_media_handle
from baml_bridge.typemap import BamlTypeMap
from baml_bridge.proto import _try_rehydrate_host_value, decode_call_result, decode_value
from baml_bridge.cffi.v1 import baml_outbound_pb2


class _Resume(pydantic.BaseModel):
    name: str


class _AliasedHandle(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(
        arbitrary_types_allowed=True,
        populate_by_name=True,
    )
    field_handle: BamlPyHandle = pydantic.Field(alias="_handle")


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
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


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_decode_value_class_unregistered_fqn_raises():
    holder = baml_outbound_pb2.BamlOutboundValue()
    cv = holder.class_value
    cv.name = "user.lorem.Mystery"
    tm = BamlTypeMap()

    with pytest.raises(BamlError, match="Unknown class FQN"):
        decode_value(holder, tm)


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
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


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_rehydrate_host_value_reads_projected_handle_alias(monkeypatch):
    handle = object()
    decoded = _AliasedHandle.model_construct(field_handle=handle)
    original = ValueError("original")
    monkeypatch.setattr(
        "baml_bridge.baml_py.lookup_host_value",
        lambda candidate: original if candidate is handle else None,
    )

    assert _try_rehydrate_host_value(decoded) is original


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_rehydrate_host_value_reads_handle_of_undecoded_fields(monkeypatch):
    """Without a generated `baml.errors.HostCallable`, the thrown value is the
    dict of its fields."""
    handle = object()
    original = ValueError("original")
    monkeypatch.setattr(
        "baml_bridge.baml_py.lookup_host_value",
        lambda candidate: original if candidate is handle else None,
    )

    assert _try_rehydrate_host_value({"message": "ValueError: original", "_handle": handle}) is original
    assert _try_rehydrate_host_value({"message": "no handle"}) is None


def _failure_value(holder):
    """`Failure { reason, severity, details: Detail[], by_name: map<string, Detail> }`
    of a program whose classes have no generated Python class."""
    failure = holder.class_value
    failure.name = "user.bridge_tests.Failure"
    reason = failure.fields.add()
    reason.key = "reason"
    reason.value.string_value = "disk full"
    severity = failure.fields.add()
    severity.key = "severity"
    severity.value.enum_value.name = "user.bridge_tests.Severity"
    severity.value.enum_value.value = "High"
    details = failure.fields.add()
    details.key = "details"
    detail = details.value.list_value.items.add().class_value
    detail.name = "user.bridge_tests.Detail"
    code = detail.fields.add()
    code.key = "code"
    code.value.int_value = 7
    by_name = failure.fields.add()
    by_name.key = "by_name"
    entry = by_name.value.map_value.entries.add()
    entry.key = "first"
    entry.value.class_value.name = "user.bridge_tests.Detail"
    code = entry.value.class_value.fields.add()
    code.key = "code"
    code.value.int_value = 8


FAILURE_FIELDS = {
    "reason": "disk full",
    "severity": "High",
    "details": [{"code": 7}],
    "by_name": {"first": {"code": 8}},
}


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_decode_value_unknown_class_as_fields_reaches_every_nested_class():
    holder = baml_outbound_pb2.BamlOutboundValue()
    _failure_value(holder)

    assert decode_value(holder, BamlTypeMap(), unknown_class_as_fields=True) == FAILURE_FIELDS
    with pytest.raises(BamlError, match="Unknown class FQN 'user.bridge_tests.Detail'"):
        decode_value(holder, BamlTypeMap())


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_decode_value_unknown_class_as_fields_keeps_a_generated_class():
    holder = baml_outbound_pb2.BamlOutboundValue()
    wrapper = holder.class_value
    wrapper.name = "user.bridge_tests.NotGenerated"
    resume = wrapper.fields.add()
    resume.key = "resume"
    resume.value.class_value.name = "user.lorem.Resume"
    name = resume.value.class_value.fields.add()
    name.key = "name"
    name.value.string_value = "Alice"
    tm = BamlTypeMap.from_lazy_entries(
        classes={"user.lorem.Resume": (_Resume.__module__, _Resume.__qualname__)},
        enums={},
        type_aliases={},
    )

    decoded = decode_value(holder, tm, unknown_class_as_fields=True)

    assert decoded == {"resume": _Resume(name="Alice")}


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_thrown_value_of_unknown_class_reaches_the_caller_as_its_fields():
    """The error arm: the caller gets the error that the function threw, with
    the name of its BAML class, and not an error of the decode."""
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.error.value)

    with pytest.raises(BamlError, match="disk full") as exc_info:
        decode_call_result(envelope.SerializeToString())

    assert exc_info.value.class_name == "user.bridge_tests.Failure"
    assert exc_info.value.value == FAILURE_FIELDS


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_panic_value_of_unknown_class_reaches_the_caller_as_its_fields():
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.panic.value)

    with pytest.raises(BamlPanic, match="disk full") as exc_info:
        decode_call_result(envelope.SerializeToString())

    assert exc_info.value.class_name == "user.bridge_tests.Failure"
    assert exc_info.value.value == FAILURE_FIELDS


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_returned_value_of_unknown_class_is_an_error():
    """A return value has a declared type that a typed caller relies on, so a
    class that codegen did not emit stays an error there."""
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.ok)

    with pytest.raises(BamlError, match="Unknown class FQN"):
        decode_call_result(envelope.SerializeToString())


# SDK_PARITY_LINT(skip): unit test of the Python bridge's wire decoder
def test_type_mismatch_without_generated_class_is_a_type_error(no_generated_classes):
    envelope = baml_outbound_pb2.BamlOutboundResult()
    mismatch = envelope.error.value.class_value
    mismatch.name = "baml.errors.TypeMismatch"
    message = mismatch.fields.add()
    message.key = "message"
    message.value.string_value = "Value of type 'string' does not match the declared type `int`"

    with pytest.raises(TypeError, match="does not match the declared type `int`"):
        decode_call_result(envelope.SerializeToString())
