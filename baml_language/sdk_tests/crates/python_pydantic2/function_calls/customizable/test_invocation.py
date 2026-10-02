"""BEP-81 public invocation conformance, through the generated Python SDK."""

from __future__ import annotations

import asyncio
import contextvars
from itertools import product

import pytest
from baml_bridge import BamlCancelledError
from baml_bridge._execution_context import current as current_execution_context

from baml_sdk import (
    BamlOptions,
    OptBox,
    hello_world,
    hello_world_async,
    optional_args_probe,
    optional_args_probe_async,
    trace,
)
from baml_sdk import host_callable_tests as baml
from baml_sdk.baml.spawn import CancelToken
from baml_sdk.experimental import invoke, invoke_async


# invocation_options: the four forms, empty controls, and omission.
def test_four_call_forms():
    opts: BamlOptions = {"timeout_ms": 1000}
    assert optional_args_probe(1) == [1, 5, 99]
    assert optional_args_probe(1, opt1=7) == [1, 7, 99]
    assert optional_args_probe(1, _baml=opts) == [1, 5, 99]
    assert optional_args_probe(1, opt1=7, _baml=opts) == [1, 7, 99]


async def test_four_call_forms_async():
    opts: BamlOptions = {"timeout_ms": 1000}
    assert await optional_args_probe_async(1) == [1, 5, 99]
    assert await optional_args_probe_async(1, opt1=7) == [1, 7, 99]
    assert await optional_args_probe_async(1, _baml=opts) == [1, 5, 99]
    assert await optional_args_probe_async(1, opt1=7, _baml=opts) == [1, 7, 99]


def test_empty_controls():
    assert hello_world() == "hello world"
    assert hello_world(_baml={}) == "hello world"
    assert hello_world(_baml=None) == "hello world"
    assert (
        hello_world(_baml={"trace": None, "cancel": None, "timeout_ms": None})
        == "hello world"
    )


def test_omitted_argument_is_not_null():
    assert optional_args_probe(1, _baml={}) == [1, 5, 99]
    assert optional_args_probe(1, opt1=None, _baml={}) == [1, None, 99]


def test_unknown_control_rejected():
    try:
        hello_world(_baml={"unknown": True})  # type: ignore[typeddict-unknown-key]
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("Unknown control must reject before admission")


def test_invalid_timeout_rejected():
    for value in [-1, 0.5, float("inf"), float("nan"), 2147483648]:
        try:
            hello_world(_baml={"timeout_ms": value})  # type: ignore[typeddict-item]
        except (TypeError, ValueError):
            pass
        else:
            raise AssertionError(f"Invalid timeout accepted: {value}")


def test_timeout_upper_bound_accepted():
    assert hello_world(_baml={"timeout_ms": 2147483647}) == "hello world"


# invocation_surfaces: each surface shares controls and argument binding.
def test_methods_accept_controls():
    box = OptBox.make(1, _baml={})
    assert box.base == 8
    assert box.probe(2, _baml={}) == [8, 2, 5]
    assert box.probe(2, opt1=None, _baml={}) == [8, 2, None]


async def test_methods_accept_controls_async():
    box = await OptBox.make_async(1, _baml={})
    assert box.base == 8
    assert await box.probe_async(2, _baml={}) == [8, 2, 5]


async def test_returned_callable_accepts_controls():
    add = baml.make_adder(3)
    assert add(4, _baml={}) == 7
    assert await add.call_async(4, _baml={}) == 7


def test_dynamic_call_accepts_controls():
    assert invoke("user.optional_args_probe", {"arg0": 1}, _baml={}) == [1, 5, 99]
    assert invoke("user.optional_args_probe", {"arg0": 1, "opt1": None}, _baml={}) == [
        1,
        None,
        99,
    ]
    # Dynamic application maps never strip a key named _baml as a control.
    # Generic _types is separate; specialized handles reject new type bindings.
    from baml_sdk import reflect

    for binding in [int, reflect.Type.of(int), None]:
        assert invoke("user.generic_tests.identity", {"x": 17}, _types={"T": binding}, _baml={}) == 17


