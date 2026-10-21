"""Python callable execution contract, before adding host instrumentation.

These tests call the real generated function_calls SDK directly. No tracing
imports, decorators, fake runtime, dispatch helpers, or recorder assertions.
Nested functions are application callbacks, not a testing abstraction.

Generate the Python function_calls SDK, then run this file against that SDK
with pytest and pytest-asyncio. The async tests use explicit asyncio markers.
This file deliberately lives outside the active SDK test overlay while its
execution contract is audited. Failures are implementation work, not xfails.

The four entry/callback combinations must return results and preserve errors.
Sync callbacks of a sync entry execute on its calling Python thread. Async
entries route callbacks to their originating application loop and copy that
entry's application ContextVars. Registration must not freeze a loop or context.
A sync entry with an async callback may drive a bridge-owned loop; it cannot
borrow a blocked application loop. Such a callback uses its executing loop.
Calling a synchronous entry from a running loop remains synchronous.

No promise that an async callback runs in the caller's exact asyncio Task.
No promise that callback writes to ContextVars mutate the awaiting task.
Returned BAML callables retain their synchronous __call__ interface and offer
an explicit call_async entry; async factories do not change __call__ semantics.
Spawned workers initialize their own SDK/runtime. Live handles do not transfer
process authority. Fork with an already initialized runtime, cross-process
ancestry, custom awaitables, and generators still need separate policy choices.

Comments distinguish observable assertions from scheduling choices left open.
Loop/thread/context/cancellation assertions pin the bridge's execution contract.
Run deadlock-sensitive cases with an external test-process deadline; Python
wait_for cannot interrupt a native synchronous call that blocks its loop.
"""

from __future__ import annotations

import asyncio
import contextvars
import functools
import multiprocessing
import sys
import threading
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor

import pytest

from baml_sdk import host_callable_tests as baml


# ENTRY: All four named-entry/callback combinations.
def test_sync_entry_sync_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [6]


def test_sync_entry_async_callback():
    calls = []

    async def callback(value):
        await asyncio.sleep(0)
        calls.append(value)
        return value + 1

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [6]


@pytest.mark.asyncio
async def test_async_entry_sync_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


@pytest.mark.asyncio
async def test_async_entry_async_callback():
    calls = []

    async def callback(value):
        await asyncio.sleep(0)
        calls.append(value)
        return value + 1

    assert await baml.call_int_callback_async(callback, 6) == 7
    assert calls == [6]


# ENVIRONMENT: Observe where application code actually runs.
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


@pytest.mark.asyncio
async def test_sync_callback_of_async_entry_runs_on_originating_loop_thread():
    loop = asyncio.get_running_loop()
    caller_thread = threading.get_ident()

    def callback(value):
        assert threading.get_ident() == caller_thread
        assert asyncio.get_running_loop() is loop
        return value

    assert await baml.call_int_callback_async(callback, 7) == 7


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


# CONTEXT: Ordinary application ContextVars, without any tracing carrier.
def test_sync_callback_inherits_call_time_context():
    request = contextvars.ContextVar("request", default="missing")

    def callback(value):
        assert request.get() == "at call"
        return value

    request.set("at definition")
    request.set("at call")
    assert baml.call_int_callback(callback, 7) == 7
    assert request.get() == "at call"


@pytest.mark.asyncio
async def test_async_entry_sync_callback_inherits_application_context():
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")

    def callback(value):
        assert request.get() == "A"
        return value

    assert await baml.call_int_callback_async(callback, 7) == 7
    assert request.get() == "A"


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


@pytest.mark.asyncio
async def test_coroutine_creation_does_not_invoke_callback():
    request = contextvars.ContextVar("request", default="missing")
    calls = []

    async def callback(value):
        calls.append(request.get())
        return value

    request.set("creation")
    pending = baml.call_int_callback_async(callback, 7)
    assert calls == []
    request.set("execution")
    assert await pending == 7
    assert calls == ["execution"]

    unused = baml.call_int_callback_async(callback, 8)
    unused.close()
    assert calls == ["execution"]


