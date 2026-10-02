"""Host tracing acceptance cases from sdk_tests/specs/host_tracing.py."""

import asyncio
import contextvars
import inspect
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from baml_sdk import host_callable_tests as baml
from baml_sdk import invocation, trace
from baml_sdk import invocation_context as context_baml
from baml_sdk.baml.spawn import CancelToken


# SDK_PARITY_LINT(skip): covers Python decorator forms
def test_instrument_decorator_forms_python_only():
    @trace.instrument
    def bare():
        return 1

    @trace.instrument()
    def factory():
        return 2

    @trace.instrument(None)
    def null_options():
        return 3

    @trace.instrument(trace.options(), name="display")
    def configured():
        return 4

    assert (bare(), factory(), null_options(), configured()) == (1, 2, 3, 4)
    assert invocation.current() is None


# SDK_PARITY_LINT(skip): covers Python signatures and positional argument binding
def test_instrument_preserves_sync_execution_python_only():
    caller = threading.get_ident()
    result = object()

    @trace.instrument
    def work(first, /, second=2, *rest, flag=True, **kwargs):
        assert threading.get_ident() == caller
        assert (first, second, rest, flag, kwargs) == (1, 3, (4,), False, {"extra": 6})
        return result

    assert work(1, 3, 4, flag=False, extra=6) is result
    assert (
        str(inspect.signature(work))
        == "(first, /, second=2, *rest, flag=True, **kwargs)"
    )
    with pytest.raises(TypeError):
        work(first=1)
    assert invocation.current() is None


def test_host_context_inherits_and_restores():
    options = trace.hidden().context(
        distinct_id="alice", metadata={"keep": 1, "drop": 2}
    )

    @trace.instrument(trace.context(metadata={"drop": None, "child": True}))
    def child():
        observed = context_baml.current_context()
        assert observed.distinct_id == "alice"
        assert observed.metadata == {"keep": 1, "child": True}
        observed.metadata["keep"] = 99
        assert trace.current_context().metadata == {"keep": 1, "child": True}

    @trace.instrument(options)
    def parent():
        child()
        assert trace.current_context().metadata == {"keep": 1, "drop": 2}

    parent()
    assert trace.current_context().metadata == {}


async def test_host_callback_reentry_context():
    @trace.instrument(trace.context(metadata={"phase": "host"}))
    async def parent():
        async def callback(value):
            assert trace.current_context().metadata == {"phase": "callback"}
            observed = await context_baml.current_context_async()
            assert observed.metadata == {"phase": "callback"}
            return value

        assert (
            await baml.call_int_callback_async(
                callback,
                7,
                _baml={
                    "trace": trace.context(metadata={"phase": "callback"}),
                },
            )
            == 7
        )
        assert trace.current_context().metadata == {"phase": "host"}

    await parent()
    assert invocation.current() is None


async def test_host_callback_cleanup_retains_context():
    source = CancelToken.new()
    entered = asyncio.Event()
    cleaning = asyncio.Event()
    release = asyncio.Event()
    exited = asyncio.Event()
    failures = []

    @trace.instrument
    async def callback(value):
        active = invocation.current()
        assert active is not None
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            try:
                await release.wait()
                assert trace.current_context().metadata == {"request": 7}
                assert active.cancel.is_cancelled() is True
            except Exception as error:
                failures.append(error)
                raise
            finally:
                exited.set()
        return value

    pending = asyncio.create_task(
        baml.call_int_callback_async(
            callback,
            7,
            _baml={"cancel": source, "trace": trace.context(metadata={"request": 7})},
        )
    )
    try:
        await asyncio.wait_for(entered.wait(), 5)
        source.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
        await asyncio.wait_for(cleaning.wait(), 5)
        assert exited.is_set() is False
        assert invocation.current() is None
    finally:
        release.set()
    await asyncio.wait_for(exited.wait(), 5)
    assert failures == []


async def test_host_baml_cancellation_remains_live_after_exit():
    source = CancelToken.new()
    source.cancel()
    retained = None

    @trace.instrument
    async def work():
        nonlocal retained
        retained = invocation.current()
        assert retained is not None
        assert retained.cancel.is_cancelled() is False
        await context_baml.current_context_async(_baml={"cancel": source})

    with pytest.raises(asyncio.CancelledError):
        await work()
    assert retained is not None
    assert retained.cancel.is_cancelled() is True
    assert invocation.current() is None


# SDK_PARITY_LINT(skip): covers Python asyncio task and event-loop identity
async def test_async_host_runs_in_calling_task_python_only():
    task = asyncio.current_task()
    loop = asyncio.get_running_loop()

    @trace.instrument
    async def work():
        assert asyncio.current_task() is task
        assert asyncio.get_running_loop() is loop
        await asyncio.sleep(0)
        assert asyncio.current_task() is task
        return 7

    assert await work() == 7


