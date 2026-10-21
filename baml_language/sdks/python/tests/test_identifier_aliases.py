"""Wire-identity tests for Python-safe generated identifier aliases."""

from __future__ import annotations

import asyncio
import enum

import pytest

import pydantic

import baml_bridge
from baml_bridge.baml_py import _encode_inbound
from baml_bridge.cffi.v1 import baml_inbound_pb2
from baml_bridge.proto import _EncodedCall
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


class KeywordChoice(str, enum.Enum):
    None_ = "None"


class KeywordModel(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="ignore", populate_by_name=True)

    None_: int = pydantic.Field(alias="None")


def _identifier_typemap() -> BamlTypeMap:
    return BamlTypeMap.from_lazy_entries(
        classes={
            "user.KeywordModel": (KeywordModel.__module__, KeywordModel.__name__),
        },
        enums={
            "user.KeywordChoice": (KeywordChoice.__module__, KeywordChoice.__name__),
        },
        type_aliases={},
    )


def test_renamed_enum_member_encodes_raw_value_for_scalar_and_map_key():
    saved = get_type_map()
    set_type_map(_identifier_typemap())
    try:
        scalar = baml_inbound_pb2.InboundValue.FromString(
            _encode_inbound(KeywordChoice.None_, "choice")
        )
        assert scalar.enum_value.name == "user.KeywordChoice"
        assert scalar.enum_value.value == "None"

        # Map keys cross the boundary as strings only; an enum member is not
        # one, whatever its value, so a dict keyed by one is refused.
        with pytest.raises(TypeError, match="map keys must be string"):
            _encode_inbound({KeywordChoice.None_: 1}, "choices")
    finally:
        set_type_map(saved)


def test_renamed_pydantic_field_populates_both_ways_and_encodes_raw_name():
    by_host = KeywordModel(None_=1)
    by_wire = KeywordModel.model_validate({"None": 2})
    assert by_host.None_ == 1
    assert by_wire.None_ == 2

    saved = get_type_map()
    set_type_map(_identifier_typemap())
    try:
        inbound = baml_inbound_pb2.InboundValue.FromString(_encode_inbound(by_host, "model"))
        assert inbound.value_type.class_ty.name == "user.KeywordModel"
        assert inbound.class_value.fields[0].string_key == "None"
        assert inbound.class_value.fields[0].value.int_value == 1
    finally:
        set_type_map(saved)


def test_callable_parameter_aliases_map_positional_and_keyword_calls_to_wire_names():
    aliases = {"class_": "class", "_types_": "_types"}

    assert baml_bridge._build_kwargs(
        (1,), {"_types_": 2}, ["class_"], ["_types_"], aliases
    ) == {"class": 1, "_types": 2}
    assert baml_bridge._build_kwargs((), {"class_": 1}, ["class_"], [], aliases) == {
        "class": 1
    }


def test_define_function_preserves_generated_callable_metadata_and_generic_wrapping():
    direct = baml_bridge.define_function(
        "user.GenerateNote",
        "sync",
        [],
        binding_name="GenerateNote",
        binding_qualname="GenerateNote",
        binding_module="baml_sdk",
    )
    assert direct.__name__ == "GenerateNote"
    assert direct.__qualname__ == "GenerateNote"
    assert direct.__module__ == "baml_sdk"

    generic_method = baml_bridge.define_function(
        "user.Box.map",
        "async",
        ["self", "value"],
        type_params=["T"],
        binding_name="map_async",
        binding_qualname="Box.map_async",
        binding_module="baml_sdk.models",
    )
    assert generic_method.__name__ == "map_async"
    assert generic_method.__qualname__ == "Box.map_async"
    assert generic_method.__module__ == "baml_sdk.models"
    assert generic_method.__wrapped__.__name__ == "map_async"


def test_stream_companion_calls_its_exact_fqn(monkeypatch):
    encoded_calls = []

    class FakeRuntime:
        def call_function_sync(self, args, kwargs=None, cancel=None, *, stream=False):
            assert args == b"encoded"
            return b"stream"

        async def call_function(self, args, kwargs=None, cancel=None, *, stream=False):
            assert args == b"encoded"
            return b"stream"

    def fake_encode(kwargs, call_id, type_args, *, function_name, _baml):
        encoded_calls.append((kwargs, call_id, type_args, function_name, _baml))
        return _EncodedCall(b"encoded", None, kwargs, None)

    monkeypatch.setattr(baml_bridge, "new_function_call", lambda: 42)
    monkeypatch.setattr(baml_bridge, "get_runtime", lambda: FakeRuntime())
    monkeypatch.setattr(baml_bridge, "encode_call_args", fake_encode)
    monkeypatch.setattr(baml_bridge, "decode_call_result", lambda _value: "stream")
    monkeypatch.setattr(
        baml_bridge, "_decode_call_result_async", lambda _value: "stream"
    )
    # A stream companion's result is decoded into a `BamlStream`; this test
    # pins the call's spelling, not the stream.
    monkeypatch.setattr(
        baml_bridge._stream,
        "_decode_stream_result",
        lambda result, *, asynchronous=False: "stream",
    )

    listener = object()
    sync_stream = baml_bridge.define_function(
        "user.plan@stream",
        "sync",
        ["prompt"],
        ["on_event"],
    )
    async_stream = baml_bridge.define_function(
        "user.plan@stream",
        "async",
        ["prompt"],
        ["on_event"],
    )

    options = object()
    assert sync_stream("hello", on_event=listener) == "stream"
    assert asyncio.run(async_stream("world", on_event=listener, _baml=options)) == "stream"
    assert encoded_calls == [
        ({"prompt": "hello", "on_event": listener}, 42, None, "user.plan@stream", None),
        ({"prompt": "world", "on_event": listener}, 42, None, "user.plan@stream", options),
    ]


def test_stream_exact_fqn_is_preserved_on_the_wire():
    encoded = baml_bridge.encode_call_args({}, 1, function_name="user.plan@stream")
    decoded = baml_inbound_pb2.CallFunctionArgs.FromString(encoded)
    assert decoded.function_name == "user.plan@stream"
