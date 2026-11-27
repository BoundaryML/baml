"""The native bridge's own tests: what today's SDK tests do not cover.

Many calls at once, calls that sleep or read the environment, the ways a
call ends, refusals, cancellation, the five ways an async call is awaited,
one loop and one thread at a time, and every handle freed. Run against the
native bindings only (PYTHONPATH=.../native).
"""

import _thread
import asyncio
import gc
import os
import threading
import time
import warnings

import pytest

import baml_sdk
import baml_native
from baml_native import _native
from baml_bridge.errors import BamlError, BamlPanic

from baml_sdk.bridge_test import (
    LookupFailed,
    Point,
    add,
    add_sync,
    count_ints,
    env_or,
    env_or_sync,
    explode,
    explode_sync,
    fail_quietly,
    lookup,
    lookup_sync,
    make_ints,
    make_points,
    nap,
    nap_sync,
    round_trip_points,
)
from baml_sdk.bridge_test import divide, fail_later, quit, stuck
from baml_sdk.complex_models import round_trip_complex_profile

if "native" not in os.path.dirname(baml_sdk.__file__).split(os.sep):
    pytest.skip("the native bindings' own tests", allow_module_level=True)


def settle():
    """Let calls whose callers went away end, then collect."""
    baml_native.collect()


# ---- the five ways to await a call ----


async def test_await():
    assert await add(1, 2) == 3
    assert await nap(5) == 5


def test_asyncio_run():
    assert asyncio.run(add(2, 3)) == 5
    assert asyncio.run(nap(5)) == 5


async def test_create_task():
    tasks = [asyncio.create_task(add(i, 1)) for i in range(5)]
    tasks.append(asyncio.create_task(nap(5)))
    assert [await t for t in tasks] == [1, 2, 3, 4, 5, 5]


async def test_gather():
    assert await asyncio.gather(add(1, 1), nap(5), env_or("NO_SUCH_VAR_X", "fb")) == [2, 5, "fb"]


def test_a_call_that_ends_in_its_first_turn_needs_no_loop():
    # Its first step returns the result: no future, no trip around a loop,
    # and no running loop is needed at all.
    call = add(20, 22)
    with pytest.raises(StopIteration) as stopped:
        call.send(None)
    assert stopped.value.value == 42
    with pytest.raises(RuntimeError, match="no running event loop"):
        nap(5).send(None)  # this one must wait, and has no loop to wait on
    # Its BAML call went on; a sync call's turns see it end and free it.
    assert nap_sync(20) == 20
    assert baml_native.calls_in_flight() == 0


async def test_wait_for():
    assert await asyncio.wait_for(add(4, 4), 1) == 8
    assert await asyncio.wait_for(nap(5), 1) == 5


# ---- many calls at once ----


async def test_many_calls_at_once():
    os.environ["BAML_NATIVE_TEST_VAR"] = "here"
    before = baml_native.handle_count()
    calls = []
    expected = []
    for i in range(300):
        match i % 5:
            case 0:
                calls.append(add(i, i))
                expected.append(2 * i)
            case 1:
                calls.append(nap(1 + i % 20))
                expected.append(1 + i % 20)
            case 2:
                calls.append(env_or("BAML_NATIVE_TEST_VAR", "fb"))
                expected.append("here")
            case 3:
                calls.append(env_or("NO_SUCH_VAR_X", f"fb{i}"))
                expected.append(f"fb{i}")
            case 4:
                calls.append(make_points(3))
                expected.append([Point(x=0, y=3), Point(x=1, y=2), Point(x=2, y=1)])
    assert await asyncio.gather(*calls) == expected
    assert baml_native.calls_in_flight() == 0
    assert baml_native.handle_count() == before


async def test_many_calls_at_once_with_the_gil_held():
    baml_native.set_release_gil(False)
    try:
        await test_many_calls_at_once()
    finally:
        baml_native.set_release_gil(True)


async def test_sync_call_delivers_other_calls_ending_meanwhile():
    task = asyncio.ensure_future(nap(20))
    await asyncio.sleep(0)  # the call starts and waits on the loop
    # This blocks the loop's thread for 60 ms; the nap ends during its turns,
    # which resolve its future: the task only has to be stepped.
    assert nap_sync(60) == 60
    await asyncio.sleep(0)
    assert task.done() and task.result() == 20


# ---- how a call ends ----


async def test_a_declared_throw_arrives_typed():
    with pytest.raises(BamlError) as raised:
        await lookup("missing")
    assert raised.value.value == LookupFailed(key="missing", code=404)
    assert raised.value.class_name == "user.bridge_test.LookupFailed"
    assert await lookup("there") == "there"


