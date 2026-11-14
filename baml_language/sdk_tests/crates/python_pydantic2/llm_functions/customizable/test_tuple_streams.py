"""Tuples as stream items, offline: `baml_sdk.tuple_streams` scripts each
function's client to replay fixed text deltas, so the generated stream
companions run the ordinary partial/final parse with no replay server.

  * `stream_tuple_items` -- `(string, int)[]`: partials are lists of native
    tuples, `[("a", 1)]` first; the final value is `[("a", 1), ("b", 2)]`.
  * `stream_tuple_final` -- `(int, string)`: every partial is itself a
    native tuple, and so is the final value `(1, "a")`.
"""
import asyncio

from baml_bridge.cffi.v1 import baml_type_pb2
from baml_bridge.proto import BamlTuple

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.ai.stream import Done
from baml_sdk.tuple_streams import (
    stream_tuple_final_sync,
    stream_tuple_final,
    stream_tuple_final_stream_sync,
    stream_tuple_final_stream,
    stream_tuple_items_sync,
    stream_tuple_items_stream_sync,
    stream_tuple_items_stream,
)


_INT = baml_type_pb2.BAML_TY_PRIMITIVE_INT
_STRING = baml_type_pb2.BAML_TY_PRIMITIVE_STRING


def _assert_carries(value, slot_kinds):
    """A native tuple carrying exactly the tuple type `slot_kinds` spells."""
    assert isinstance(value, tuple)
    assert isinstance(value, BamlTuple)
    assert [slot.primitive.kind for slot in value._ty.tuple.items] == slot_kinds


def _drain(stream):
    partials = []
    while True:
        v = stream.next_sync()
        if isinstance(v, Done):
            return partials
        partials.append(v)
        assert len(partials) < 10_000, "stream.next() failed to terminate"


async def _drain_async(stream):
    partials = []
    while True:
        v = await stream.next()
        if isinstance(v, Done):
            return partials
        partials.append(v)
        assert len(partials) < 10_000, "stream.next_async() failed to terminate"


def _assert_items_partials(partials):
    assert partials, "expected at least one partial"
    assert partials[0] == [("a", 1)]
    for partial in partials:
        assert isinstance(partial, list)
        for item in partial:
            _assert_carries(item, [_STRING, _INT])
            assert isinstance(item[0], str) and isinstance(item[1], int)
        assert partial == [("a", 1), ("b", 2)][: len(partial)]


# SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
def test_tuples_stream_items():
    stream = stream_tuple_items_stream_sync("x")
    _assert_items_partials(_drain(stream))
    final_sync = stream.final_sync()
    assert final_sync == [("a", 1), ("b", 2)]
    for item in final_sync:
        _assert_carries(item, [_STRING, _INT])
    assert stream_tuple_items_sync("x") == final_sync

    async def run_async():
        stream = await stream_tuple_items_stream("x")
        _assert_items_partials(await _drain_async(stream))
        return await stream.final()

    assert asyncio.run(run_async()) == [("a", 1), ("b", 2)]


def _assert_final_partials(partials):
    assert partials, "expected at least one partial"
    for partial in partials:
        _assert_carries(partial, [_INT, _STRING])
        assert partial[0] == 1
        assert isinstance(partial[1], str) and "a".startswith(partial[1])
    assert partials[-1] == (1, "a")


# SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
def test_tuples_stream_final():
    stream = stream_tuple_final_stream_sync("x")
    _assert_final_partials(_drain(stream))
    final_sync = stream.final_sync()
    assert final_sync == (1, "a")
    _assert_carries(final_sync, [_INT, _STRING])
    assert stream_tuple_final_sync("x") == final_sync

    async def run_async():
        stream = await stream_tuple_final_stream("x")
        _assert_final_partials(await _drain_async(stream))
        return await stream.final()

    assert asyncio.run(run_async()) == (1, "a")
    assert asyncio.run(stream_tuple_final("x")) == (1, "a")