# SDK_PARITY_LINT(skip): Python dynamic type bindings accept Python classes and reflected BamlType handles
async def test_dynamic_type_bindings_async_python_only():
    from baml_sdk import reflect

    for binding in [int, reflect.Type.of(int), None]:
        assert await invoke_async("user.generic_tests.identity", {"x": 17}, _types={"T": binding}, _baml={}) == 17


# invocation_lifecycle: admission, linked sources, and reservation ownership.
async def test_pre_cancelled_call_does_not_enter_callback():
    token = CancelToken.new()
    token.cancel()
    calls = []

    def callback(value):
        calls.append(value)
        return value

    try:
        await baml.call_int_callback_async(callback, 1, _baml={"cancel": token})
    except asyncio.CancelledError as error:
        assert isinstance(error.reason, BamlCancelledError)
    else:
        raise AssertionError("Pre-cancelled call must reject")
    assert calls == []


async def test_zero_timeout_does_not_enter_callback():
    calls = []

    def callback(value):
        calls.append(value)
        return value

    try:
        await baml.call_int_callback_async(callback, 1, _baml={"timeout_ms": 0})
    except asyncio.CancelledError as error:
        assert isinstance(error.reason, BamlCancelledError)
    else:
        raise AssertionError("Explicit zero expires immediately")
    assert calls == []


async def test_composite_token_observes_every_source():
    for index in [0, 1]:
        sources = [CancelToken.new(), CancelToken.new()]
        combined = CancelToken.any(sources)
        sources[index].cancel()
        assert combined.is_cancelled()
        try:
            await hello_world_async(_baml={"cancel": combined})
        except asyncio.CancelledError as error:
            assert isinstance(error.reason, BamlCancelledError)
        else:
            raise AssertionError("Every composite source must cancel admission")
        assert not sources[1 - index].is_cancelled()


async def test_child_cancellation_does_not_cancel_input():
    upstream = CancelToken.new()

    def callback(value):
        active = trace.current_cancel_token()
        assert active is not None
        active.cancel()
        # Exact token operations remain serviceable under ambient cancellation.
        assert active.is_cancelled()
        assert not upstream.is_cancelled()
        return value

    try:
        await baml.call_int_callback_async(callback, 1, _baml={"cancel": upstream})
    except asyncio.CancelledError as error:
        assert isinstance(error.reason, BamlCancelledError)
    # Callback success may win the completion race; either outcome is valid.
    assert not upstream.is_cancelled()
    assert await hello_world_async(_baml={"cancel": upstream}) == "hello world"


async def test_rejected_admission_does_not_consume_reservation():
    reserved = trace.span().reserve()
    token = CancelToken.new()
    token.cancel()
    try:
        await hello_world_async(_baml={"trace": reserved, "cancel": token})
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Pre-cancelled admission must reject")
    assert hello_world(_baml={"trace": reserved}) == "hello world"


async def test_concurrent_reservation_attaches_once():
    reserved = trace.span().reserve()
    results = await asyncio.gather(
        hello_world_async(_baml={"trace": reserved}),
        hello_world_async(_baml={"trace": reserved}),
        return_exceptions=True,
    )
    assert sum(result == "hello world" for result in results) == 1
    assert sum(isinstance(result, BaseException) for result in results) == 1
    # Losing call must have the documented attachment failure, never enter body.


# invocation_callbacks: state visible to ordinary, uninstrumented callbacks.
async def test_callback_frame_and_reentry():
    assert trace.current_cancel_token() is None
    upstream = CancelToken.new()
    frames = []

    async def leaf(value):
        active = trace.current_cancel_token()
        assert active is not None
        assert not active.is_cancelled()
        # Re-entry has a fresh execution identity; wrapper identity is unspecified.
        return value + 1

    async def callback(value):
        active = trace.current_cancel_token()
        assert active is not None
        frames.append(active)
        await asyncio.sleep(0)
        assert trace.current_cancel_token() is not None
        resumed = trace.current_cancel_token()
        assert resumed is not None
        assert not resumed.is_cancelled()
        return await baml.call_int_callback_async(leaf, value, _baml={})

    assert (
        await baml.call_int_callback_async(callback, 6, _baml={"cancel": upstream}) == 7
    )
    assert trace.current_cancel_token() is None
    assert not upstream.is_cancelled()


