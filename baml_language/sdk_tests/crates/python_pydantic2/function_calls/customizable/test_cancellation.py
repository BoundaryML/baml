from __future__ import annotations

import asyncio
import threading
import time

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_bridge import BamlCallContext
from baml_sdk.baml import BamlPanic
from baml_sdk.baml.panics import Cancelled
from baml_sdk import throws_test

_MAX_CANCELLATION_SECONDS = 0.5


def _assert_cancelled_panic(exc: BamlPanic) -> None:
    assert isinstance(exc.value, Cancelled)


def _assert_cancelled_reason(exc: asyncio.CancelledError) -> None:
    reason = getattr(exc, "reason", None)
    assert type(reason).__name__ == "BamlCancelledError"
    assert isinstance(reason.value, Cancelled)


def _assert_fast_cancellation(start: float) -> None:
    elapsed = time.monotonic() - start
    assert elapsed < _MAX_CANCELLATION_SECONDS, f"cancellation took {elapsed:.3f}s"


def test_cancellation_sync_call_returns_none():
    assert throws_test.SleepMs(1) is None


async def test_cancellation_async_call_returns_none():
    assert await throws_test.SleepMs_async(1) is None


def test_cancellation_sync_cancel_via_call_context():
    start = time.monotonic()
    ctx = BamlCallContext()
    timer = threading.Timer(0.05, ctx.abort)

    timer.start()
    try:
        with pytest.raises(BamlPanic) as exc_info:
            throws_test.SleepMs(2000, _ctx=ctx)
    finally:
        timer.cancel()

    _assert_cancelled_panic(exc_info.value)
    _assert_fast_cancellation(start)


async def test_cancellation_async_cancel_via_call_context():
    start = time.monotonic()
    ctx = BamlCallContext()
    task = asyncio.create_task(throws_test.SleepMs_async(2000, _ctx=ctx))

    await asyncio.sleep(0.05)
    ctx.abort()

    with pytest.raises(asyncio.CancelledError) as exc_info:
        await task

    _assert_cancelled_reason(exc_info.value)
    _assert_fast_cancellation(start)


async def test_cancellation_async_cancel_via_task_cancel():
    start = time.monotonic()
    task = asyncio.create_task(throws_test.SleepMs_async(2000))

    await asyncio.sleep(0.05)
    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    _assert_fast_cancellation(start)


async def test_cancellation_async_cancel_via_task_group_sibling():
    start = time.monotonic()

    async def _fail_soon() -> None:
        await asyncio.sleep(0.05)
        raise RuntimeError("cancel siblings")

    with pytest.raises(ExceptionGroup) as exc_info:
        async with asyncio.TaskGroup() as group:
            task = group.create_task(throws_test.SleepMs_async(2000))
            group.create_task(_fail_soon())

    assert any(isinstance(exc, RuntimeError) for exc in exc_info.value.exceptions)
    assert task.cancelled()
    _assert_fast_cancellation(start)


async def test_cancellation_async_cancel_via_asyncio_timeout():
    start = time.monotonic()
    with pytest.raises(TimeoutError):
        await asyncio.wait_for(throws_test.SleepMs_async(2000), timeout=0.05)

    _assert_fast_cancellation(start)
