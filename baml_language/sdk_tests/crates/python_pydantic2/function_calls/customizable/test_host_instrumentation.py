"""Host tracing acceptance cases from sdk_tests/specs/host_tracing.py."""

import asyncio
import contextvars
import enum
import inspect
import json
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from baml_sdk import host_callable_tests as baml
from baml_sdk import trace
from baml_sdk import execution_context_tests as context_baml
from baml_sdk.baml.spawn import CancelToken
from baml_bridge import _host_capture
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


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
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None


async def test_host_callback_cleanup_retains_context():
    source = CancelToken.new()
    entered = asyncio.Event()
    cleaning = asyncio.Event()
    release = asyncio.Event()
    exited = asyncio.Event()
    failures = []

    @trace.instrument
    async def callback(value):
        active = trace.current_cancel_token()
        assert active is not None
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            try:
                await release.wait()
                assert trace.current_context().metadata == {"request": 7}
                assert active.is_cancelled() is True
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
        assert trace.current_cancel_token() is None
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
        retained = trace.current_cancel_token()
        assert retained is not None
        assert retained.is_cancelled() is False
        await context_baml.current_context_async(_baml={"cancel": source})

    with pytest.raises(asyncio.CancelledError):
        await work()
    assert retained is not None
    assert retained.is_cancelled() is True
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None
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
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None
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
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None


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
    assert trace.current_cancel_token() is None


def test_host_current_exposes_generated_cancel_token():
    @trace.instrument
    def work():
        active = trace.current_cancel_token()
        assert active is not None
        assert active.is_cancelled() is False
        active.cancel()
        assert active.is_cancelled() is True

    work()
    assert trace.current_cancel_token() is None


async def test_callback_marker_context_precedence():
    @trace.instrument(
        trace.context(
            distinct_id="marker", metadata={"stage": "marker", "keep": 1, "remove": 2}
        )
    )
    async def callback(value):
        expected = {"stage": "call", "keep": 1, "caller": True}
        assert trace.current_context().distinct_id == "call"
        assert trace.current_context().metadata == expected
        assert (await context_baml.current_context_async()).metadata == expected
        return value

    assert (
        await baml.call_configured_callback_async(
            callback,
            7,
            trace.context(
                distinct_id="call", metadata={"stage": "call", "remove": None}
            ),
            _baml={
                "trace": trace.context(
                    distinct_id="caller", metadata={"stage": "caller", "caller": True}
                )
            },
        )
        == 7
    )
    assert trace.current_cancel_token() is None


