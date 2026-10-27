"""Outbound class/enum decode goes through the typemap (25a2 §4.1).

These directly assert the new code path. They build a `BamlTypeMap`
from lazy entries pointing at a test-local Pydantic model, hand-build
a `class_value` proto, and call `decode_value(holder, type_map)`. No
runtime, no sdk_root.
"""

from __future__ import annotations

import typing

import pydantic
import pytest

from baml_bridge import BamlClassInstance, BamlError, BamlPanic, BamlPyHandle
from baml_bridge.baml_py import _seed_generic_media_handle
from baml_bridge.typemap import BamlTypeMap
from baml_bridge.proto import (
    _try_rehydrate_host_value,
    _ty_to_python_type,
    decode_call_result,
    decode_value,
)
from baml_bridge.cffi.v1 import baml_outbound_pb2, baml_type_pb2


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


# A thrown value has no declared Python type, and its class may have no Python
# class: a process with no generated SDK has none, and a program can make a
# class at run time. `BamlTypeMap.where_generated` is the lookup such a value
# decodes with.


def test_rehydrate_host_value_reads_the_handle_of_a_class_instance(monkeypatch):
    handle = object()
    original = ValueError("original")
    monkeypatch.setattr(
        "baml_bridge.baml_py.lookup_host_value",
        lambda candidate: original if candidate is handle else None,
    )

    thrown = BamlClassInstance(
        "baml.errors.HostCallable", {"message": "ValueError: original", "_handle": handle}
    )
    assert _try_rehydrate_host_value(thrown) is original
    without_handle = BamlClassInstance("baml.errors.HostCallable", {"message": "no handle"})
    assert _try_rehydrate_host_value(without_handle) is None


def _failure_value(holder):
    """`Failure { reason, severity, details: Detail[], by_name: map<string, Detail> }`
    of a program whose classes have no Python class."""
    failure = holder.class_value
    failure.name = "user.Failure"
    reason = failure.fields.add()
    reason.key = "reason"
    reason.value.string_value = "disk full"
    severity = failure.fields.add()
    severity.key = "severity"
    severity.value.enum_value.name = "user.Severity"
    severity.value.enum_value.value = "High"
    details = failure.fields.add()
    details.key = "details"
    detail = details.value.list_value.items.add().class_value
    detail.name = "user.Detail"
    code = detail.fields.add()
    code.key = "code"
    code.value.int_value = 7
    by_name = failure.fields.add()
    by_name.key = "by_name"
    entry = by_name.value.map_value.entries.add()
    entry.key = "first"
    entry.value.class_value.name = "user.Detail"
    code = entry.value.class_value.fields.add()
    code.key = "code"
    code.value.int_value = 8


FAILURE = BamlClassInstance(
    "user.Failure",
    {
        "reason": "disk full",
        "severity": "High",
        "details": [BamlClassInstance("user.Detail", {"code": 7})],
        "by_name": {"first": BamlClassInstance("user.Detail", {"code": 8})},
    },
)


def test_class_with_no_python_class_keeps_its_name_at_every_depth():
    holder = baml_outbound_pb2.BamlOutboundValue()
    _failure_value(holder)

    assert decode_value(holder, BamlTypeMap().where_generated()) == FAILURE
    # A map and a class instance with the same entries stay different values.
    assert FAILURE.fields["by_name"]["first"] != {"code": 8}
    with pytest.raises(BamlError, match="Unknown class FQN 'user.Detail'"):
        decode_value(holder, BamlTypeMap())


def test_class_with_a_python_class_decodes_to_it_inside_one_with_none():
    holder = baml_outbound_pb2.BamlOutboundValue()
    wrapper = holder.class_value
    wrapper.name = "user.NotGenerated"
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

    decoded = decode_value(holder, tm.where_generated())

    assert decoded == BamlClassInstance("user.NotGenerated", {"resume": _Resume(name="Alice")})


class _Wrapped(pydantic.BaseModel):
    item: object


def test_type_argument_with_no_python_type_is_a_wildcard():
    # A generated generic class whose type argument is a class or an enum that
    # the program made at run time.
    for unknown in ("class_ty", "enum", "type_alias"):
        ty = baml_type_pb2.BamlTy()
        getattr(ty, unknown).name = "user.MadeAtRunTime"

        assert _ty_to_python_type(ty, BamlTypeMap().where_generated()) is typing.Any
        with pytest.raises(BamlError, match="user.MadeAtRunTime"):
            _ty_to_python_type(ty, BamlTypeMap())


def test_prompt_with_no_python_class_is_a_class_instance():
    holder = baml_outbound_pb2.BamlOutboundValue()
    holder.prompt_ast_value.SetInParent()

    decoded = decode_value(holder, BamlTypeMap().where_generated())

    assert isinstance(decoded, BamlClassInstance)
    assert decoded.class_name == "ai.Prompt"
    assert list(decoded.fields) == ["_data"]
    with pytest.raises(BamlError, match="Unknown class FQN 'ai.Prompt'"):
        decode_value(holder, BamlTypeMap())


def test_thrown_value_of_a_class_with_no_python_class_reaches_the_caller():
    """The caller gets the error that the function threw, with the name of its
    BAML class, and not an error of the decode."""
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.error.value)

    with pytest.raises(BamlError, match="disk full") as exc_info:
        decode_call_result(envelope.SerializeToString())

    assert exc_info.value.class_name == "user.Failure"
    assert exc_info.value.value == FAILURE


def test_panic_value_of_a_class_with_no_python_class_reaches_the_caller():
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.panic.value)

    with pytest.raises(BamlPanic, match="disk full") as exc_info:
        decode_call_result(envelope.SerializeToString())

    assert exc_info.value.class_name == "user.Failure"
    assert exc_info.value.value == FAILURE


def test_returned_value_of_a_class_with_no_python_class_is_an_error():
    """A return value has a declared type that a typed caller relies on, so a
    class that codegen did not emit stays an error there."""
    envelope = baml_outbound_pb2.BamlOutboundResult()
    _failure_value(envelope.ok)

    with pytest.raises(BamlError, match="Unknown class FQN"):
        decode_call_result(envelope.SerializeToString())


def test_type_mismatch_with_no_python_class_is_a_type_error():
    envelope = baml_outbound_pb2.BamlOutboundResult()
    mismatch = envelope.error.value.class_value
    mismatch.name = "baml.errors.TypeMismatch"
    message = mismatch.fields.add()
    message.key = "message"
    message.value.string_value = "Value of type 'string' does not match the declared type `int`"

    with pytest.raises(TypeError, match="does not match the declared type `int`"):
        decode_call_result(envelope.SerializeToString())


def test_class_instance_reads_as_its_class():
    instance = BamlClassInstance("baml.errors.InvalidArgument", {"message": "no such function"})

    assert repr(instance) == "baml.errors.InvalidArgument(message='no such function')"