@pytest.mark.asyncio
async def test_running_call_keeps_launch_context_when_awaited_elsewhere():
    request = contextvars.ContextVar("request", default="missing")
    entered, release = asyncio.Event(), asyncio.Event()
    loop = asyncio.get_running_loop()

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        assert request.get() == "launch"
        entered.set()
        await release.wait()
        assert request.get() == "launch"
        return value

    request.set("launch")
    task = asyncio.create_task(baml.call_int_callback_async(callback, 7))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        request.set("awaiter")
        release.set()
        assert await task == 7
        assert request.get() == "awaiter"
    finally:
        release.set()
        if not task.done():
            task.cancel()
        await asyncio.gather(task, return_exceptions=True)


@pytest.mark.asyncio
async def test_explicit_empty_task_context_does_not_inherit_application_context():
    request = contextvars.ContextVar("request", default="missing")
    request.set("parent")

    async def callback(value):
        assert request.get() == "missing"
        return value

    pending = baml.call_int_callback_async(callback, 7)
    task = contextvars.Context().run(asyncio.create_task, pending)
    assert await task == 7
    assert request.get() == "parent"


# OVERLAP: Same callback object, different in-flight calls and application state.
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


@pytest.mark.asyncio
async def test_callback_task_can_outlive_the_dispatch_that_created_it():
    request = contextvars.ContextVar("request", default="missing")
    request.set("launch")
    loop = asyncio.get_running_loop()
    release = asyncio.Event()
    children = []

    async def child():
        await release.wait()
        assert request.get() == "launch"
        return await baml.call_int_callback_async(lambda value: value + 1, 6)

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        children.append(asyncio.create_task(child()))
        return value

    try:
        assert await baml.call_int_callback_async(callback, 1) == 1
        assert len(children) == 1
        assert not children[0].done()
        request.set("after dispatch")
        release.set()
        assert await children[0] == 7
        assert request.get() == "after dispatch"
    finally:
        release.set()
        await asyncio.gather(*children, return_exceptions=True)


# REENTRY: Python -> BAML -> Python -> BAML -> Python.
def test_sync_callback_reenters_sync_baml():
    calls = []

    def leaf(value):
        calls.append(("leaf", value))
        return value + 1

    def callback(value):
        calls.append(("outer", value))
        return baml.call_int_callback(leaf, value)

    assert baml.call_int_callback(callback, 6) == 7
    assert calls == [("outer", 6), ("leaf", 6)]


def test_sync_callback_can_run_async_baml_with_its_own_application_loop():
    async def work(value):
        async def leaf(item):
            await asyncio.sleep(0)
            return item + 1

        return await baml.call_int_callback_async(leaf, value)

    def callback(value):
        return asyncio.run(work(value))

    assert baml.call_int_callback(callback, 6) == 7


@pytest.mark.asyncio
async def test_async_callback_reenters_async_baml():
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


@pytest.mark.asyncio
async def test_async_callback_reenters_sync_baml():
    async def callback(value):
        await asyncio.sleep(0)
        return baml.call_int_callback(lambda item: item + 1, value)

    assert await baml.call_int_callback_async(callback, 6) == 7


@pytest.mark.asyncio
async def test_sync_callback_of_async_entry_reenters_sync_baml():
    def callback(value):
        return baml.call_int_callback(lambda item: item + 1, value)

    assert await baml.call_int_callback_async(callback, 6) == 7


def test_same_callback_recurses_through_baml_without_reusing_an_entry():
    seen = []

    def callback(value):
        seen.append(value)
        if value == 0:
            return 7
        return baml.call_int_callback(callback, value - 1)

    assert baml.call_int_callback(callback, 3) == 7
    assert seen == [3, 2, 1, 0]


@pytest.mark.asyncio
async def test_same_async_callback_recurses_through_baml():
    seen = []

    async def callback(value):
        seen.append(value)
        await asyncio.sleep(0)
        if value == 0:
            return 7
        return await baml.call_int_callback_async(callback, value - 1)

    assert await baml.call_int_callback_async(callback, 3) == 7
    assert seen == [3, 2, 1, 0]