async def test_callback_marker_adopts_sync_body():
    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    def callback(value):
        assert trace.current_context().metadata == {"stage": "call"}
        return value

    assert (
        await baml.call_configured_callback_async(
            callback, 7, trace.context(metadata={"stage": "call"})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


async def test_callback_marker_defaults_override_inherited_context():
    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    async def callback(value):
        assert trace.current_context().metadata == {"stage": "marker", "keep": 1}
        return value

    assert (
        await baml.call_int_callback_async(
            callback,
            7,
            _baml={
                "trace": trace.context(metadata={"stage": "caller", "keep": 1}),
            },
        )
        == 7
    )
    assert trace.current_cancel_token() is None


async def test_callback_marker_adopts_once_during_recursion():
    stages = []

    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    async def callback(value):
        stages.append(trace.current_context().metadata["stage"])
        if value:
            child = await callback(value - 1)
            assert trace.current_context().metadata["stage"] == "call"
            return child + 1
        return 0

    assert (
        await baml.call_configured_callback_async(
            callback, 1, trace.context(metadata={"stage": "call"})
        )
        == 1
    )
    assert stages == ["call", "marker"]
    assert trace.current_cancel_token() is None


async def test_callback_marker_only_outer_wrapper_adopts():
    @trace.instrument(trace.context(metadata={"stage": "inner"}))
    async def inner(value):
        assert trace.current_context().metadata == {
            "stage": "inner",
            "outer": True,
            "call": True,
        }
        return value

    outer = trace.instrument(trace.context(metadata={"stage": "outer", "outer": True}))(
        inner
    )
    assert (
        await baml.call_configured_callback_async(
            outer, 7, trace.context(metadata={"stage": "call", "call": True})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


async def test_callback_marker_concurrent_reuse_and_later_direct_call():
    ready = [asyncio.Event(), asyncio.Event()]
    release = asyncio.Event()

    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    async def callback(value):
        if value < 2:
            ready[value].set()
            await release.wait()
        expected = "marker" if value == 2 else value
        assert trace.current_context().metadata == {"stage": expected}
        return value

    pending = [
        asyncio.create_task(
            baml.call_configured_callback_async(
                callback, index, trace.context(metadata={"stage": index})
            )
        )
        for index in range(2)
    ]
    try:
        await asyncio.wait_for(asyncio.gather(*(event.wait() for event in ready)), 5)
    finally:
        release.set()
    assert await asyncio.gather(*pending) == [0, 1]
    assert await callback(2) == 2
    assert trace.current_cancel_token() is None


async def test_callback_third_party_wrapper_is_not_adopted():
    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    async def marked(value):
        assert trace.current_context().metadata == {"stage": "marker"}
        return value

    # SDK identity must not follow __wrapped__ or copied marker attributes.
    import functools

    @functools.wraps(marked)
    async def third_party(value):
        assert trace.current_context().metadata == {"stage": "call"}
        return await marked(value)

    assert (
        await baml.call_configured_callback_async(
            third_party, 7, trace.context(metadata={"stage": "call"})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


async def test_unmarked_callback_explicit_context_and_reentry():
    async def callback(value):
        assert trace.current_context().metadata == {"stage": "call"}
        child = await context_baml.current_context_async(
            _baml={"trace": trace.context(metadata={"stage": "child"})}
        )
        assert child.metadata == {"stage": "child"}
        assert trace.current_context().metadata == {"stage": "call"}
        return value

    assert (
        await baml.call_configured_callback_async(
            callback, 7, trace.context(metadata={"stage": "call"})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


# SDK_PARITY_LINT(skip): covers Python bound method identity via its exact SDK function
async def test_callback_bound_method_adoption_python_only():
    class Service:
        @trace.instrument(trace.context(metadata={"stage": "marker"}))
        async def callback(self, value):
            assert trace.current_context().metadata == {"stage": "call"}
            return value

    assert (
        await baml.call_configured_callback_async(
            Service().callback, 7, trace.context(metadata={"stage": "call"})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


# SDK_PARITY_LINT(skip): covers Python synchronous BAML entry and inline callback dispatch
def test_callback_marker_sync_entry_python_only():
    @trace.instrument(trace.context(metadata={"stage": "marker"}))
    def callback(value):
        assert trace.current_context().metadata == {"stage": "call"}
        return value

    assert (
        baml.call_configured_callback(
            callback, 7, trace.context(metadata={"stage": "call"})
        )
        == 7
    )
    assert trace.current_cancel_token() is None


class CaptureSeverity(str, enum.Enum):
    None_ = "None"
    Low = "Low"


class CountingMeta(type):
    compared = 0

    def __eq__(cls, other):
        CountingMeta.compared += 1
        return NotImplemented

    __hash__ = type.__hash__


class MetaCompared(metaclass=CountingMeta):
    pass


# SDK_PARITY_LINT(skip): checks the Python capture adapter's wire output directly
def test_host_capture_wire_stays_decodable_python_only():
    # A lone surrogate drops only its own string, not the whole observation.
    assert json.loads(_host_capture.capture({"ok": 1, "path": "file\udcff.txt"})) == [
        "map",
        [["ok", ["number", 1]], ["path", ["unavailable"]]],
    ]

    # Depth markers count against the value budget, so the wire stays small.
    nested = [[0] * 255] * 256
    for _ in range(7):
        nested = [nested]
    wire = _host_capture.capture(nested)
    assert len(wire) < 16 * 1024 and '["values"]' in wire

    # Enum captures carry the BAML variant, not a renamed Python member name.
    previous = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={},
            enums={"user.CaptureSeverity": (__name__, "CaptureSeverity")},
            type_aliases={},
        )
    )
    try:
        assert json.loads(_host_capture.capture(CaptureSeverity.None_)) == [
            "enum",
            "user.CaptureSeverity",
            "None",
        ]
    finally:
        set_type_map(previous)

    # Base-class checks never call an application metaclass's __eq__.
    assert json.loads(_host_capture.capture(MetaCompared())) == ["unavailable"]
    assert CountingMeta.compared == 0
