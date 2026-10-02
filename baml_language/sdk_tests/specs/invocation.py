"""BEP-081 invocation audit spec, using the function_calls fixture.

Ordinary calls and assertions; no runner, mock runtime, or dispatch helpers.
These examples are outside the active test overlay until BEP-081 exists.
The shared scenario names/groups match invocation.ts. Language-only scenarios
have a _python_only suffix. Existing callable suites cover execution patterns.
Run potentially blocking scenarios with an external process deadline.

Boundary-only requirements, to test directly when the ABI exists:
  - allocation binds the runtime immediately; replacement cannot redirect it;
  - pre-submission cancellation persists; failed encoding releases allocation;
  - submission consumes allocation once, including native preparation failure;
  - wrong-runtime/stale target, token, frame, and reservation handles reject;
  - target pinning survives release of the original wrapper after preparation;
  - deadline includes encoding/preparation; inherited re-entry cannot reset it;
  - cancellation arriving before enqueue/start is retained, duplicates harmless;
  - waiter retirement does not close a live callback or prevent late cleanup;
  - late/duplicate completions dispose payloads and cannot resettle/reuse IDs;
  - every envelope, including empty controls, requires protocol 1 / ABI 3;
  - old envelopes/controls and partial protocol implementations reject;
  - defaults/hooks never run before admission; omitted defaults run once.
These are requirements, not claims that public assertions inspect native state.
Stream producer cancellation and visibility during defaults/hooks remain open
in BEP-081; this spec does not settle them. No instrumentation or flush.
"""

from __future__ import annotations

import asyncio

from baml_bridge import BamlCancelledError
from baml_sdk import (
    BamlOptions,
    OptBox,
    hello_world,
    hello_world_async,
    invocation,
    invoke,
    optional_args_probe,
    optional_args_probe_async,
    trace,
)
from baml_sdk import host_callable_tests as baml
from baml_sdk.baml.spawn import CancelToken


# invocation_options: the four forms, empty controls, and omission.
def four_call_forms():
    opts: BamlOptions = {"timeout_ms": 1000}
    assert optional_args_probe(1) == [1, 5, 99]
    assert optional_args_probe(1, opt1=7) == [1, 7, 99]
    assert optional_args_probe(1, _baml=opts) == [1, 5, 99]
    assert optional_args_probe(1, opt1=7, _baml=opts) == [1, 7, 99]


async def four_call_forms_async():
    opts: BamlOptions = {"timeout_ms": 1000}
    assert await optional_args_probe_async(1) == [1, 5, 99]
    assert await optional_args_probe_async(1, opt1=7) == [1, 7, 99]
    assert await optional_args_probe_async(1, _baml=opts) == [1, 5, 99]
    assert await optional_args_probe_async(1, opt1=7, _baml=opts) == [1, 7, 99]


def empty_controls():
    assert hello_world() == "hello world"
    assert hello_world(_baml={}) == "hello world"
    assert hello_world(_baml=None) == "hello world"
    assert (
        hello_world(_baml={"trace": None, "cancel": None, "timeout_ms": None})
        == "hello world"
    )


def omitted_argument_is_not_null():
    assert optional_args_probe(1, _baml={}) == [1, 5, 99]
    assert optional_args_probe(1, opt1=None, _baml={}) == [1, None, 99]


def unknown_control_rejected():
    try:
        hello_world(_baml={"unknown": True})
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("Unknown control must reject before admission")


def invalid_timeout_rejected():
    for value in [-1, 0.5, float("inf"), float("nan"), 2147483648]:
        try:
            hello_world(_baml={"timeout_ms": value})
        except (TypeError, ValueError):
            pass
        else:
            raise AssertionError(f"Invalid timeout accepted: {value}")


def timeout_upper_bound_accepted():
    assert hello_world(_baml={"timeout_ms": 2147483647}) == "hello world"


# invocation_surfaces: each surface shares controls and argument binding.
def methods_accept_controls():
    box = OptBox.make(1, _baml={})
    assert box.base == 8
    assert box.probe(2, _baml={}) == [8, 2, 5]
    assert box.probe(2, opt1=None, _baml={}) == [8, 2, None]


async def methods_accept_controls_async():
    box = await OptBox.make_async(1, _baml={})
    assert box.base == 8
    assert await box.probe_async(2, _baml={}) == [8, 2, 5]