# CALLABLES OUT: Returned engine closures and Python factories returning callbacks.
def test_baml_returns_callable_that_is_reused_after_factory_returns():
    add = baml.make_adder(10)
    assert callable(add)
    assert add(5) == 15
    assert add(value=7) == 17
    assert baml.call_int_callback(add, 2) == 12


@pytest.mark.asyncio
async def test_async_baml_factory_returns_ordinary_callable():
    add = await baml.make_adder_async(10)
    assert add(5) == 15
    assert await baml.call_int_callback_async(add, 7) == 17


def test_returned_baml_callable_preserves_state_between_calls():
    first = baml.make_counter(40)
    second = baml.make_counter(100)
    assert first() == 41
    assert second() == 101
    assert first() == 42
    assert second() == 102


def test_baml_invokes_callback_returned_by_python_factory():
    calls = []

    def factory():
        def callback(value):
            calls.append(value)
            return f"returned:{value}"

        return callback

    assert baml.call_returned_callback(factory, 7) == "returned:7"
    assert calls == [7]


@pytest.mark.asyncio
async def test_async_python_factory_returns_async_callback_to_baml():
    loop = asyncio.get_running_loop()

    async def factory():
        assert asyncio.get_running_loop() is loop
        await asyncio.sleep(0)

        async def callback(value):
            assert asyncio.get_running_loop() is loop
            await asyncio.sleep(0)
            return f"returned:{value}"

        return callback

    assert await baml.call_returned_callback_async(factory, 7) == "returned:7"


def test_callback_returned_inside_container_is_invokable():
    def factory():
        return [lambda value: f"nested:{value}"]

    assert baml.call_returned_callback_in_list(factory, 7) == "nested:7"


def test_returned_baml_callable_preserves_argument_binding_and_structured_result():
    pair = baml.make_pair_builder(30)
    assert pair(12, "Ada") == baml.Person(name="Ada", age=42)
    assert pair(delta=5, label="Grace") == baml.Person(name="Grace", age=35)
    with pytest.raises(TypeError):
        pair(12, delta=12, label="Ada")
    with pytest.raises(TypeError):
        pair(12, "Ada", extra=1)


def test_returned_baml_closure_retains_host_callback_after_factory_returns():
    calls = []

    def callback(value):
        calls.append(value)
        return value + 1

    forward = baml.make_callback_forwarder(callback)
    assert calls == []  # Receiving/registering a callback never executes it.
    assert forward(6) == 7
    assert forward(7) == 8
    assert baml.call_int_callback(forward, 8) == 9
    assert calls == [6, 7, 8]


@pytest.mark.asyncio
async def test_retained_host_callback_uses_later_invocations_application_context():
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


def test_retained_host_callback_preserves_error_and_remains_reusable():
    failure = ValueError("retained failure")

    def callback(value):
        if value == 1:
            raise failure
        return value

    forward = baml.make_callback_forwarder(callback)
    with pytest.raises(ValueError) as caught:
        forward(1)
    assert caught.value is failure
    assert forward(2) == 2


# PYTHON FORMS: Ordinary callable shapes; no SDK decorators involved.
def test_bound_method_partial_callable_object_and_application_wrapper():
    class Service:
        def callback(self, value):
            return self.offset + value

        def __init__(self, offset):
            self.offset = offset

    class Callable:
        def __call__(self, value):
            return value + 1

    def body(value, *, offset):
        return value + offset

    def application_wrapper(fn):
        @functools.wraps(fn)
        def wrapped(*args, **kwargs):
            return fn(*args, **kwargs)

        return wrapped

    assert baml.call_int_callback(Service(1).callback, 6) == 7
    assert baml.call_int_callback(functools.partial(body, offset=1), 6) == 7
    assert baml.call_int_callback(Callable(), 6) == 7
    assert baml.call_int_callback(application_wrapper(Callable()), 6) == 7


