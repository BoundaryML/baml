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

import baml_bridge
import baml_bridge.baml_py
import baml_bridge.proto
import pytest

from baml_bridge import BamlPyHandle, BamlStream
from baml_bridge.baml_py import (
    _seed_function_ref_handle,
    _seed_generic_media_handle,
)
from baml_bridge.proto import _decode_handle
from baml_bridge.typemap import BamlTypeMap
from baml_bridge.cffi.v1 import baml_handle_pb2, baml_outbound_pb2


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


def test_tagged_handle_passes_class_fqn_to_wrapper():
    class TaggedWrapper:
        @classmethod
        def _from_pyhandle(cls, pyhandle, class_fqn):
            return pyhandle, class_fqn

    key, _ = _seed_function_ref_handle(9)
    handle = baml_outbound_pb2.BamlOutboundHandle()
    handle.key = key
    handle.handle_type = baml_handle_pb2.ADT_TAGGED_HEAP_HANDLE
    handle.ty.class_ty.name = "test.stream.Custom"
    tm = BamlTypeMap()
    tm._class_cache["test.stream.Custom"] = TaggedWrapper

    pyhandle, class_fqn = _decode_handle(handle, tm)
    assert isinstance(pyhandle, BamlPyHandle)
    assert class_fqn == "test.stream.Custom"


@pytest.mark.asyncio
async def test_stream_derives_method_fqns_from_tagged_class(monkeypatch):
    stream = BamlStream._from_pyhandle(
        typing.cast(BamlPyHandle, object()), "test.stream.Custom"
    )
    monkeypatch.setattr(BamlStream, "_call_sync", lambda _self, fqn: fqn)

    async def capture_async(_self, fqn):
        return fqn

    monkeypatch.setattr(BamlStream, "_call_async", capture_async)

    assert stream.next() == "test.stream.Custom.next"
    assert stream.final() == "test.stream.Custom.final"
    assert await stream.next_async() == "test.stream.Custom.next"
    assert await stream.final_async() == "test.stream.Custom.final"


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
    stream = BamlStream._from_pyhandle(
        typing.cast(BamlPyHandle, object()),
        "test.stream.Custom",
    )

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
    stream = BamlStream._from_pyhandle(
        typing.cast(BamlPyHandle, object()),
        "test.stream.Custom",
    )

    task = asyncio.create_task(stream.next_async())
    await asyncio.wait_for(entered.wait(), timeout=1.0)
    assert task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert task.cancelled()
    assert call_ids == [23]
