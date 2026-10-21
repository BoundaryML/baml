"""Python-only callable reentry: asyncio, ContextVars, and thread semantics."""

from __future__ import annotations

import asyncio
import contextvars

import pytest
from baml_sdk import host_callable_tests as baml


# SDK_PARITY_LINT(skip): sync callback re-entry differs between Python and Node
def test_sync_callback_reenters_sync_baml_python_only():
    calls = []

    def leaf(value):
        calls.append(("leaf", value))
        return value + 1

    def callback(value):
        calls.append(("outer", value))
        return baml.call_int_callback(leaf, value)

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [("outer", 6), ("leaf", 6)]


# SDK_PARITY_LINT(skip): Node cannot synchronously drive an application loop
def test_sync_callback_can_run_async_baml_with_its_own_application_loop_python_only():
    async def work(value):
        async def leaf(item):
            await asyncio.sleep(0)
            return item + 1

        return await baml.call_int_callback_async(leaf, value)

    def callback(value):
        return asyncio.run(work(value))

    assert baml.call_int_callback(callback, 6) == 7


# SDK_PARITY_LINT(skip): Python loop/ContextVar and Node AsyncLocalStorage assertions
@pytest.mark.asyncio
async def test_async_callback_reenters_async_baml_python_only():
    loop = asyncio.get_running_loop()
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")

    async def leaf(value):
        assert asyncio.get_running_loop() is loop
        assert request.get() == "A"
        await asyncio.sleep(0)
        return value + 1

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        return await baml.call_int_callback_async(leaf, value)

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert request.get() == "A"


# SDK_PARITY_LINT(skip): sync callback re-entry differs between Python and Node
@pytest.mark.asyncio
async def test_async_callback_reenters_sync_baml_python_only():
    async def callback(value):
        await asyncio.sleep(0)
        return baml.call_int_callback(lambda item: item + 1, value)

    assert await baml.call_int_callback_async(callback, 6) == 7


# SDK_PARITY_LINT(skip): sync callback re-entry differs between Python and Node
@pytest.mark.asyncio
async def test_sync_callback_of_async_entry_reenters_sync_baml_python_only():
    def callback(value):
        return baml.call_int_callback(lambda item: item + 1, value)

    assert await baml.call_int_callback_async(callback, 6) == 7


# SDK_PARITY_LINT(skip): sync callback re-entry differs between Python and Node
def test_same_callback_recurses_through_baml_without_reusing_an_entry_python_only():
    seen = []

    def callback(value):
        seen.append(value)
        if value == 0:
            return 7
        return baml.call_int_callback(callback, value - 1)

    assert baml.call_int_callback(callback, 3) == 7
    assert seen == [3, 2, 1, 0]
