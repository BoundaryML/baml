"""Python callback dispatch: entry thread, originating loop, and call-time context."""

from __future__ import annotations

import asyncio
import contextvars
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from baml_sdk import host_callable_tests as baml


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_sync_entry_sync_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [6]


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_sync_entry_async_callback():
    calls = []

    async def callback(value):
        await asyncio.sleep(0)
        calls.append(value)
        return value + 1

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [6]


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_async_entry_sync_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_async_entry_async_callback():
    calls = []

    async def callback(value):
        await asyncio.sleep(0)
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_sync_callback_runs_on_sync_callers_thread():
    caller_thread = threading.get_ident()
    local = threading.local()
    local.request = "A"

    def callback(value):
        assert threading.get_ident() == caller_thread
        assert local.request == "A"
        local.request = "body ran"
        return value

    assert baml.call_int_callback(callback, 7) == 7
    assert local.request == "body ran"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_sync_callback_of_async_entry_runs_on_originating_loop_thread():
    loop = asyncio.get_running_loop()
    caller_thread = threading.get_ident()

    def callback(value):
        assert threading.get_ident() == caller_thread
        assert asyncio.get_running_loop() is loop
        return value

    assert await baml.call_int_callback_async(callback, 7) == 7


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_async_callback_can_use_originating_loop_resources():
    loop = asyncio.get_running_loop()
    reply = loop.create_future()

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        loop.call_soon(reply.set_result, value)
        return await reply

    assert await baml.call_int_callback_async(callback, 7) == 7
    assert reply.result() == 7


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_sync_entry_in_running_loop_with_sync_callback():
    loop = asyncio.get_running_loop()
    caller_thread = threading.get_ident()

    def callback(value):
        assert threading.get_ident() == caller_thread
        assert asyncio.get_running_loop() is loop
        return value

    assert baml.call_int_callback(callback, 7) == 7
    assert asyncio.get_running_loop() is loop


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_sync_entry_in_running_loop_with_self_contained_async_callback():
    caller_loop = asyncio.get_running_loop()

    async def callback(value):
        # The calling loop is blocked by the sync entry. Do not capture a
        # Future/Event from it here or require reentrant application-loop polls.
        callback_loop = asyncio.get_running_loop()
        assert callback_loop is not caller_loop
        await asyncio.sleep(0)
        return value

    assert baml.call_int_callback(callback, 7) == 7
    assert asyncio.get_running_loop() is caller_loop
    await asyncio.sleep(0)


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_sync_callback_inherits_call_time_context():
    request = contextvars.ContextVar("request", default="missing")

    def callback(value):
        assert request.get() == "at call"
        return value

    request.set("at definition")
    request.set("at call")
    assert baml.call_int_callback(callback, 7) == 7
    assert request.get() == "at call"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_async_entry_sync_callback_inherits_application_context():
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")

    def callback(value):
        assert request.get() == "A"
        return value

    assert await baml.call_int_callback_async(callback, 7) == 7
    assert request.get() == "A"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_async_callback_inherits_application_context_across_suspension():
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")

    async def callback(value):
        assert request.get() == "A"
        await asyncio.sleep(0)
        assert request.get() == "A"
        return value

    assert await baml.call_int_callback_async(callback, 7) == 7
    assert request.get() == "A"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_concurrent_calls_share_callback_without_sharing_context():
    request = contextvars.ContextVar("request", default=-1)
    loop = asyncio.get_running_loop()
    arrivals = 0
    all_entered = asyncio.Event()

    async def callback(value):
        nonlocal arrivals
        assert asyncio.get_running_loop() is loop
        assert request.get() == value
        arrivals += 1
        if arrivals == 4:
            all_entered.set()
        await asyncio.wait_for(all_entered.wait(), 3)
        assert request.get() == value
        return value

    tasks = []
    for value in range(4):
        token = request.set(value)
        try:
            tasks.append(
                asyncio.create_task(baml.call_int_callback_async(callback, value))
            )
        finally:
            request.reset(token)

    try:
        assert await asyncio.gather(*tasks) == [0, 1, 2, 3]
        assert arrivals == 4
        assert request.get() == -1
    finally:
        all_entered.set()
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_completed_call_does_not_pin_reused_callback_to_old_context():
    request = contextvars.ContextVar("request", default="missing")

    async def callback(value):
        await asyncio.sleep(0)
        return f"{request.get()}:{value}"

    request.set("A")
    assert await baml.call_with_callback_async(callback, 1) == "A:1"
    request.set("B")
    assert await baml.call_with_callback_async(callback, 2) == "B:2"
    assert await callback(3) == "B:3"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
