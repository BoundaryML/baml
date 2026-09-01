"""Decoder-side tests for BAML handle round-tripping.

These tests exercise `_decode_handle` with handle kinds that have no
Python-facing constructor (FunctionRef, ADT_MEDIA_GENERIC). The
HANDLE_TABLE entries are seeded via the `_seed_*_handle` PyO3 helpers,
which return `(key, handle_type)` so tests can stage a wire
`BamlHandle` and exercise the decoder dispatch.
"""

from __future__ import annotations

import asyncio
import copy
import typing

import pytest

import baml_bridge
import baml_bridge.baml_py
import baml_bridge.proto

from baml_bridge import BamlFunctionSpec, BamlPyHandle, BamlRuntimeValue, BamlStream
from baml_bridge.baml_py import (
    _live_handle_count,
    _release_wire_handle,
    _seed_function_ref_handle,
    _seed_generic_media_handle,
)
from baml_bridge.cffi.v1 import (
    baml_handle_pb2,
    baml_inbound_pb2,
    baml_outbound_pb2,
)
from baml_bridge.proto import _decode_handle
from baml_bridge.typemap import BamlTypeMap


def _make_handle(key: int, handle_type: int) -> "baml_handle_pb2.BamlHandle":
    h = baml_handle_pb2.BamlHandle()
    h.key = key
    # `BamlHandleType` is an `int` subclass at runtime; the proto field
    # accepts bare ints. Cast for the static checker.
    h.handle_type = typing.cast(baml_handle_pb2.BamlHandleType, handle_type)
    return h


def test_function_ref_decodes_to_callable():
    key, ht = _seed_function_ref_handle(123)
    result = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert callable(result)


def test_adt_media_generic_decodes_to_pyhandle():
    key, ht = _seed_generic_media_handle()
    result = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert isinstance(result, BamlPyHandle)


def test_decoded_pyhandle_releases_on_drop():
    """Dropping a `BamlPyHandle` removes its row from `HANDLE_TABLE` —
    a subsequent wrapper can still be created from the wire payload, but
    cloning it fails because the entry is gone.
    """
    key, ht = _seed_function_ref_handle(7)
    closure = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert callable(closure)
    del closure  # CPython refcount drops to 0; Drop runs HANDLE_TABLE.release.
    stale = _decode_handle(_make_handle(key, ht), BamlTypeMap())
    assert callable(stale)
    with pytest.raises(RuntimeError, match="invalid handle"):
        copy.copy(stale._handle)


def test_encode_failure_releases_every_cloned_capability_handle():
    """A later nested encode error explicitly releases every HANDLE_TABLE
    clone while leaving each original Python-owned capability live."""
    handles = []
    capabilities = []
    wrappers = (
        lambda handle: handle,
        BamlFunctionSpec._from_pyhandle,
        BamlStream._from_pyhandle,
        BamlRuntimeValue._from_pyhandle,
    )
    for index, wrap in enumerate(wrappers):
        key, handle_type = _seed_function_ref_handle(index)
        handle = BamlPyHandle(key, handle_type)
        handles.append(handle)
        capabilities.append(wrap(handle))

    before = _live_handle_count()
    with pytest.raises(TypeError, match="Cannot encode argument 'values'"):
        baml_bridge.proto.encode_call_args(
            {"values": [*capabilities, object()]},
            call_id=1,
        )

    assert _live_handle_count() == before
    for handle in handles:
        clone_key, _ = handle._clone_key_for_wire()
        _release_wire_handle(clone_key)


def test_successful_encode_retains_clone_until_wire_owner_consumes_it():
    key, handle_type = _seed_function_ref_handle(17)
    original = BamlPyHandle(key, handle_type)
    spec = BamlFunctionSpec._from_pyhandle(original)
    before = _live_handle_count()

    encoded = baml_bridge.proto.encode_call_args({"spec": spec}, call_id=2)
    args = baml_inbound_pb2.CallFunctionArgs.FromString(encoded)
    wire_key = args.kwargs[0].value.handle.key

    assert wire_key != key
    assert _live_handle_count() == before + 1
    _release_wire_handle(wire_key)
    assert _live_handle_count() == before


def test_stream_handle_kind_ignores_misleading_type_metadata():
    key, _ = _seed_function_ref_handle(9)
    handle = baml_outbound_pb2.BamlOutboundHandle()
    handle.key = key
    handle.handle_type = baml_handle_pb2.ADT_TAGGED_HEAP_HANDLE
    handle.ty.class_ty.name = "test.stream.Custom"
    assert isinstance(_decode_handle(handle, BamlTypeMap()), BamlStream)


@pytest.mark.parametrize(
    ("handle_type", "expected_type"),
    [
        (baml_handle_pb2.ADT_FUNCTION_SPEC, BamlFunctionSpec),
        (baml_handle_pb2.ADT_RUNTIME_VALUE, BamlRuntimeValue),
    ],
)
def test_live_handle_kinds_select_trusted_wrappers(handle_type, expected_type):
    key, _ = _seed_function_ref_handle(10)
    handle = baml_outbound_pb2.BamlOutboundHandle(
        key=key,
        handle_type=handle_type,
    )
    handle.ty.class_ty.name = "user.CollidingCompiledName"
    assert isinstance(_decode_handle(handle, BamlTypeMap()), expected_type)