@pytest.mark.asyncio
async def test_async_callable_object_and_partial_are_awaited():
    class Callable:
        async def __call__(self, value):
            await asyncio.sleep(0)
            return value + 1

    async def body(value, *, offset):
        await asyncio.sleep(0)
        return value + offset

    assert await baml.call_int_callback_async(Callable(), 6) == 7
    assert await baml.call_int_callback_async(functools.partial(body, offset=1), 6) == 7


@pytest.mark.asyncio
async def test_ordinary_callback_returning_coroutine_is_awaited():
    loop = asyncio.get_running_loop()

    async def body(value):
        assert asyncio.get_running_loop() is loop
        await asyncio.sleep(0)
        return value + 1

    def callback(value):
        return body(value)

    assert await baml.call_int_callback_async(callback, 6) == 7


def test_optional_callback_arguments_use_python_defaults_and_keyword_binding():
    def callback(value, /, y=8, *, z=9):
        return value * 100 + y * 10 + z

    assert baml.call_callback_with_optional_args_all_unset(callback, 5) == [589]
    assert baml.call_callback_with_optional_args_partially_set(callback, 5) == [
        529,
        583,
    ]
    assert baml.call_callback_with_optional_args_all_set(callback, 5) == [523]


# ERRORS: The actual object survives each route; later calls still work.
def test_sync_entry_preserves_sync_callback_exception():
    failure = ValueError("sync failure")

    def callback(value):
        raise failure

    with pytest.raises(ValueError) as caught:
        baml.call_int_callback(callback, 7)
    assert caught.value is failure
    assert baml.call_int_callback(lambda value: value, 8) == 8


def test_sync_entry_preserves_async_callback_exception():
    failure = ValueError("async failure")

    async def callback(value):
        await asyncio.sleep(0)
        raise failure

    with pytest.raises(ValueError) as caught:
        baml.call_int_callback(callback, 7)
    assert caught.value is failure
    assert baml.call_int_callback(lambda value: value, 8) == 8


@pytest.mark.asyncio
async def test_async_entry_preserves_sync_callback_exception():
    failure = ValueError("sync failure")

    def callback(value):
        raise failure

    with pytest.raises(ValueError) as caught:
        await baml.call_int_callback_async(callback, 7)
    assert caught.value is failure
    assert await baml.call_int_callback_async(lambda value: value, 8) == 8


@pytest.mark.asyncio
async def test_async_entry_preserves_async_callback_exception():
    failure = ValueError("async failure")

    async def callback(value):
        await asyncio.sleep(0)
        raise failure

    with pytest.raises(ValueError) as caught:
        await baml.call_int_callback_async(callback, 7)
    assert caught.value is failure
    assert await baml.call_int_callback_async(lambda value: value, 8) == 8


@pytest.mark.asyncio
async def test_concurrent_callback_exceptions_keep_their_own_identity():
    failures = [ValueError("first"), KeyError("second")]

    async def callback(value):
        await asyncio.sleep(0)
        raise failures[value]

    results = await asyncio.gather(
        baml.call_int_callback_async(callback, 0),
        baml.call_int_callback_async(callback, 1),
        return_exceptions=True,
    )
    assert results[0] is failures[0]
    assert results[1] is failures[1]


def test_callback_can_handle_nested_baml_error_and_continue():
    failure = ValueError("nested failure")

    def leaf(value):
        raise failure

    def callback(value):
        with pytest.raises(ValueError) as caught:
            baml.call_int_callback(leaf, value)
        assert caught.value is failure
        return baml.call_int_callback(lambda item: item + 1, value)

    assert baml.call_int_callback(callback, 6) == 7


def test_callback_baseexception_round_trips_and_later_call_still_works():
    class ApplicationAbort(BaseException):
        pass

    failure = ApplicationAbort()

    def callback(value):
        raise failure

    with pytest.raises(ApplicationAbort) as caught:
        baml.call_int_callback(callback, 7)
    assert caught.value is failure
    assert baml.call_int_callback(lambda value: value, 8) == 8


