"""Shared callback re-entry contracts, independent of a language's call mode."""

import asyncio

import pytest
from baml_sdk import host_callable_tests as baml


@pytest.mark.asyncio
async def test_callback_reenters_baml():
    calls = []

    async def leaf(value):
        calls.append(("leaf", value))
        await asyncio.sleep(0)
        return value + 1

    async def callback(value):
        calls.append(("outer", value))
        return await baml.call_int_callback_async(leaf, value)

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [("outer", 6), ("leaf", 6)]


@pytest.mark.asyncio
async def test_same_callback_recurses_through_baml():
    calls = []

    async def callback(value):
        calls.append(value)
        await asyncio.sleep(0)
        if value == 0:
            return 7
        return await baml.call_int_callback_async(callback, value - 1)

    assert await baml.call_int_callback_async(callback, 3) == 7
    assert calls == [3, 2, 1, 0]
