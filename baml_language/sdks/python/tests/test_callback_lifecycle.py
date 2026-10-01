"""Ownership of host-call arguments discarded before callback execution."""

from collections import Counter

import pytest

from baml_bridge.baml_py import (
    BamlPyHandle,
    _discard_host_call_args,
    _handle_refcount,
    _seed_generic_media_handle,
    _seed_heap_handle,
)
from baml_bridge.cffi.v1 import baml_handle_pb2, baml_outbound_pb2


@pytest.mark.parametrize("heap", [False, True])
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