# SDK_PARITY_LINT(skip): covers Python coroutine creation versus first execution
async def test_coroutine_entry_uses_execution_context_python_only():
    @trace.instrument
    async def work():
        return trace.current_context().metadata

    @trace.instrument(trace.context(metadata={"phase": "creator"}))
    def create():
        return work()

    @trace.instrument(trace.context(metadata={"phase": "consumer"}))
    async def consume(coroutine):
        return await coroutine

    coroutine = create()
    assert invocation.current() is None
    assert await consume(coroutine) == {"phase": "consumer"}


async def test_concurrent_host_invocations_isolate_context():
    ready = [asyncio.Event(), asyncio.Event()]
    release = asyncio.Event()

    async def request(index):
        @trace.instrument(trace.context(metadata={"request": index}))
        async def work():
            ready[index].set()
            await release.wait()
            return (await context_baml.current_context_async()).metadata

        return await work()

    tasks = [asyncio.create_task(request(index)) for index in range(2)]
    try:
        await asyncio.wait_for(asyncio.gather(*(event.wait() for event in ready)), 5)
    finally:
        release.set()
    assert await asyncio.gather(*tasks) == [{"request": 0}, {"request": 1}]
    assert invocation.current() is None


async def test_host_child_outlives_parent():
    release = asyncio.Event()

    @trace.instrument
    async def child():
        await release.wait()
        return (await context_baml.current_context_async()).metadata

    @trace.instrument(trace.context(metadata={"request": 7}))
    async def parent():
        return asyncio.create_task(child())

    task = await parent()
    assert invocation.current() is None
    release.set()
    assert await task == {"request": 7}


# SDK_PARITY_LINT(skip): covers Python ContextVar handoff to a thread pool
def test_host_thread_context_handoff_python_only():
    @trace.instrument
    def child():
        return context_baml.current_context().metadata

    @trace.instrument(trace.context(metadata={"request": 7}))
    def parent(pool):
        copied = contextvars.copy_context()
        assert pool.submit(copied.run, child).result(timeout=5) == {"request": 7}
        assert pool.submit(child).result(timeout=5) == {}

    with ThreadPoolExecutor(max_workers=1) as pool:
        parent(pool)


def test_host_errors_preserve_identity_and_restore_context():
    failure = ValueError("application failure")

    @trace.instrument(trace.context(metadata={"phase": "child"}))
    def child():
        raise failure

    @trace.instrument(trace.context(metadata={"phase": "parent"}))
    def parent():
        with pytest.raises(ValueError) as caught:
            child()
        assert caught.value is failure
        assert trace.current_context().metadata == {"phase": "parent"}

    parent()
    assert invocation.current() is None


# SDK_PARITY_LINT(skip): covers Python task cancellation and asynchronous finally cleanup
async def test_host_cancellation_waits_for_cleanup_python_only():
    entered = asyncio.Event()
    cleaning = asyncio.Event()
    release = asyncio.Event()
    exited = asyncio.Event()

    @trace.instrument(trace.context(metadata={"phase": "host"}))
    async def work():
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()
            assert trace.current_context().metadata == {"phase": "host"}
            exited.set()

    task = asyncio.create_task(work())
    await asyncio.wait_for(entered.wait(), 5)
    task.cancel()
    try:
        await asyncio.wait_for(cleaning.wait(), 5)
        assert not task.done()
        assert not exited.is_set()
    finally:
        release.set()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert exited.is_set()
    assert invocation.current() is None


# SDK_PARITY_LINT(skip): covers unsupported Python generator/decorator shapes
def test_host_rejects_unsupported_configuration_python_only():
    def ordinary():
        return 1

    def generator():
        yield 1

    with pytest.raises(trace.TraceUsageError):
        trace.instrument(trace.span().reserve())  # type: ignore
    with pytest.raises(trace.TraceUsageError):
        trace.instrument(generator)
    with pytest.raises(trace.TraceUsageError):
        trace.instrument(ordinary, name="")
    with pytest.raises(trace.TraceUsageError):
        trace.instrument({"metadata": {}})  # type: ignore


def test_host_capture_preserves_application_values():
    class Opaque:
        def __repr__(self):
            raise AssertionError("capture must not call repr")

        def __iter__(self):
            raise AssertionError("capture must not consume an iterator")

    failure = ValueError(Opaque())
    result = Opaque()
    cycle = []
    cycle.append(cycle)

    @trace.instrument(trace.span(inputs=True, output=True, error=True))
    def work(value, default=7):
        assert default == 7
        if value is failure:
            raise failure
        return result

    assert work(cycle) is result
    assert work("x" * 100_000) is result
    assert work("\U0001f600" * 100_000) is result
    with pytest.raises(ValueError) as caught:
        work(failure)
    assert caught.value is failure
    assert invocation.current() is None


def test_host_current_exposes_generated_cancel_token():
    @trace.instrument
    def work():
        active = invocation.current()
        assert active is not None
        assert active.cancel.is_cancelled() is False
        active.cancel.cancel()
        assert active.cancel.is_cancelled() is True

    work()
    assert invocation.current() is None
