"""Shared retained-callable and cancelled-waiter contracts.

No specific async closure method or callback cancellation mechanism is part
of the shared contract. Callbacks may finish after their BAML waiter is gone.
"""

import asyncio

import pytest
from baml_sdk.baml.spawn import CancelToken
from baml_sdk import host_callable_tests as baml


@pytest.mark.asyncio
async def test_returned_closure_retains_host_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    forward = await baml.make_callback_forwarder_async(callback)
    assert calls == []
    assert await baml.call_int_callback_async(forward, 6) == 7
    assert await baml.call_int_callback_async(forward, 7) == 8
    assert calls == [6, 7]


@pytest.mark.asyncio
async def test_returned_closure_preserves_callback_error_and_remains_reusable():
    failure = ValueError("retained failure")

    def callback(value):
        if value == 1:
            raise failure
        return value

    forward = await baml.make_callback_forwarder_async(callback)
    with pytest.raises(ValueError) as caught:
        await baml.call_int_callback_async(forward, 1)
    assert caught.value is failure
    assert await baml.call_int_callback_async(forward, 2) == 2


@pytest.mark.asyncio
async def test_cancelled_waiter_stays_cancelled_when_callback_returns_late():
    controller = CancelToken.new()
    entered, release, exited = (asyncio.Event() for _ in range(3))

    async def callback(value):
        entered.set()
        try:
            await release.wait()
        except asyncio.CancelledError:
            # Application elects to finish later; automatic cancellation
            # delivery is tested only in the Python-specific suite.
            await release.wait()
        exited.set()
        return value

    call = asyncio.create_task(
        baml.call_int_callback_async(callback, 1, _baml={"cancel": controller})
    )
    try:
        await asyncio.wait_for(entered.wait(), 3)
        controller.cancel()
        with pytest.raises(asyncio.CancelledError):
            await call
        release.set()
        await asyncio.wait_for(exited.wait(), 3)
        await asyncio.sleep(0)
        with pytest.raises(asyncio.CancelledError):
            await call
        assert await baml.call_int_callback_async(lambda value: value, 2) == 2
    finally:
        release.set()
        controller.cancel()
        await asyncio.gather(call, return_exceptions=True)
