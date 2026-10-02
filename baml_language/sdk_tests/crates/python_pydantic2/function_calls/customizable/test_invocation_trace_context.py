"""Shared invocation-context contracts; no instrumentation or trace hooks.

Coverage follows host_tracing.py CTX-03/09/10, ASYNC-04/06, CB-01,
and ID-01/02, plus the invocation.py callback and admission scenarios.
"""

import asyncio
import contextvars

import pytest

from baml_sdk import host_callable_tests as baml
from baml_sdk import invocation, trace
from baml_sdk import invocation_context as context_baml
from baml_sdk.baml.spawn import CancelToken


async def test_callback_captures_internal_baml_context():
    def callback(value):
        assert invocation.current() is not None
        current = trace.current_context()
        assert current.distinct_id == "internal-id"
        assert current.metadata == {"phase": "internal", "keep": 7}
        return value

    assert await context_baml.internal_context_async(
        callback,
        7,
        _baml={"trace": trace.hidden().context(
            distinct_id="root-id",
            metadata={"phase": "root", "keep": 7, "remove": 9},
        )},
    ) == 7
    assert invocation.current() is None
    assert trace.current_context().metadata == {}


async def test_current_context_returns_detached_snapshot():
    def callback(value):
        observed = trace.current_context()
        observed.metadata["request"] = 99
        observed.distinct_id = "changed"
        assert trace.current_context().metadata == {"request": 7}
        assert trace.current_context().distinct_id == "original"
        return value

    assert await baml.call_int_callback_async(
        callback, 1,
        _baml={"trace": trace.context(distinct_id="original", metadata={"request": 7})},
    ) == 1


async def test_concurrent_invocations_isolate_context():
    ready = [asyncio.Event(), asyncio.Event()]
    release = asyncio.Event()

    async def request(index):
        async def callback(value):
            assert trace.current_context().metadata == {"request": index}
            ready[index].set()
            await release.wait()
            assert trace.current_context().metadata == {"request": index}
            nested = await context_baml.current_context_async()
            assert nested.metadata == {"request": index}
            return value

        return await baml.call_int_callback_async(
            callback, index, _baml={"trace": trace.hidden().context(metadata={"request": index})}
        )

    pending = [asyncio.create_task(request(index)) for index in range(2)]
    try:
        await asyncio.wait_for(asyncio.gather(*(event.wait() for event in ready)), 5)
    finally:
        release.set()
    assert await asyncio.gather(*pending) == [0, 1]
    assert invocation.current() is None
    assert trace.current_context().metadata == {}


async def test_retained_context_survives_parent_completion():
    captured = []

    def callback(value):
        captured.append(contextvars.copy_context())
        return value

    assert await baml.call_int_callback_async(
        callback, 1, _baml={"trace": trace.hidden().context(metadata={"request": 7})}
    ) == 1
    assert invocation.current() is None
    assert captured[0].run(trace.current_context).metadata == {"request": 7}

    def child_callback(value):
        assert trace.current_context().metadata == {"request": 7}
        return value

    # Constructing a coroutine does not execute it in a copied Python context.
    # Task creation is the native carrier handoff; the parent has already exited.
    child = captured[0].run(asyncio.create_task, baml.call_int_callback_async(child_callback, 7))
    assert await child == 7
    assert invocation.current() is None
    assert trace.current_context().metadata == {}


async def test_reservation_does_not_inherit():
    reserved = trace.span().context(metadata={"request": 7}).reserve()

    async def callback(value):
        nested = await context_baml.current_context_async()
        assert nested.metadata == {"request": 7}
        return value

    assert await baml.call_int_callback_async(callback, 1, _baml={"trace": reserved}) == 1
    # The root consumed the reservation. Descendants succeeded without reusing it.
    with pytest.raises(TypeError, match="invalid trace reservation: AlreadyAttached"):
        await context_baml.current_context_async(_baml={"trace": reserved})


async def test_hidden_mode_does_not_inherit():
    assert await context_baml.child_span_under_hidden_parent_async(
        _baml={"trace": trace.hidden()}
    )


async def test_cancelled_waiter_preserves_callback_context():
    source = CancelToken.new()
    started = asyncio.Event()
    cleanup_started = asyncio.Event()
    release = asyncio.Event()
    exited = asyncio.Event()
    cleanup_errors = []

    async def callback(value):
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleanup_started.set()
            try:
                assert trace.current_context().metadata == {"request": 7}
                assert invocation.current().cancel.is_cancelled()
                await release.wait()
                assert trace.current_context().metadata == {"request": 7}
            except Exception as error:
                # The waiter is already cancelled, so it cannot report a
                # callback assertion failure during physical cleanup.
                cleanup_errors.append(error)
                raise
            finally:
                exited.set()
        return value

    pending = asyncio.create_task(baml.call_int_callback_async(
        callback, 1,
        _baml={"cancel": source, "trace": trace.hidden().context(metadata={"request": 7})},
    ))
    try:
        await asyncio.wait_for(started.wait(), 5)
        source.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
        await asyncio.wait_for(cleanup_started.wait(), 5)
        assert not exited.is_set()
        assert invocation.current() is None
        assert trace.current_context().metadata == {}
    finally:
        release.set()
    await asyncio.wait_for(exited.wait(), 5)
    assert cleanup_errors == []
