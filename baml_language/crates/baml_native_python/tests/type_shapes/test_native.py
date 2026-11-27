"""The native bridge's own tests: what today's SDK tests do not cover.

Many calls at once, calls that sleep or read the environment, the ways a
call ends, refusals, cancellation, the five ways an async call is awaited,
one loop and one thread at a time, and every handle freed. Run against the
native bindings only (PYTHONPATH=.../native).
"""

import asyncio
import os
import threading

import pytest

import baml_sdk
import baml_native
from baml_native import _native
from baml_bridge.errors import BamlError

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


async def test_many_calls_at_once_with_the_gil_released():
    baml_native.set_release_gil(True)
    try:
        await test_many_calls_at_once()
    finally:
        baml_native.set_release_gil(False)


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
    with pytest.raises(TypeError, match="missing required argument"):
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
        baml_native.set_model_mode("native")