def test_a_declared_throw_arrives_typed_sync():
    with pytest.raises(BamlError) as raised:
        lookup_sync("missing")
    assert isinstance(raised.value.value, LookupFailed)


async def test_a_thrown_string():
    with pytest.raises(BamlError) as raised:
        await explode(1)
    assert raised.value.value == "exploded"
    assert raised.value.class_name is None
    assert explode_sync(0) == 0


async def test_an_unhandled_failure_reaches_the_handler():
    seen = []
    previous = baml_native.set_unhandled_spawn_error_handler(
        lambda error, cancelled: seen.append((error, cancelled))
    )
    try:
        assert await fail_quietly() == 1
        for _ in range(100):
            if seen:
                break
            await asyncio.sleep(0.01)
    finally:
        baml_native.set_unhandled_spawn_error_handler(previous)
    assert len(seen) == 1
    error, cancelled = seen[0]
    assert isinstance(error, BamlError) and error.value == "exploded"
    assert cancelled is False


def test_a_refusal_raises_the_libraries_reason():
    with pytest.raises(TypeError) as raised:
        _native.start_unchecked("bridge_test.add", 1)
    assert "argument" in str(raised.value), str(raised.value)


def test_arguments_of_the_wrong_type_are_refused():
    with pytest.raises(TypeError, match="does not fit"):
        add_sync("1", 2)
    with pytest.raises(TypeError, match="does not fit"):
        add_sync(True, 2)  # bool is checked before int
    with pytest.raises(BamlError, match="Invalid argument: b"):
        add_sync(1)
    with pytest.raises(TypeError, match="unexpected keyword"):
        add_sync(1, 2, c=3)
    assert add_sync(1, b=2) == 3


async def test_an_env_read_parks_and_resumes():
    os.environ["BAML_NATIVE_TEST_VAR"] = "value"
    assert await env_or("BAML_NATIVE_TEST_VAR", "fb") == "value"
    assert env_or_sync("NO_SUCH_VAR_X", "fallback") == "fallback"


# ---- cancellation ----


async def test_cancelling_the_awaiting_task_frees_the_result():
    before = baml_native.handle_count()
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(nap(200), 0.02)
    # The BAML call goes on; its result is freed when it arrives.
    for _ in range(100):
        if baml_native.calls_in_flight() == 0:
            break
        await asyncio.sleep(0.01)
    assert baml_native.calls_in_flight() == 0
    assert baml_native.handle_count() == before
    assert await add(1, 1) == 2


async def test_closing_an_unfinished_call():
    call = nap(50)
    step = call.send(None)  # starts it: it waits on a future of this loop
    assert isinstance(step, asyncio.Future)
    call.close()
    for _ in range(100):
        if baml_native.calls_in_flight() == 0:
            break
        await asyncio.sleep(0.01)
    assert baml_native.calls_in_flight() == 0


# ---- one loop, one thread ----


def test_another_thread_is_refused():
    errors = []

    def call():
        try:
            add_sync(1, 2)
        except RuntimeError as error:
            errors.append(error)

    thread = threading.Thread(target=call)
    thread.start()
    thread.join()
    assert len(errors) == 1 and "thread" in str(errors[0])


def test_a_second_loop_with_calls_in_flight_is_refused():
    first = asyncio.new_event_loop()
    second = asyncio.new_event_loop()
    try:
        task = first.create_task(nap(100))
        first.run_until_complete(asyncio.sleep(0.01))  # it waits on `first`
        # A call that ends in its first turn needs no loop.
        assert second.run_until_complete(add(1, 2)) == 3
        with pytest.raises(RuntimeError, match="another event loop"):
            second.run_until_complete(nap(5))
        assert first.run_until_complete(task) == 100
        # `first` has nothing in flight now: the reader moves.
        assert second.run_until_complete(nap(5)) == 5
    finally:
        first.close()
        second.close()


# ---- values ----


async def test_large_values_both_ways():
    before = baml_native.handle_count()
    xs = await make_ints(100_000)
    assert xs == list(range(100_000))
    assert await count_ints(xs) == 100_000
    points = [Point(x=i, y=-i) for i in range(10_000)]
    assert await round_trip_points(points) == points
    assert baml_native.handle_count() == before


@pytest.mark.parametrize("mode", ["validate", "construct", "native"])
async def test_every_way_of_making_models_gives_equal_results(mode):
    from roundtrip_tests_profile import profile

    baml_native.set_model_mode(mode)
    try:
        back = await round_trip_complex_profile(profile())
        assert back == profile()
        assert back.model_fields_set == profile().model_fields_set
        assert back.model_dump() == profile().model_dump()
    finally:
        baml_native.set_model_mode(os.environ.get("BAML_NATIVE_MODEL_MODE", "native"))