async def test_null_controls_preserve_inherited_context():
    async def callback(value):
        nested = await baml.call_int_callback_async(
            lambda inner: trace.current_context().metadata["request"],
            value,
            _baml={"trace": None, "cancel": None, "timeout_ms": None},
        )
        return nested

    assert (
        await baml.call_int_callback_async(
            callback,
            1,
            _baml={"trace": trace.hidden().context(metadata={"request": 7})},
        )
        == 7
    )
    # Context survives even with recording disabled; no extra plumbing span.


@pytest.mark.parametrize("async_entry", [False, True], ids=["sync-entry", "async-entry"])
@pytest.mark.parametrize(
    "async_callbacks",
    list(product((False, True), repeat=4)),
    ids=lambda modes: "callbacks-" + "".join("A" if mode else "S" for mode in modes),
)
@pytest.mark.parametrize("async_reentry", [False, True], ids=["sync-reentry", "async-reentry"])
@pytest.mark.parametrize(
    "inherited_controls",
    [{}, {"_baml": {}}, {"_baml": None}, {"_baml": {"trace": None, "cancel": None, "timeout_ms": None}}],
    ids=["omitted-controls", "empty-controls", "none-controls", "null-fields"],
)
# SDK_PARITY_LINT(skip): Python sync/async re-entry and ContextVar propagation
def test_layered_callback_context_inheritance_and_restoration_python_only(
    async_entry, async_callbacks, async_reentry, inherited_controls
):
    # A(3) sets context; B(2) inherits; C(1) patches; D(0) inherits.
    # Each BAML invocation takes a callback which makes the next BAML call.
    # Async callbacks also exercise both sync and async re-entry into BAML.
    host_request = contextvars.ContextVar("layered_host_request", default="missing")
    host_token = host_request.set("application")
    root_metadata = {"request": "A", "keep": 7, "remove": 9}
    child_metadata = {"request": "C", "keep": 7, "child": 1}
    visited = []

    def assert_layer(depth):
        active = trace.current_cancel_token()
        assert active is not None
        assert not active.is_cancelled()
        assert host_request.get() == "application"
        current = trace.current_context()
        assert current.metadata == (root_metadata if depth >= 2 else child_metadata)
        assert current.distinct_id == ("root-id" if depth >= 2 else "child-id")

    def child_controls(depth):
        if depth == 2:
            return {"_baml": {"trace": trace.context(
                distinct_id="child-id", metadata={"request": "C", "child": 1, "remove": None}
            )}}
        return inherited_controls

    def sync_callback(depth):
        assert_layer(depth)
        visited.append(depth)
        if depth == 0:
            return 7
        callback = async_callback if async_callbacks[4 - depth] else sync_callback
        result = baml.call_int_callback(callback, depth - 1, **child_controls(depth))
        # Descendant overrides must not change this callback's frame.
        assert_layer(depth)
        return result

    async def async_callback(depth):
        assert_layer(depth)
        visited.append(depth)
        await asyncio.sleep(0)
        assert_layer(depth)
        if depth == 0:
            return 7
        callback = async_callback if async_callbacks[4 - depth] else sync_callback
        controls = child_controls(depth)
        if async_reentry:
            result = await baml.call_int_callback_async(callback, depth - 1, **controls)
        else:
            result = baml.call_int_callback(callback, depth - 1, **controls)
        assert_layer(depth)
        await asyncio.sleep(0)
        assert_layer(depth)
        return result

    try:
        assert trace.current_cancel_token() is None
        assert trace.current_context().metadata == {}
        assert trace.current_context().distinct_id is None
        callback = async_callback if async_callbacks[0] else sync_callback
        controls = {"trace": trace.hidden().context(distinct_id="root-id", metadata=root_metadata)}
        if async_entry:
            result = asyncio.run(baml.call_int_callback_async(callback, 3, _baml=controls))
        else:
            result = baml.call_int_callback(callback, 3, _baml=controls)
        assert result == 7
        assert visited == [3, 2, 1, 0]
        assert trace.current_cancel_token() is None
        assert trace.current_context().metadata == {}
        assert trace.current_context().distinct_id is None
        assert host_request.get() == "application"
    finally:
        host_request.reset(host_token)


