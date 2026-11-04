"""Async calls resolve on the event loop that started them, whatever loops
come and go around them."""

import asyncio
import contextvars
import gc
import threading
import warnings
import weakref

import pytest

from baml_bridge import (
    BamlPanic,
    BamlRuntime,
    call_function,
    cancel_function_call,
    encode_call_args,
    new_function_call,
)

BAML = """\
function Echo(n: int) -> int {
    n
}

function EchoLater(n: int) -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
    n
}

function EchoMuchLater(n: int) -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    n
}
"""


@pytest.fixture(scope="module")
def rt():
    return BamlRuntime.initialize_runtime(".", {"main.baml": BAML})


async def echo_many(rt, count):
    # A call its loop never resolves would hang the test run.
    results = await asyncio.wait_for(
        asyncio.gather(*[call_function(rt, "Echo", {"n": n}) for n in range(count)]),
        timeout=10,
    )
    return [result.result() for result in results]


def test_calls_resolve_on_each_new_event_loop(rt):
    # Each asyncio.run is a new loop, often at the address of the one before.
    for _ in range(5):
        assert asyncio.run(echo_many(rt, 50)) == list(range(50))
        gc.collect()


def test_calls_resolve_on_event_loops_of_several_threads(rt):
    results = {}

    def run(thread):
        results[thread] = asyncio.run(echo_many(rt, 300))

    threads = [threading.Thread(target=run, args=(thread,)) for thread in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(timeout=30)
    assert results == {thread: list(range(300)) for thread in range(4)}


async def test_a_call_finishing_after_its_caller_was_cancelled_is_dropped(rt):
    task = asyncio.create_task(call_function(rt, "EchoLater", {"n": 1}))
    await asyncio.sleep(0.05)
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    # Past the cancelled call's sleep, in case it still finishes.
    await asyncio.sleep(0.3)
    assert (await call_function(rt, "Echo", {"n": 2})).result() == 2


def test_a_loop_closed_with_a_call_in_flight_does_not_affect_the_next_loop(rt):
    async def leave_a_call_running():
        asyncio.get_running_loop().create_task(call_function(rt, "EchoLater", {"n": 1}))
        await asyncio.sleep(0.05)

    asyncio.run(leave_a_call_running())
    assert asyncio.run(echo_many(rt, 20)) == list(range(20))


async def test_a_failed_call_resolves_only_its_own_future(rt):
    calls = [
        call_function(rt, "Echo", {"n": n}) if n % 2 else call_function(rt, "NoSuchFunction", {})
        for n in range(40)
    ]
    results = await asyncio.gather(*calls, return_exceptions=True)
    for n, result in enumerate(results):
        if n % 2:
            assert result.result() == n
        else:
            assert isinstance(result, BamlPanic)


class LoopWithoutAddReader(asyncio.SelectorEventLoop):
    def add_reader(self, fd, callback, *args):
        raise NotImplementedError


def test_a_loop_without_add_reader_still_resolves_calls(rt):
    event_loop = LoopWithoutAddReader()
    try:
        assert event_loop.run_until_complete(echo_many(rt, 50)) == list(range(50))
    finally:
        event_loop.close()


async def test_cancelling_the_native_future_stops_the_call(rt):
    call_id = new_function_call()
    args = encode_call_args({"n": 1}, call_id, function_name="EchoMuchLater")
    future = rt.call_function(args, args.kwargs, args.cancel)
    await asyncio.sleep(0.05)
    future.cancel()
    await asyncio.sleep(0.5)
    # A call that ended has no allocation left to cancel.
    assert not cancel_function_call(call_id)


def test_a_loop_dropped_without_close_is_collected(rt):
    async def leave_a_call_running():
        asyncio.get_running_loop().create_task(call_function(rt, "EchoLater", {"n": 1}))
        await asyncio.sleep(0.01)

    event_loop = asyncio.new_event_loop()
    event_loop.run_until_complete(leave_a_call_running())
    dropped = weakref.ref(event_loop)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        del event_loop
        # The call finishes into the dropped loop's queue.
        asyncio.run(asyncio.sleep(0.4))
        # Python objects the call released off the GIL are let go on the
        # bridge's next entry.
        asyncio.run(echo_many(rt, 1))
        gc.collect()
    assert dropped() is None


def test_the_first_callers_context_does_not_outlive_its_call(rt):
    request = contextvars.ContextVar("request")

    class Payload:
        pass

    async def first_call(payload):
        request.set(payload)
        await call_function(rt, "Echo", {"n": 1})

    async def main():
        payload = Payload()
        released = weakref.ref(payload)
        # A task runs in a copy of the context, which ends with it.
        await asyncio.get_running_loop().create_task(first_call(payload))
        del payload
        # Python objects the call released off the GIL are let go on the
        # bridge's next entry.
        await call_function(rt, "Echo", {"n": 2})
        gc.collect()
        return released() is None

    assert asyncio.run(main())
