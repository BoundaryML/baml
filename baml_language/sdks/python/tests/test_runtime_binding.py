"""SDK calls retain their selected runtime and codec context, not a later default."""

from __future__ import annotations

import asyncio
import gc
import threading
import weakref

import pytest
from pydantic import BaseModel

from baml_bridge import define_function, proto
from baml_bridge.baml_py import (
    BamlRuntime,
    _pending_transfer_count,
    new_function_call,
    shutdown_runtime,
)
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


SOURCE = """
class Record { value: string }
function echo<T>(value: T) -> T throws never { value }
function map_record(value: Record, callback: (Record) -> Record) -> Record { callback(value) }
function catch_record(callback: () -> never throws Record) -> Record {
    callback() catch (error) { let record: Record => record }
}
function delayed(value: Record, wait: () -> int throws never) -> Record { wait(); value }
function effect(callback: () -> int throws never) -> string { callback(); "ran" }
function retain(callback: () -> int throws never) -> () -> int throws never { () -> { callback() } }
"""


class Record(BaseModel):
    value: str


class Callback:
    def __init__(self, events: list[str]):
        self.events = events

    def __call__(self) -> int:
        self.events.append("called")
        return 1


@pytest.fixture
def binding():
    saved = get_type_map()
    mapping = BamlTypeMap.from_lazy_entries(
        {"user.Record": (__name__, "Record")}, {}, {}
    )
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    yield runtime, mapping
    shutdown_runtime()
    set_type_map(saved)
    gc.collect()


@pytest.mark.parametrize("mode", ["sync", "async"])
def test_replaced_binding_rejects_before_callback_and_releases_arguments(binding, mode):
    runtime, mapping = binding
    call = define_function(
        "user.effect", mode, ["callback"], runtime=runtime, type_map=mapping
    )
    replacement = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    events: list[str] = []
    callback = Callback(events)
    weak = weakref.ref(callback)
    with pytest.raises(BamlError, match="closed or replaced"):
        if mode == "sync":
            call(callback)
        else:
            asyncio.run(call(callback))
    assert events == []
    del callback
    gc.collect()
    assert weak() is None
    assert _pending_transfer_count() == 0
    current_call = define_function(
        "user.effect", "sync", ["callback"], runtime=replacement, type_map=mapping
    )
    assert current_call(Callback(events)) == "ran"
    assert events == ["called"]


def test_bound_codecs_ignore_and_restore_the_process_default(binding):
    runtime, mapping = binding
    call = define_function(
        "user.echo",
        "sync",
        ["value"],
        type_params=["T"],
        runtime=runtime,
        type_map=mapping,
    )
    unrelated = BamlTypeMap()
    set_type_map(unrelated)
    value = call(Record(value="Ada"), _types={"T": Record})
    assert type(value) is Record
    assert value.value == "Ada"
    assert get_type_map() is unrelated
    with pytest.raises(TypeError):
        call(object(), _types={"T": Record})
    assert get_type_map() is unrelated


@pytest.mark.asyncio
async def test_async_result_uses_the_captured_sdk_map(binding):
    runtime, mapping = binding
    # Current callback dispatch uses a separate host loop/thread. A threading
    # barrier isolates outer-call codec binding from that executor limitation.
    started, release = threading.Event(), threading.Event()

    def wait():
        started.set()
        assert release.wait(timeout=5)
        return 1

    call = define_function(
        "user.delayed", "async", ["value", "wait"], runtime=runtime, type_map=mapping
    )
    task = asyncio.create_task(call(Record(value="Ada"), wait))
    assert await asyncio.to_thread(started.wait, 5)
    unrelated = BamlTypeMap()
    set_type_map(unrelated)
    release.set()
    result = await asyncio.wait_for(task, timeout=5)
    assert type(result) is Record
    assert result.value == "Ada"
    assert get_type_map() is unrelated


def test_failed_scheduling_drops_prepared_callback(binding):
    runtime, _ = binding
    callback = Callback([])
    weak = weakref.ref(callback)
    encoded = proto.encode_call_args(
        {"callback": callback}, new_function_call(), function_name="user.effect"
    )
    with pytest.raises(RuntimeError, match="no running event loop"):
        runtime.call_function(encoded)
    del callback
    gc.collect()
    assert weak() is None


@pytest.mark.asyncio
async def test_sdk_binding_does_not_keep_a_closed_engine_heap_alive(binding):
    runtime, mapping = binding

    class Value:
        def __call__(self) -> int:
            return 42

    call = define_function(
        "user.retain", "sync", ["callback"], runtime=runtime, type_map=mapping
    )
    callback = Value()
    weak = weakref.ref(callback)
    retained = call(callback)
    del retained, callback
    gc.collect()
    # Only BAML's uncollected closure/capture retains the host callback now.
    assert weak() is not None
    shutdown_runtime()
    for _ in range(100):
        gc.collect()
        if weak() is None:
            break
        await asyncio.sleep(0.01)
    # Both the SDK binding and its generated-call-shaped closure remain live.
    assert runtime is not None and call is not None
    assert weak() is None


@pytest.mark.asyncio
@pytest.mark.parametrize("throws", [False, True])
async def test_callback_uses_registered_sdk_for_arguments_and_outcomes(binding, throws):
    runtime, mapping = binding
    unrelated = BamlTypeMap()
    set_type_map(unrelated)

    async def callback(*args):
        assert get_type_map() is mapping
        await asyncio.sleep(0)
        assert get_type_map() is mapping
        if throws:
            raise BamlError(Record(value="declared error"))
        (record,) = args
        assert type(record) is Record
        return Record(value=record.value + "!")

    if throws:
        call = define_function(
            "user.catch_record",
            "async",
            ["callback"],
            runtime=runtime,
            type_map=mapping,
        )
        result = await call(callback)
        assert result.value == "declared error"
    else:
        call = define_function(
            "user.map_record",
            "async",
            ["value", "callback"],
            runtime=runtime,
            type_map=mapping,
        )
        result = await call(Record(value="Ada"), callback)
        assert result.value == "Ada!"
    assert type(result) is Record
    assert get_type_map() is unrelated
    assert _pending_transfer_count() == 0