@pytest.mark.parametrize("leaf_throws", [False, True], ids=["success", "exception"])
async def test_layered_callback_context_restores_after_returned_callable(leaf_throws):
    original = ValueError("leaf failed")
    sibling_requests = []

    async def leaf(value):
        assert trace.current_cancel_token() is not None
        assert trace.current_context().metadata == {"request": "C", "keep": 7}
        await asyncio.sleep(0)
        assert trace.current_context().metadata == {"request": "C", "keep": 7}
        if leaf_throws:
            raise original
        return value + 1

    # Retention must preserve the callable, not the factory's invocation context.
    forward = await baml.make_callback_forwarder_async(
        leaf, _baml={"trace": trace.context(metadata={"request": "factory"})}
    )

    async def overridden(value):
        assert trace.current_cancel_token() is not None
        assert trace.current_context().metadata == {"request": "C", "keep": 7}
        try:
            return await forward.call_async(value)
        finally:
            assert trace.current_cancel_token() is not None
            assert trace.current_context().metadata == {"request": "C", "keep": 7}

    async def sibling(value):
        assert trace.current_cancel_token() is not None
        sibling_requests.append(trace.current_context().metadata.copy())
        return value

    async def inherited(value):
        assert trace.current_context().metadata == {"request": "A", "keep": 7}
        try:
            result = await baml.call_int_callback_async(
                overridden, value, _baml={"trace": trace.context(metadata={"request": "C"})}
            )
        except ValueError as error:
            assert leaf_throws
            assert error is original
            result = value + 1
        finally:
            assert trace.current_cancel_token() is not None
            assert trace.current_context().metadata == {"request": "A", "keep": 7}
        # A later sibling inherits A, never the earlier child's C override.
        assert await baml.call_int_callback_async(sibling, value) == value
        assert sibling_requests == [{"request": "A", "keep": 7}]
        return result

    async def outer(value):
        assert trace.current_cancel_token() is not None
        assert trace.current_context().metadata == {"request": "A", "keep": 7}
        result = await baml.call_int_callback_async(inherited, value)
        assert trace.current_context().metadata == {"request": "A", "keep": 7}
        return result

    assert await baml.call_int_callback_async(
        outer, 6, _baml={"trace": trace.hidden().context(metadata={"request": "A", "keep": 7})}
    ) == 7
    assert trace.current_cancel_token() is None
    assert trace.current_context().metadata == {}


async def test_configuration_snapshot_is_not_live():
    opts: BamlOptions = {"trace": trace.context(metadata={"request": 7})}

    async def callback(value):
        opts["trace"] = trace.context(metadata={"request": 99})
        await asyncio.sleep(0)
        assert trace.current_context().metadata["request"] == 7
        return value

    assert await baml.call_int_callback_async(callback, 1, _baml=opts) == 1
    changed = opts["trace"]
    assert changed is not None
    assert changed.inspect().context.metadata["request"] == 99


async def test_live_token_cancels_after_admission():
    source = CancelToken.new()
    combined = CancelToken.any([source])
    started = asyncio.Event()
    exited = asyncio.Event()
    release_cleanup = asyncio.Event()

    async def callback(value):
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            await release_cleanup.wait()
            exited.set()
        return value

    async def run():
        try:
            await baml.call_int_callback_async(callback, 1, _baml={"cancel": combined})
        except asyncio.CancelledError as error:
            # Python 3.10 drops exception attributes at an asyncio.Task boundary.
            # Verify the SDK's structured cause before crossing that boundary.
            assert isinstance(error.reason, BamlCancelledError)
            raise

    pending = asyncio.create_task(run())
    await started.wait()
    source.cancel()
    try:
        await pending
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Live composite source must cancel execution")
    assert not exited.is_set()
    release_cleanup.set()
    await exited.wait()
    # Waiter cancellation precedes actual host exit; cleanup still has an owner.


