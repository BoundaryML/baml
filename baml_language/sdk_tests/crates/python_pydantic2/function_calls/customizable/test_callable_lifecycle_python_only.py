"""Python-only callable lifecycle: asyncio, ContextVars, and thread semantics."""

from __future__ import annotations

import asyncio
import contextvars
import gc
import sys
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from baml_bridge import BamlCancelledError
from baml_sdk.baml.spawn import CancelToken
from baml_sdk import host_callable_tests as baml


# SDK_PARITY_LINT(skip): Python explicit async closure entry and context capture
@pytest.mark.asyncio
async def test_closure_call_async_uses_invoking_loop_and_context_python_only():
    request = contextvars.ContextVar("request", default="missing")
    request.set("registration")
    loop = asyncio.get_running_loop()
    seen = []

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        seen.append(request.get())
        reply = loop.create_future()
        loop.call_soon(reply.set_result, value)
        return await reply

    forward = await baml.make_callback_forwarder_async(callback)
    request.set("coroutine creation")
    pending = forward.call_async(1)
    assert seen == []
    request.set("first execution")
    assert await pending == 1
    request.set("second execution")
    assert await forward.call_async(value=2) == 2
    unused = forward.call_async(3)
    unused.close()
    assert seen == ["first execution", "second execution"]


# SDK_PARITY_LINT(skip): Python returned closure reuse across application loops
def test_closure_call_async_can_be_reused_after_creation_loop_closes_python_only():
    request = contextvars.ContextVar("request", default="missing")
    seen = []

    async def callback(value):
        seen.append((asyncio.get_running_loop(), request.get()))
        await asyncio.sleep(0)
        return value

    async def create():
        request.set("factory")
        return await baml.make_callback_forwarder_async(callback)

    forward = asyncio.run(create())

    async def invoke(label, value):
        request.set(label)
        return await forward.call_async(value)

    assert asyncio.run(invoke("first", 1)) == 1
    assert asyncio.run(invoke("second", 2)) == 2
    assert seen[0][0] is not seen[1][0]
    assert [label for _, label in seen] == ["first", "second"]


# SDK_PARITY_LINT(skip): Python closure async entry argument and error semantics
@pytest.mark.asyncio
async def test_closure_call_async_preserves_binding_and_exception_identity_python_only():
    build = baml.make_pair_builder(10)
    assert await build.call_async(delta=2, label="Ada") == baml.Person(
        name="Ada", age=12
    )
    with pytest.raises(TypeError, match="multiple values"):
        await build.call_async(2, delta=3, label="Ada")
    with pytest.raises(TypeError, match="unexpected keyword"):
        await build.call_async(2, "Ada", unknown=3)

    failure = ValueError("retained callback failed")

    async def callback(value):
        if value == 1:
            raise failure
        return value

    forward = await baml.make_callback_forwarder_async(callback)
    with pytest.raises(ValueError) as caught:
        await forward.call_async(1)
    assert caught.value is failure
    assert await forward.call_async(2) == 2


# SDK_PARITY_LINT(skip): Python closure async entry cancellation delivery
@pytest.mark.asyncio
async def test_closure_call_async_cancels_its_retained_callback_python_only():
    entered, exited = asyncio.Event(), asyncio.Event()

    async def callback(value):
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            exited.set()

    forward = await baml.make_callback_forwarder_async(callback)
    call = asyncio.create_task(forward.call_async(1))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        call.cancel()
        with pytest.raises(asyncio.CancelledError):
            await call
        await asyncio.wait_for(exited.wait(), 3)
    finally:
        call.cancel()
        await asyncio.gather(call, return_exceptions=True)


# SDK_PARITY_LINT(skip): Python callback cancellation under an explicit controller
@pytest.mark.asyncio
async def test_repeated_abort_does_not_interrupt_async_callback_cleanup_python_only():
    controller = CancelToken.new()
    entered, cleaning, release, exited = (asyncio.Event() for _ in range(4))

    async def callback(value):
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()
            exited.set()

    call = asyncio.create_task(
        baml.call_int_callback_async(callback, 1, _baml={"cancel": controller})
    )
    try:
        await asyncio.wait_for(entered.wait(), 3)
        controller.cancel()
        await asyncio.wait_for(cleaning.wait(), 3)
        controller.cancel()
        await asyncio.sleep(0)
        assert not exited.is_set()
        release.set()
        with pytest.raises(asyncio.CancelledError):
            await call
        await asyncio.wait_for(exited.wait(), 3)
    finally:
        release.set()
        call.cancel()
        await asyncio.gather(call, return_exceptions=True)


# SDK_PARITY_LINT(skip): Python cancellation on a bridge-owned callback loop
def test_sync_entry_cancellation_reaches_async_callback_on_worker_loop_python_only():
    controller = CancelToken.new()
    entered, exited = threading.Event(), threading.Event()

    async def callback(value):
        entered.set()
        try:
            await asyncio.get_running_loop().create_future()
        finally:
            exited.set()

    with ThreadPoolExecutor(max_workers=1) as pool:
        call = pool.submit(baml.call_int_callback, callback, 1, _baml={"cancel": controller})
        try:
            assert entered.wait(3)
        finally:
            controller.cancel()
        with pytest.raises(BamlCancelledError):
            call.result(timeout=3)
        assert exited.wait(3)