@pytest.mark.asyncio
async def test_to_thread_sync_entry_inherits_copied_application_context():
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")
    caller_thread = threading.get_ident()

    def work():
        worker_thread = threading.get_ident()
        assert worker_thread != caller_thread
        assert request.get() == "A"

        def callback(value):
            assert threading.get_ident() == worker_thread
            assert request.get() == "A"
            return value

        return baml.call_int_callback(callback, 7)

    assert await asyncio.to_thread(work) == 7
    assert request.get() == "A"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_two_application_loops_on_two_threads_share_callback_without_rerouting():
    request = contextvars.ContextVar("request", default="missing")
    barrier = threading.Barrier(2)
    loops = {}

    async def callback(value):
        assert asyncio.get_running_loop() is loops[value]
        assert request.get() == value
        await asyncio.sleep(0)
        return value

    def work(value):
        async def main():
            request.set(value)
            loops[value] = asyncio.get_running_loop()
            barrier.wait(timeout=3)
            return await baml.call_int_callback_async(callback, value)

        return asyncio.run(main())

    with ThreadPoolExecutor(max_workers=2) as pool:
        first = pool.submit(work, 1)
        second = pool.submit(work, 2)
        assert first.result() == 1
        assert second.result() == 2
    assert loops[1] is not loops[2]
    assert request.get() == "missing"


# SDK_PARITY_LINT(skip): Python thread, event loop, and ContextVar semantics
def test_callback_reuse_after_previous_application_loop_is_closed():
    async def callback(value):
        await asyncio.sleep(0)
        return value

    async def work(value):
        return await baml.call_int_callback_async(callback, value)

    assert asyncio.run(work(1)) == 1
    assert asyncio.run(work(2)) == 2


# SDK_PARITY_LINT(skip): Python ContextVar semantics with a bridge-owned loop
def test_sync_entry_async_callback_copies_context_across_suspension():
    request = contextvars.ContextVar("request", default="missing")
    request.set("caller")

    async def callback(value):
        assert request.get() == "caller"
        request.set("callback")
        await asyncio.sleep(0)
        assert request.get() == "callback"
        return value

    assert baml.call_int_callback(callback, 7) == 7
    assert request.get() == "caller"


# SDK_PARITY_LINT(skip): Python ContextVar isolation between dispatch tasks
@pytest.mark.asyncio
async def test_repeated_dispatches_each_start_with_entry_context():
    request = contextvars.ContextVar("request", default="missing")
    request.set("caller")

    async def callback(value):
        assert request.get() == "caller"
        request.set(str(value))
        await asyncio.sleep(0)
        assert request.get() == str(value)
        return str(value)

    assert await baml.call_repeatedly_async(callback, 3) == ["0", "1", "2"]
    assert request.get() == "caller"


# SDK_PARITY_LINT(skip): Python asyncio task factory failures during dispatch
@pytest.mark.asyncio
async def test_task_factory_failure_completes_dispatch_and_preserves_exception():
    loop = asyncio.get_running_loop()
    previous_factory = loop.get_task_factory()
    failure = RuntimeError("application task factory rejected callback")
    called = []

    def factory(loop, coroutine, **kwargs):
        if coroutine.cr_code.co_name == "_dispatch":
            raise failure
        if previous_factory is not None:
            return previous_factory(loop, coroutine, **kwargs)
        return asyncio.Task(coroutine, loop=loop, **kwargs)

    def callback(value):
        called.append(value)
        return value

    loop.set_task_factory(factory)
    try:
        with pytest.raises(RuntimeError) as caught:
            await baml.call_int_callback_async(callback, 7)
        assert caught.value is failure
        assert called == []
    finally:
        loop.set_task_factory(previous_factory)
    assert await baml.call_int_callback_async(callback, 8) == 8
    assert called == [8]
