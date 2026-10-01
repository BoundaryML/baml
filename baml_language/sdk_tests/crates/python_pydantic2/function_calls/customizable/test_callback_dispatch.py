"""Shared callable dispatch contracts; names/assertions match every SDK port.

Use the language's supported entry and callback forms. These cases require
observable results/completion, not a specific thread, loop, or context carrier.
Python-only environment assertions live in test_callback_dispatch_python_only.
"""

import asyncio

import pytest
from baml_sdk import host_callable_tests as baml


@pytest.mark.asyncio
async def test_callable_entry_invokes_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


@pytest.mark.asyncio
async def test_callable_entry_waits_for_callback_completion():
    calls = []

    async def callback(value):
        await asyncio.sleep(0)
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


@pytest.mark.asyncio
async def test_concurrent_calls_keep_callback_results_independent():
    arrivals = []
    all_entered = asyncio.Event()

    async def callback(value):
        arrivals.append(value)
        if len(arrivals) == 4:
            all_entered.set()
        await asyncio.wait_for(all_entered.wait(), 3)
        return value + 1

    calls = [
        asyncio.create_task(baml.call_int_callback_async(callback, value))
        for value in range(4)
    ]
    try:
        assert await asyncio.gather(*calls) == [1, 2, 3, 4]
        assert sorted(arrivals) == [0, 1, 2, 3]
    finally:
        all_entered.set()
        for call in calls:
            if not call.done():
                call.cancel()
        await asyncio.gather(*calls, return_exceptions=True)


@pytest.mark.asyncio
async def test_completed_call_can_reuse_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 1) == 2
    assert await baml.call_int_callback_async(callback, 2) == 3
    assert calls == [1, 2]


@pytest.mark.asyncio
async def test_repeated_dispatches_invoke_callback_in_order():
    calls = []

    def callback(value):
        calls.append(value)
        return str(value)

    assert await baml.call_repeatedly_async(callback, 3) == ["0", "1", "2"]
    assert calls == [0, 1, 2]


@pytest.mark.asyncio
async def test_cancelled_waiter_does_not_end_host_execution():
    from baml_bridge import BamlCallContext

    for late_error in (False, True):
        ctx = BamlCallContext()
        entered = asyncio.Event()
        release = asyncio.Event()
        exited = asyncio.Event()
        exits = []

        async def callback(value):
            entered.set()
            try:
                await release.wait()
            except asyncio.CancelledError:
                # Cooperative cancellation still has to unwind host cleanup.
                await release.wait()
            finally:
                exits.append(value)
                exited.set()
            if late_error:
                raise ValueError("late host failure")
            return value + 1

        pending = asyncio.create_task(baml.call_int_callback_async(callback, 6, _ctx=ctx))
        try:
            await asyncio.wait_for(entered.wait(), 3)
            ctx.abort()
            with pytest.raises(asyncio.CancelledError):
                await asyncio.wait_for(pending, 3)
            assert not exited.is_set()
            release.set()
            await asyncio.wait_for(exited.wait(), 3)
            await asyncio.sleep(0)  # Let the adapter observe actual host exit.
            assert exits == [6]
            assert pending.cancelled()
            assert await baml.call_int_callback_async(lambda value: value + 1, 7) == 8
        finally:
            release.set()
            if not pending.done():
                pending.cancel()
            await asyncio.gather(pending, return_exceptions=True)