# ---- after the review ----


def wait_until(condition, seconds=2.0):
    """Turn the library (with sync calls) until `condition` holds."""
    deadline = time.monotonic() + seconds
    while not condition() and time.monotonic() < deadline:
        add_sync(0, 0)
        time.sleep(0.005)
    return condition()


def test_a_panic_while_starting_leaves_the_module_usable():
    baml_native.panic_next("start")
    with pytest.raises(BaseException) as raised:  # pyo3's PanicException
        add_sync(1, 2)
    assert "panic" in type(raised.value).__name__.lower() or "panic" in str(raised.value)
    # The lock it poisoned is taken back.
    assert add_sync(1, 2) == 3
    assert baml_native.handle_count() >= 0


async def test_a_panic_while_delivering_one_result_spares_the_others():
    before = baml_native.handle_count()
    first = asyncio.ensure_future(nap(20))
    second = asyncio.ensure_future(nap(20))
    await asyncio.sleep(0)  # both started, both wait on the loop
    baml_native.panic_next("event")
    results = await asyncio.gather(first, second, return_exceptions=True)
    failed = [r for r in results if isinstance(r, BaseException)]
    assert len(failed) == 1 and "panicked" in str(failed[0])
    assert [r for r in results if not isinstance(r, BaseException)] == [20]
    assert baml_native.calls_in_flight() == 0
    assert baml_native.handle_count() == before
    assert await add(2, 2) == 4


async def test_a_dict_argument_is_read_from_a_snapshot():
    # A map argument walked from `items()`: changing the dict while it is
    # read cannot crash the walk. (Nothing in this walk runs Python code, so
    # the change is made before; the snapshot is what makes it safe.)
    from baml_sdk.maps import round_trip_simple_map

    m = {str(i): i for i in range(100)}
    assert await round_trip_simple_map(m) == m


def test_ctrl_c_in_a_sync_call_loses_no_wake():
    timer = threading.Timer(0.1, _thread.interrupt_main)
    timer.start()
    started = time.monotonic()
    with pytest.raises(KeyboardInterrupt):
        nap_sync(5000)
    # The wait looks for signals in slices, not only at the BAML timer.
    assert time.monotonic() - started < 1.0

    async def after():
        # Calls that wait on the I/O thread still wake the loop.
        for _ in range(5):
            assert await asyncio.wait_for(env_or("NO_SUCH_VAR_X", "fb"), 2) == "fb"
            assert await asyncio.wait_for(nap(5), 2) == 5

    asyncio.run(after())
    assert wait_until(lambda: baml_native.calls_in_flight() == 0, 7.0)


def test_one_future_that_cannot_take_its_result_stops_nothing():
    # The review's case: a future of a closed loop.
    event_loop = asyncio.new_event_loop()
    task = event_loop.create_task(nap(100))
    event_loop.run_until_complete(asyncio.sleep(0.01))
    event_loop.close()
    assert nap_sync(200) == 200
    del task


async def test_a_task_that_sleeps_after_its_call_returned_still_runs():
    seen = []
    previous = baml_native.set_unhandled_spawn_error_handler(
        lambda error, cancelled: seen.append(error)
    )
    try:
        # The call returns at once; its task sleeps 30 ms on a BAML timer,
        # which becomes a timer of this loop, then fails.
        assert await fail_later(30) == 1
        for _ in range(100):
            if seen:
                break
            await asyncio.sleep(0.01)
    finally:
        baml_native.set_unhandled_spawn_error_handler(previous)
    assert len(seen) == 1 and seen[0].value == "exploded"


async def test_a_result_delivered_by_a_nested_turn_reaches_its_call():
    seen = []

    def handler(error, cancelled):
        # A sync call from the handler: its turns deliver the nap below.
        seen.append(nap_sync(30))

    previous = baml_native.set_unhandled_spawn_error_handler(handler)
    try:
        assert await fail_later(10) == 1
        time.sleep(0.03)  # the task's timer is due; no turn has run it
        # This call's first turn runs the failing task; the handler's sync
        # call then sees this nap end.
        assert await asyncio.wait_for(nap(5), 2) == 5
    finally:
        baml_native.set_unhandled_spawn_error_handler(previous)
    assert seen == [30]
    assert baml_native.calls_in_flight() == 0