async def returned_callable_accepts_controls():
    add = baml.make_adder(3)
    assert add(4, _baml={}) == 7
    assert await add.call_async(4, _baml={}) == 7


def dynamic_call_accepts_controls():
    assert invoke("user.optional_args_probe", {"arg0": 1}, _baml={}) == [1, 5, 99]
    assert invoke("user.optional_args_probe", {"arg0": 1, "opt1": None}, _baml={}) == [
        1,
        None,
        99,
    ]
    # Dynamic application maps never strip a key named _baml as a control.
    # Generic _types is separate; specialized handles reject new type bindings.


# invocation_lifecycle: admission, linked sources, and reservation ownership.
async def pre_cancelled_call_does_not_enter_callback():
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


async def zero_timeout_does_not_enter_callback():
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


async def composite_token_observes_every_source():
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


async def child_cancellation_does_not_cancel_input():
    upstream = CancelToken.new()

    def callback(value):
        active = invocation.current()
        assert active is not None
        active.cancel.cancel()
        # Exact token operations remain serviceable under ambient cancellation.
        assert active.cancel.is_cancelled()
        assert not upstream.is_cancelled()
        return value

    try:
        await baml.call_int_callback_async(callback, 1, _baml={"cancel": upstream})
    except asyncio.CancelledError as error:
        assert isinstance(error.reason, BamlCancelledError)
    # Callback success may win the completion race; either outcome is valid.
    assert not upstream.is_cancelled()
    assert await hello_world_async(_baml={"cancel": upstream}) == "hello world"


async def rejected_admission_does_not_consume_reservation():
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


async def concurrent_reservation_attaches_once():
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
async def callback_frame_and_reentry():
    assert invocation.current() is None
    upstream = CancelToken.new()
    frames = []

    async def leaf(value):
        active = invocation.current()
        assert active is not None
        assert not active.cancel.is_cancelled()
        # Re-entry has a fresh execution identity; wrapper identity is unspecified.
        return value + 1

    async def callback(value):
        active = invocation.current()
        assert active is not None
        frames.append(active)
        await asyncio.sleep(0)
        assert invocation.current() is not None
        assert not invocation.current().cancel.is_cancelled()
        return await baml.call_int_callback_async(leaf, value, _baml={})

    assert (
        await baml.call_int_callback_async(callback, 6, _baml={"cancel": upstream}) == 7
    )
    assert invocation.current() is None
    assert not upstream.is_cancelled()


async def null_controls_preserve_inherited_context():
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


async def configuration_snapshot_is_not_live():
    opts: BamlOptions = {"trace": trace.context(metadata={"request": 7})}

    async def callback(value):
        opts["trace"] = trace.context(metadata={"request": 99})
        await asyncio.sleep(0)
        assert trace.current_context().metadata["request"] == 7
        return value

    assert await baml.call_int_callback_async(callback, 1, _baml=opts) == 1
    assert opts["trace"].inspect().context.metadata["request"] == 99


async def live_token_cancels_after_admission():
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

    pending = asyncio.create_task(
        baml.call_int_callback_async(callback, 1, _baml={"cancel": combined})
    )
    await started.wait()
    source.cancel()
    try:
        await pending
    except asyncio.CancelledError as error:
        assert isinstance(error.reason, BamlCancelledError)
    else:
        raise AssertionError("Live composite source must cancel execution")
    assert not exited.is_set()
    release_cleanup.set()
    await exited.wait()
    # Waiter cancellation precedes actual host exit; cleanup still has an owner.


async def retained_effective_token_stays_live_after_callback():
    source = CancelToken.new()
    retained = []

    def callback(value):
        active = invocation.current()
        assert active is not None
        retained.append(active.cancel)
        return value

    assert (
        await baml.call_int_callback_async(callback, 1, _baml={"cancel": source}) == 1
    )
    assert invocation.current() is None
    source.cancel()
    assert retained[0].is_cancelled()
    # Keeping the token does not restore the frame or callback completion ID.


# invocation_options_python_only: execution-time snapshot is Python-specific.
async def options_snapshot_when_coroutine_starts_python_only():
    opts: BamlOptions = {"timeout_ms": 1000}
    pending = hello_world_async(_baml=opts)
    opts["timeout_ms"] = 0
    try:
        await pending
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Edits before first coroutine execution must apply")


def boolean_timeout_rejected_python_only():
    try:
        hello_world(_baml={"timeout_ms": True})
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("bool is not an integer timeout")