@pytest.mark.asyncio
async def test_error_named_cancelled_error_is_still_an_application_error():
    class CancelledError(Exception):
        pass

    failure = CancelledError("ordinary failure")

    async def callback(value):
        raise failure

    with pytest.raises(CancelledError) as caught:
        await baml.call_int_callback_async(callback, 7)
    assert caught.value is failure


@pytest.mark.asyncio
async def test_callback_that_handles_its_own_cancellation_can_return_success():
    async def callback(value):
        task = asyncio.current_task()
        assert task is not None
        task.cancel()
        try:
            await asyncio.sleep(0)
        except asyncio.CancelledError:
            return value
        raise AssertionError("Expected callback's self-cancellation")

    assert await baml.call_int_callback_async(callback, 7) == 7


# THREADS: Caller environment belongs to an entry, not a process-global loop.
def test_native_thread_can_make_sync_entry_and_receive_its_callback():
    results = []

    def work():
        worker_thread = threading.get_ident()

        def callback(value):
            assert threading.get_ident() == worker_thread
            return value + 1

        results.append(baml.call_int_callback(callback, 6))

    thread = threading.Thread(target=work)
    thread.start()
    thread.join(timeout=5)
    assert not thread.is_alive()
    assert results == [7]


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


def test_explicit_copied_and_empty_thread_contexts():
    request = contextvars.ContextVar("request", default="missing")
    request.set("A")

    def callback(value):
        return f"{request.get()}:{value}"

    copied = contextvars.copy_context()
    empty = contextvars.Context()
    with ThreadPoolExecutor(max_workers=1) as pool:
        assert (
            pool.submit(copied.run, baml.call_with_callback, callback, 1).result()
            == "A:1"
        )
        assert (
            pool.submit(empty.run, baml.call_with_callback, callback, 2).result()
            == "missing:2"
        )


def test_reused_pool_worker_does_not_reuse_previous_call_context():
    request = contextvars.ContextVar("request", default="missing")

    def callback(value):
        return f"{request.get()}:{value}"

    def work(label, value):
        token = request.set(label)
        try:
            return baml.call_with_callback(callback, value)
        finally:
            request.reset(token)

    with ThreadPoolExecutor(max_workers=1) as pool:
        assert pool.submit(work, "A", 1).result() == "A:1"
        assert pool.submit(work, "B", 2).result() == "B:2"
        assert pool.submit(request.get).result() == "missing"


def test_returned_baml_callable_can_be_called_from_other_threads():
    add = baml.make_adder(10)
    with ThreadPoolExecutor(max_workers=2) as pool:
        first = pool.submit(add, 1)
        second = pool.submit(add, 2)
        assert first.result() == 11
        assert second.result() == 12
    assert add(3) == 13


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


def test_callback_reuse_after_previous_application_loop_is_closed():
    async def callback(value):
        await asyncio.sleep(0)
        return value

    async def work(value):
        return await baml.call_int_callback_async(callback, value)

    assert asyncio.run(work(1)) == 1
    assert asyncio.run(work(2)) == 2


# CANCELLATION: Entry, caller lifetime, and callback lifetime are distinct.
@pytest.mark.asyncio
async def test_call_cancelled_before_first_execution_does_not_dispatch():
    calls = []

    def callback(value):
        calls.append(value)
        return value

    loop = asyncio.get_running_loop()
    previous_factory = loop.get_task_factory()
    loop.set_task_factory(None)  # Deliberately test non-eager task execution.
    try:
        task = asyncio.create_task(baml.call_int_callback_async(callback, 7))
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
    finally:
        loop.set_task_factory(previous_factory)
    assert calls == []


@pytest.mark.asyncio
async def test_cancelling_call_delivers_cancellation_and_allows_callback_cleanup():
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
async def test_taskgroup_failure_cancels_sibling_baml_call_and_preserves_error():
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