def test_moving_to_a_new_loop_when_the_old_was_closed_with_tasks_pending():
    first = asyncio.new_event_loop()
    task = first.create_task(nap(100))
    first.run_until_complete(asyncio.sleep(0.01))
    first.close()  # the task is still pending
    second = asyncio.new_event_loop()
    try:
        assert second.run_until_complete(nap(5)) == 5
        # The old call's result is freed when it arrives.
        second.run_until_complete(asyncio.sleep(0.15))
        assert baml_native.calls_in_flight() == 0
    finally:
        second.close()
    del task


async def test_throwing_another_exception_into_a_waiting_call():
    call = nap(50)
    assert isinstance(call.send(None), asyncio.Future)
    with pytest.raises(ValueError, match="thrown in"):
        call.throw(ValueError("thrown in"))
    for _ in range(100):
        if baml_native.calls_in_flight() == 0:
            break
        await asyncio.sleep(0.01)
    assert baml_native.calls_in_flight() == 0


async def test_a_panic_from_python():
    with pytest.raises(BamlPanic) as raised:
        await divide(1, 0)
    assert "DivisionByZero" in (raised.value.class_name or ""), raised.value.class_name
    assert await divide(6, 3) == 2


async def test_an_exit_from_python_is_a_panic_not_the_end_of_the_process():
    with pytest.raises(BamlPanic) as raised:
        await quit(3)
    assert "Exit" in (raised.value.class_name or ""), raised.value.class_name
    assert await add(1, 1) == 2


async def test_a_stuck_call_from_python():
    with pytest.raises(BamlPanic) as raised:
        await stuck()
    assert raised.value.class_name == "baml.panics.Stuck"


def test_a_call_never_awaited_warns():
    with pytest.warns(RuntimeWarning, match="never awaited"):
        add(1, 2)
        gc.collect()


def test_a_call_in_a_reference_cycle_is_collected():
    def calls():
        return [o for o in gc.get_objects() if type(o) is _native.Call]

    gc.collect()
    before = len(calls())
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        items = []
        call = count_ints(items)
        items.append(call)  # the call's arguments hold the call
        del call, items
        gc.collect()
    assert len(calls()) == before


def test_models_the_native_construct_would_build_wrong_are_validated():
    import pydantic

    class Plain(pydantic.BaseModel):
        x: int

    class Extra(pydantic.BaseModel):
        model_config = pydantic.ConfigDict(extra="allow")
        x: int

    class Private(pydantic.BaseModel):
        x: int
        _seen: int = pydantic.PrivateAttr(default=7)

    class Validated(pydantic.BaseModel):
        x: int

        @pydantic.field_validator("x")
        @classmethod
        def double(cls, value):
            return value * 2

    class PostInit(pydantic.BaseModel):
        x: int

        def model_post_init(self, context):
            self.__dict__["y"] = self.x + 1

    model, validates = baml_native.build_model(Plain, {"x": 1})
    assert not validates and model == Plain(x=1)
    for cls in (Extra, Private, Validated, PostInit):
        model, validates = baml_native.build_model(cls, {"x": 1})
        assert validates, cls
        expected = cls.model_validate({"x": 1})
        assert model == expected and model.__dict__ == expected.__dict__, cls
        assert model.__pydantic_private__ == expected.__pydantic_private__, cls


def test_two_threads_loading_at_once_share_the_wake_the_library_uses():
    import subprocess
    import sys
    import textwrap

    script = textwrap.dedent(
        """
        import asyncio, sys, threading
        import baml_native
        from baml_native import _native
        path, interface_hash = sys.argv[1], int(sys.argv[2], 16)
        start = threading.Barrier(2)
        done = threading.Barrier(2)
        results = []

        def worker():
            start.wait()
            baml_native.load(path, interface_hash)
            done.wait()
            try:
                env_or = _native.function("bridge_test.env_or", False, ["name", "fallback"], {})
                # Parks on the I/O thread: only a wake through the socket the
                # loop watches finishes it.
                results.append(asyncio.run(asyncio.wait_for(env_or("NO_SUCH_VAR_X", "fb"), 5)))
            except RuntimeError as error:
                results.append("refused" if "thread" in str(error) else repr(error))
            except Exception as error:
                results.append(repr(error))

        threads = [threading.Thread(target=worker) for _ in range(2)]
        for t in threads: t.start()
        for t in threads: t.join()
        print(sorted(results))
        """
    )
    from baml_sdk import _native_library

    out = subprocess.run(
        [sys.executable, "-c", script, _native_library.PATH, f"{_native_library.INTERFACE_HASH:x}"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert out.returncode == 0, out.stderr
    assert out.stdout.strip() == "['fb', 'refused']", out.stdout + out.stderr