async def test_retained_effective_token_stays_live_after_callback():
    source = CancelToken.new()
    retained = []

    def callback(value):
        active = trace.current_cancel_token()
        assert active is not None
        retained.append(active)
        return value

    assert (
        await baml.call_int_callback_async(callback, 1, _baml={"cancel": source}) == 1
    )
    assert trace.current_cancel_token() is None
    source.cancel()
    assert retained[0].is_cancelled()
    # Keeping the token does not restore the frame or callback completion ID.


async def test_retained_invocation_resolves_token_after_callback():
    source = CancelToken.new()
    retained = []

    def callback(value):
        retained.append(current_execution_context())
        return value

    assert await baml.call_int_callback_async(callback, 1, _baml={"cancel": source}) == 1
    assert trace.current_cancel_token() is None
    source.cancel()
    active = retained[0]
    assert active is not None
    assert active.cancel.is_cancelled()
    assert active.cancel is active.cancel


# invocation_options_python_only: execution-time snapshot is Python-specific.
async def test_options_snapshot_when_coroutine_starts_python_only():
    opts: BamlOptions = {"timeout_ms": 1000}
    pending = hello_world_async(_baml=opts)
    opts["timeout_ms"] = 0
    try:
        await pending
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Edits before first coroutine execution must apply")


def test_boolean_timeout_rejected_python_only():
    try:
        hello_world(_baml={"timeout_ms": True})
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("bool is not an integer timeout")


@pytest.mark.parametrize("options", [[], 3, {"cancel": object()}, {"trace": object()}])
def test_invalid_controls_rejected(options):
    with pytest.raises(TypeError):
        hello_world(_baml=options)


def test_legacy_controls_rejected():
    from baml_bridge import BamlError

    for name in ("_ctx", "_trace", "_call"):
        with pytest.raises(BamlError):
            hello_world(**{name: None})


def test_specialized_callable_rejects_type_bindings():
    add = baml.make_adder(3)
    with pytest.raises(TypeError):
        add(4, _types={"T": int})  # type: ignore[call-arg]
    with pytest.raises(TypeError):
        invoke(add, {"value": 4}, _types={"T": int})


async def test_deadline_reentry_does_not_reset_budget():
    from baml_sdk import throws_test

    async def callback(value):
        await asyncio.sleep(0.04)
        await throws_test.SleepMs_async(500, _baml={"timeout_ms": 1000})
        return value

    with pytest.raises(asyncio.CancelledError):
        await asyncio.wait_for(
            baml.call_int_callback_async(callback, 1, _baml={"timeout_ms": 100}), 2
        )


async def test_dynamic_call_async_accepts_controls():
    from baml_sdk.experimental import invoke_async

    assert await invoke_async("user.optional_args_probe", {"arg0": 1}, _baml={}) == [1, 5, 99]
    add = baml.make_adder(3)
    assert await invoke_async(add, {"value": 4}, _baml={}) == 7


async def test_encoding_time_counts_against_deadline_python_only():
    import time

    calls = []

    class SlowList(list):
        def __iter__(self):
            time.sleep(0.05)
            return super().__iter__()

    def callback(values):
        calls.append(values)
        return values

    with pytest.raises(asyncio.CancelledError):
        await baml.call_list_roundtrip_callback_async(
            callback, SlowList([1]), _baml={"timeout_ms": 10}
        )
    assert calls == []


def test_dynamic_application_map_preserves_control_like_keys():
    from baml_bridge import BamlError

    with pytest.raises(BamlError, match="_baml"):
        invoke("user.hello_world", {"_baml": None}, _baml={})