@pytest.mark.asyncio
async def test_shielded_call_survives_its_awaiters_cancellation():
    loop = asyncio.get_running_loop()
    entered, release = asyncio.Event(), asyncio.Event()

    async def callback(value):
        assert asyncio.get_running_loop() is loop
        entered.set()
        await release.wait()
        return value

    call = asyncio.create_task(baml.call_int_callback_async(callback, 7))

    async def await_call():
        return await asyncio.shield(call)

    waiter = asyncio.create_task(await_call())
    try:
        await asyncio.wait_for(entered.wait(), 3)
        waiter.cancel()
        with pytest.raises(asyncio.CancelledError):
            await waiter
        assert not call.done()
        release.set()
        assert await call == 7
    finally:
        release.set()
        for task in (waiter, call):
            if not task.done():
                task.cancel()
        await asyncio.gather(waiter, call, return_exceptions=True)


@pytest.mark.asyncio
async def test_cancelling_to_thread_awaiter_does_not_stop_running_sync_callback():
    loop = asyncio.get_running_loop()
    entered, finished = asyncio.Event(), asyncio.Event()
    release = threading.Event()
    results = []

    def callback(value):
        loop.call_soon_threadsafe(entered.set)
        assert release.wait(timeout=5)
        return value

    def work():
        try:
            results.append(baml.call_int_callback(callback, 7))
        finally:
            loop.call_soon_threadsafe(finished.set)

    task = asyncio.create_task(asyncio.to_thread(work))
    try:
        await asyncio.wait_for(entered.wait(), 3)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert not finished.is_set()
        release.set()
        await asyncio.wait_for(finished.wait(), 3)
        assert results == [7]
    finally:
        release.set()
        await asyncio.gather(task, return_exceptions=True)


# PROCESSES: Module-level workers are application code required by spawn.
# Pass ordinary data to workers; create Python callbacks/BAML handles there.
def process_sync_job(value):
    def callback(item):
        return item + 1

    add = baml.make_adder(10)
    return baml.call_int_callback(callback, value), add(value)


def process_async_job(value):
    async def work():
        loop = asyncio.get_running_loop()

        async def callback(item):
            assert asyncio.get_running_loop() is loop
            await asyncio.sleep(0)
            return item + 1

        return await baml.call_int_callback_async(callback, value)

    return asyncio.run(work())


def process_failing_job(value):
    def callback(item):
        raise ValueError(f"child:{item}")

    return baml.call_int_callback(callback, value)


def test_spawned_worker_reuses_its_runtime_for_independent_sync_jobs():
    with ProcessPoolExecutor(
        max_workers=1, mp_context=multiprocessing.get_context("spawn")
    ) as pool:
        assert list(pool.map(process_sync_job, [1, 2])) == [(2, 11), (3, 12)]
    assert baml.call_int_callback(lambda value: value, 7) == 7


def test_spawned_worker_reuses_its_runtime_across_application_loops():
    with ProcessPoolExecutor(
        max_workers=1, mp_context=multiprocessing.get_context("spawn")
    ) as pool:
        assert list(pool.map(process_async_job, [1, 2])) == [2, 3]


def test_child_callback_failure_does_not_poison_parent_runtime():
    with ProcessPoolExecutor(
        max_workers=1, mp_context=multiprocessing.get_context("spawn")
    ) as pool:
        failed = pool.submit(process_failing_job, 1)
        with pytest.raises(ValueError, match="child:1"):
            failed.result(timeout=5)
        assert pool.submit(process_sync_job, 2).result(timeout=5) == (3, 12)
    assert baml.call_int_callback(lambda value: value, 7) == 7


@pytest.mark.skipif(
    "forkserver" not in multiprocessing.get_all_start_methods(),
    reason="forkserver unavailable",
)
def test_forkserver_worker_initializes_its_own_runtime():
    with ProcessPoolExecutor(
        max_workers=1, mp_context=multiprocessing.get_context("forkserver")
    ) as pool:
        assert pool.submit(process_sync_job, 1).result(timeout=5) == (2, 11)
