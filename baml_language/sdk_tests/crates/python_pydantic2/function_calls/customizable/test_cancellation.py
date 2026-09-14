from __future__ import annotations

import asyncio
import sys
import threading
import time
from pathlib import Path

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_bridge import BamlCallContext
from baml_bridge import BamlCancelledError
from baml_sdk.baml.panics import Cancelled
from baml_sdk import throws_test

_MAX_CANCELLATION_SECONDS = 0.5


def _assert_cancelled_panic(exc: BamlCancelledError) -> None:
    assert isinstance(exc.value, Cancelled)


def _assert_cancelled_reason(exc: asyncio.CancelledError) -> None:
    reason = getattr(exc, "reason", None)
    assert isinstance(reason, BamlCancelledError)
    assert isinstance(reason.value, Cancelled)


def _assert_fast_cancellation(start: float) -> None:
    elapsed = time.monotonic() - start
    assert elapsed < _MAX_CANCELLATION_SECONDS, f"cancellation took {elapsed:.3f}s"


async def _wait_for_marker(path: Path) -> None:
    deadline = time.monotonic() + 2.0
    while not path.exists():
        assert time.monotonic() < deadline, f"timed out waiting for marker: {path.name}"
        await asyncio.sleep(0.005)


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
        with pytest.raises(BamlCancelledError) as exc_info:
            throws_test.SleepMs(2000, _ctx=ctx)
    finally:
        timer.cancel()

    _assert_cancelled_panic(exc_info.value)
    _assert_fast_cancellation(start)


async def test_cancellation_async_cancel_via_call_context():
    start = time.monotonic()
    ctx = BamlCallContext()

    async def _abort_soon() -> None:
        await asyncio.sleep(0.05)
        ctx.abort()

    abort_task = asyncio.create_task(_abort_soon())

    with pytest.raises(asyncio.CancelledError) as exc_info:
        await throws_test.SleepMs_async(2000, _ctx=ctx)

    await abort_task

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


@pytest.mark.skipif(
    sys.version_info < (3, 11), reason="asyncio.TaskGroup requires Python 3.11+"
)
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
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(throws_test.SleepMs_async(2000), timeout=0.05)

    _assert_fast_cancellation(start)


# SDK_PARITY_LINT(skip): drives Python asyncio cancellation (task.cancel / wait_for)
@pytest.mark.parametrize("mode", ["task_cancel", "asyncio_timeout"])
async def test_cancellation_async_cancel_skips_later_step(tmp_path: Path, mode: str):
    """A cancel delivered mid-sleep must stop the run, not just detach the host.

    Returning fast only proves the host stopped waiting. This waits out the full
    native sleep afterwards and asserts the post-sleep step never ran, which is
    what rules out an orphaned continuation still doing work (and still spending)
    for a call the caller already abandoned.
    """
    entry = tmp_path / "entered"
    later = tmp_path / "later"
    sleep_ms = 2000

    task = asyncio.create_task(
        throws_test.SleepThenMarkMs_async(sleep_ms, str(entry), str(later))
    )
    await _wait_for_marker(entry)
    # measured from the cancellation point: task startup and marker polling are
    # not cancellation latency, and folding them in makes this flaky on a slow
    # or loaded runner
    start = time.monotonic()

    if mode == "task_cancel":
        assert task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
    else:
        with pytest.raises(asyncio.TimeoutError):
            await asyncio.wait_for(task, timeout=0.05)

    assert task.cancelled()
    _assert_fast_cancellation(start)

    # Past the point where the sleep would have finished on its own.
    await asyncio.sleep((sleep_ms / 1000) + 0.2)
    assert not later.exists(), "post-sleep step ran after cancellation"