# SDK_PARITY_LINT(skip): Python callback suppression of cancellation and late results
@pytest.mark.asyncio
async def test_callback_suppresses_task_cancellation_and_returns_late_python_only():
    entered, finished = asyncio.Event(), asyncio.Event()

    async def callback(value):
        entered.set()
        try:
            await asyncio.Event().wait()
        except asyncio.CancelledError:
            await asyncio.sleep(0)
            finished.set()
            return value

    call = asyncio.create_task(baml.call_int_callback_async(callback, 1))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        call.cancel()
        with pytest.raises(asyncio.CancelledError):
            await call
        await asyncio.wait_for(finished.wait(), 3)
        assert call.cancelled()
        assert await baml.call_int_callback_async(lambda value: value, 2) == 2
    finally:
        call.cancel()
        await asyncio.gather(call, return_exceptions=True)


# SDK_PARITY_LINT(skip): asyncio tasks retain callback bodies until actual completion
@pytest.mark.asyncio
async def test_suspended_callback_survives_gc_until_cancellation_cleanup_finishes_python_only():
    entered, exited = asyncio.Event(), asyncio.Event()

    async def callback(value):
        entered.set()
        try:
            await asyncio.get_running_loop().create_future()
        finally:
            exited.set()

    call = asyncio.create_task(baml.call_int_callback_async(callback, 1))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        gc.collect()
        assert not exited.is_set()
        call.cancel()
        with pytest.raises(asyncio.CancelledError):
            await call
        await asyncio.wait_for(exited.wait(), 3)
    finally:
        call.cancel()
        await asyncio.gather(call, return_exceptions=True)


# SDK_PARITY_LINT(skip): Python task factory cancellation before callback execution
@pytest.mark.asyncio
async def test_callback_task_cancelled_before_first_execution_completes_call_python_only():
    loop = asyncio.get_running_loop()
    previous_factory = loop.get_task_factory()
    calls = []

    def factory(loop, coroutine, **kwargs):
        task = asyncio.Task(coroutine, loop=loop, **kwargs)
        if coroutine.cr_code.co_name == "_dispatch":
            task.cancel()
        return task

    def callback(value):
        calls.append(value)
        return value

    loop.set_task_factory(factory)
    try:
        with pytest.raises(asyncio.CancelledError):
            await baml.call_int_callback_async(callback, 1)
        assert calls == []
    finally:
        loop.set_task_factory(previous_factory)
    assert await baml.call_int_callback_async(callback, 2) == 2
    assert calls == [2]


# SDK_PARITY_LINT(skip): Python callback task and event loop ownership
@pytest.mark.asyncio
async def test_retained_host_callback_uses_later_invocations_application_context_python_only():
    request = contextvars.ContextVar("request", default="missing")
    request.set("registration")
    loop = asyncio.get_running_loop()
    seen = []

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        seen.append(request.get())
        await asyncio.sleep(0)
        return value

    forward = await baml.make_callback_forwarder_async(callback)
    assert seen == []
    request.set("first invocation")
    assert await baml.call_int_callback_async(forward, 1) == 1
    request.set("second invocation")
    assert await baml.call_int_callback_async(forward, 2) == 2
    assert seen == ["first invocation", "second invocation"]


# SDK_PARITY_LINT(skip): Python callback task and event loop ownership
@pytest.mark.asyncio
async def test_cancelling_call_delivers_cancellation_and_allows_callback_cleanup_python_only():
    loop = asyncio.get_running_loop()
    entered, cleaning, release, exited = (asyncio.Event() for _ in range(4))

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()
            exited.set()

    task = asyncio.create_task(baml.call_int_callback_async(callback, 7))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        task.cancel()
        await asyncio.wait_for(cleaning.wait(), 3)
        assert not exited.is_set()
        release.set()
        with pytest.raises(asyncio.CancelledError):
            await task
        await asyncio.wait_for(exited.wait(), 3)
        assert await baml.call_int_callback_async(lambda value: value, 8) == 8
    finally:
        release.set()
        if not task.done():
            task.cancel()
        await asyncio.gather(task, return_exceptions=True)


@pytest.mark.skipif(
    sys.version_info < (3, 11), reason="TaskGroup requires Python 3.11+"
)
@pytest.mark.asyncio
# SDK_PARITY_LINT(skip): Python callback task and event loop ownership
async def test_taskgroup_failure_cancels_sibling_baml_call_and_preserves_error_python_only():
    loop = asyncio.get_running_loop()
    entered, exited = asyncio.Event(), asyncio.Event()
    failure = ValueError("sibling failed")

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            exited.set()

    async def failing():
        await asyncio.wait_for(entered.wait(), 3)
        raise failure

    with pytest.raises(ExceptionGroup) as caught:  # noqa: F821 — Python 3.11+, guarded above
        async with asyncio.TaskGroup() as group:
            call = group.create_task(baml.call_int_callback_async(callback, 7))
            group.create_task(failing())
    assert caught.value.exceptions == (failure,)
    assert call.cancelled()
    await asyncio.wait_for(exited.wait(), 3)
    assert await baml.call_int_callback_async(lambda value: value, 8) == 8