@pytest.mark.asyncio
async def test_function_spec_uses_canonical_method_fqns_and_wire_argument_names(
    monkeypatch,
):
    spec = BamlFunctionSpec._from_pyhandle(typing.cast(BamlPyHandle, object()))
    sync_calls = []
    async_calls = []

    def capture_sync(_self, fqn, kwargs=None):
        sync_calls.append((fqn, kwargs))
        return fqn

    async def capture_async(_self, fqn, kwargs=None):
        async_calls.append((fqn, kwargs))
        return fqn

    monkeypatch.setattr(BamlFunctionSpec, "_call_sync", capture_sync)
    monkeypatch.setattr(BamlFunctionSpec, "_call_async", capture_async)

    assert spec.output_type() == "ai.FunctionSpec.output_type"
    assert spec.client_id() == "ai.FunctionSpec.client_id"
    assert spec.parse("{}") == "ai.FunctionSpec.parse"
    assert await spec.output_type_async() == "ai.FunctionSpec.output_type"
    assert await spec.client_id_async() == "ai.FunctionSpec.client_id"
    assert await spec.parse_async("[]") == "ai.FunctionSpec.parse"

    assert sync_calls == [
        ("ai.FunctionSpec.output_type", None),
        ("ai.FunctionSpec.client_id", None),
        ("ai.FunctionSpec.parse", {"json": "{}"}),
    ]
    assert async_calls == [
        ("ai.FunctionSpec.output_type", None),
        ("ai.FunctionSpec.client_id", None),
        ("ai.FunctionSpec.parse", {"json": "[]"}),
    ]


@pytest.mark.asyncio
async def test_stream_uses_canonical_method_fqns(monkeypatch):
    stream = BamlStream._from_pyhandle(typing.cast(BamlPyHandle, object()))
    monkeypatch.setattr(BamlStream, "_call_sync", lambda _self, fqn: fqn)

    async def capture_async(_self, fqn):
        return fqn

    monkeypatch.setattr(BamlStream, "_call_async", capture_async)

    assert stream.next() == "ai.stream.Stream.next"
    assert stream.final() == "ai.stream.Stream.final"
    assert await stream.next_async() == "ai.stream.Stream.next"
    assert await stream.final_async() == "ai.stream.Stream.final"


@pytest.mark.asyncio
async def test_stream_async_forwards_python_task_cancellation(monkeypatch):
    entered = asyncio.Event()
    encoded_call_ids: list[int] = []
    call_ids: list[int] = []

    class BlockingRuntime:
        async def call_function(self, _args, _ctx, _collectors):
            entered.set()
            await asyncio.Future()

    monkeypatch.setattr(baml_bridge, "get_runtime", lambda: BlockingRuntime())
    monkeypatch.setattr(
        baml_bridge,
        "cancel_function_call",
        lambda call_id: call_ids.append(call_id) or True,
    )
    monkeypatch.setattr(baml_bridge.baml_py, "new_function_call", lambda: 17)

    def encode_call_args(_args, call_id, **_kwargs):
        encoded_call_ids.append(call_id)
        return b"encoded"

    monkeypatch.setattr(baml_bridge.proto, "encode_call_args", encode_call_args)
    stream = BamlStream._from_pyhandle(typing.cast(BamlPyHandle, object()))

    task = asyncio.create_task(stream.next_async())
    await asyncio.wait_for(entered.wait(), timeout=1.0)
    assert task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert encoded_call_ids == call_ids == [17]


@pytest.mark.asyncio
async def test_stream_async_preserves_cancellation_when_native_cancel_fails(
    monkeypatch,
):
    entered = asyncio.Event()
    call_ids: list[int] = []

    class BlockingRuntime:
        async def call_function(self, _args, _ctx, _collectors):
            entered.set()
            await asyncio.Future()

    def fail_cancel(call_id: int) -> None:
        call_ids.append(call_id)
        raise RuntimeError("native cancellation failed")

    monkeypatch.setattr(baml_bridge, "get_runtime", lambda: BlockingRuntime())
    monkeypatch.setattr(baml_bridge, "cancel_function_call", fail_cancel)
    monkeypatch.setattr(baml_bridge.baml_py, "new_function_call", lambda: 23)
    monkeypatch.setattr(
        baml_bridge.proto,
        "encode_call_args",
        lambda _args, _call_id, **_kwargs: b"encoded",
    )
    stream = BamlStream._from_pyhandle(typing.cast(BamlPyHandle, object()))

    task = asyncio.create_task(stream.next_async())
    await asyncio.wait_for(entered.wait(), timeout=1.0)
    assert task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert task.cancelled()
    assert call_ids == [23]


@pytest.mark.asyncio
async def test_live_capability_methods_use_async_cancellation_decoder(monkeypatch):
    class CompletedRuntime:
        async def call_function(self, _args, _ctx, _collectors):
            return b"cancelled-result"

    decoded: list[bytes] = []

    def decode_async(result: bytes):
        decoded.append(result)
        raise asyncio.CancelledError("engine cancellation")

    monkeypatch.setattr(baml_bridge, "get_runtime", lambda: CompletedRuntime())
    monkeypatch.setattr(baml_bridge, "_decode_call_result_async", decode_async)
    monkeypatch.setattr(baml_bridge.baml_py, "new_function_call", lambda: 29)
    monkeypatch.setattr(
        baml_bridge.proto,
        "encode_call_args",
        lambda _args, _call_id, **_kwargs: b"encoded",
    )

    handle = typing.cast(BamlPyHandle, object())
    calls = [
        BamlFunctionSpec._from_pyhandle(handle).name_async,
        BamlStream._from_pyhandle(handle).next_async,
        BamlRuntimeValue._from_pyhandle(handle).to_data_async,
    ]
    for call in calls:
        with pytest.raises(asyncio.CancelledError, match="engine cancellation"):
            await call()

    assert decoded == [b"cancelled-result"] * 3
