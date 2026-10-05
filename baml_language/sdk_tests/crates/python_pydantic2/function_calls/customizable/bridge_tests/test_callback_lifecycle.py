"""Ownership of queued and partially decoded host-call arguments."""

from collections import Counter
import gc

import pytest
import pydantic

from baml_bridge import proto

from baml_bridge.baml_py import (
    BamlPyHandle,
    _discard_host_call_args,
    _handle_refcount,
    _invoke_host_callable,
    _release_wire_handle,
    _seed_function_ref_handle,
    _seed_generic_media_handle,
    _seed_heap_handle,
)
from baml_bridge.cffi.v1 import baml_handle_pb2, baml_outbound_pb2
from baml_bridge.typemap import BamlTypeMap


@pytest.mark.parametrize("heap", [False, True])
# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_discarded_args_release_each_nested_wire_owner_once(heap):
    key, kind = _seed_heap_handle(0xCA11BAC) if heap else _seed_generic_media_handle()
    owner = BamlPyHandle(key, kind)
    call = baml_outbound_pb2.BamlToHostCall()
    holders = [
        call.args.add().value.list_value.items.add(),
        call.args.add().value.map_value.entries.add().value,
        call.args.add().value.class_value.fields.add().value,
        call.args.add().value.union_variant_value.value,
    ]
    wire_keys = []
    for holder in holders:
        wire_key, wire_kind = owner._clone_key_for_wire()
        wire_keys.append(wire_key)
        holder.handle_value.key = wire_key
        holder.handle_value.handle_type = wire_kind
    owners = Counter(wire_keys)
    owners[key] += 1
    for live_key, count in owners.items():
        assert _handle_refcount(live_key) == count
    _discard_host_call_args(call.SerializeToString())
    assert _handle_refcount(key) == 1
    for wire_key in wire_keys:
        if wire_key != key:
            assert _handle_refcount(wire_key) is None
    del owner
    assert _handle_refcount(key) is None


# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_discarded_args_do_not_release_borrowed_host_registry_keys():
    kind = baml_handle_pb2.BamlHandleType
    key, _ = _seed_generic_media_handle()
    owner = BamlPyHandle(key, kind.ADT_MEDIA_GENERIC)
    call = baml_outbound_pb2.BamlToHostCall()
    # Numeric keys in the host registry have independent authority. Even when
    # a borrowed host key matches an ordinary key, it must not release that row.
    for host_kind in (kind.HOST_VALUE_CALLABLE, kind.HOST_VALUE_OPAQUE):
        handle = call.args.add().value.handle_value
        handle.key = key
        handle.handle_type = host_kind
    _discard_host_call_args(call.SerializeToString())
    assert _handle_refcount(key) == 1
    del owner
    assert _handle_refcount(key) is None


@pytest.fixture
def decoding_models(monkeypatch):
    captured = []

    class Capture(pydantic.BaseModel):
        model_config = pydantic.ConfigDict(arbitrary_types_allowed=True)
        value: object

        @pydantic.field_validator("value")
        @classmethod
        def capture(cls, value):
            captured.append(value)
            return value

    class Reject(pydantic.BaseModel):
        value: int

        @pydantic.field_validator("value")
        @classmethod
        def reject(cls, value):
            raise ValueError("deliberate argument validation failure")

    type_map = BamlTypeMap()
    monkeypatch.setattr(
        BamlTypeMap,
        "get_class",
        lambda self, name: {
            "Capture": Capture,
            "Reject": Reject,
        }[name],
    )
    monkeypatch.setattr(proto, "get_type_map", lambda: type_map)
    yield captured
    captured.clear()


def reject_argument(holder):
    holder.class_value.name = "Reject"
    field = holder.class_value.fields.add()
    field.key = "value"
    field.value.int_value = 1


def wire_handle(holder, key, kind):
    holder.handle_value.key = key
    holder.handle_value.handle_type = kind


# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_decode_failure_releases_untransferred_callable_and_media(decoding_models):
    owners = [_seed_function_ref_handle(17), _seed_generic_media_handle()]
    call = baml_outbound_pb2.BamlToHostCall()
    reject_argument(call.args.add().value)
    for key, kind in owners:
        wire_handle(call.args.add().value, key, kind)
    called = []
    try:
        with pytest.raises(pydantic.ValidationError, match="deliberate argument"):
            _invoke_host_callable(
                lambda *args: called.append(args), call.SerializeToString()
            )
        assert called == []
        assert [_handle_refcount(key) for key, _ in owners] == [None, None]
    finally:
        for key, _ in owners:
            if _handle_refcount(key) is not None:
                _release_wire_handle(key)


# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_decode_failure_counts_repeated_keys_and_ignores_borrowed_host_keys(
    decoding_models,
):
    key, kind = _seed_heap_handle(0xDEC100)
    owner = BamlPyHandle(key, kind)
    call = baml_outbound_pb2.BamlToHostCall()
    reject_argument(call.args.add().value)
    for _ in range(2):
        cloned, _ = owner._clone_key_for_wire()
        wire_handle(call.args.add().value, cloned, kind)
    for borrowed in (
        baml_handle_pb2.HOST_VALUE_CALLABLE,
        baml_handle_pb2.HOST_VALUE_OPAQUE,
    ):
        wire_handle(call.args.add().value, key, borrowed)
    try:
        with pytest.raises(pydantic.ValidationError, match="deliberate argument"):
            _invoke_host_callable(lambda *args: None, call.SerializeToString())
        assert _handle_refcount(key) == 1  # Only the original owner remains.
    finally:
        while (_handle_refcount(key) or 0) > 1:
            _release_wire_handle(key)
    del owner
    assert _handle_refcount(key) is None


@pytest.mark.parametrize("container", ["arguments", "list", "map", "class", "union"])
# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_decode_failure_preserves_transferred_owner_and_releases_remaining(
    container, decoding_models
):
    # Every occurrence owns a reference, even when all references share a key.
    slab = 0xDEC0DE + ["arguments", "list", "map", "class", "union"].index(container)
    key, kind = _seed_heap_handle(slab)
    owner = BamlPyHandle(key, kind)
    call = baml_outbound_pb2.BamlToHostCall()
    if container == "arguments":
        holders = [call.args.add().value for _ in range(3)]
    else:
        root = call.args.add().value
        if container == "union":
            root = root.union_variant_value.value
        if container in ("list", "union"):
            holders = [root.list_value.items.add() for _ in range(3)]
        elif container == "map":
            holders = []
            for name in ("first", "reject", "last"):
                entry = root.map_value.entries.add()
                entry.key = name
                holders.append(entry.value)
        else:
            root.class_value.name = "Capture"
            holders = []
            for name in ("first", "reject", "last"):
                field = root.class_value.fields.add()
                field.key = name
                holders.append(field.value)
    holders[0].class_value.name = "Capture"
    field = holders[0].class_value.fields.add()
    field.key = "value"
    transferred, _ = owner._clone_key_for_wire()
    wire_handle(field.value, transferred, kind)
    reject_argument(holders[1])
    remaining, _ = owner._clone_key_for_wire()
    wire_handle(holders[2], remaining, kind)
    assert _handle_refcount(key) == 3
    try:
        with pytest.raises(pydantic.ValidationError, match="deliberate argument"):
            _invoke_host_callable(lambda *args: None, call.SerializeToString())
        assert len(decoding_models) == 1
        assert _handle_refcount(key) == 2  # Original owner and captured wrapper.
        decoding_models.clear()
        gc.collect()  # A validation traceback can retain already-decoded values.
        assert _handle_refcount(key) == 1
    finally:
        decoding_models.clear()
        gc.collect()
        while (_handle_refcount(key) or 0) > 1:
            _release_wire_handle(key)
    del owner
    assert _handle_refcount(key) is None


# SDK_PARITY_LINT(skip): unit test of the Python bridge's ownership of wire handles
def test_successful_decode_transfers_ownership_to_callback():
    key, kind = _seed_generic_media_handle()
    call = baml_outbound_pb2.BamlToHostCall()
    wire_handle(call.args.add().value, key, kind)
    captured = []
    _invoke_host_callable(
        lambda value: captured.append(value), call.SerializeToString()
    )
    assert _handle_refcount(key) == 1
    captured.clear()
    assert _handle_refcount(key) is None
