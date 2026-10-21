"""Real bridge executions; run with the generated function_calls SDK on PYTHONPATH."""

import asyncio

from baml_bridge import shutdown_runtime
from baml_sdk import execution_context_tests as context_baml
from baml_sdk import host_callable_tests as callbacks
from baml_sdk import trace


def options(case):
    return trace.span(inputs=True, output=True, error=True).context(
        distinct_id="audit-python", metadata={"audit": "python", "case": case}
    )


@trace.instrument(options("sync"), name="audit custom sync")
def ordinary(value, *, flag=False, _baml=None):
    assert _baml == "application-data"
    assert context_baml.current_context().metadata["case"] == "sync"
    return {"value": value + 1, "flag": flag}


class Service:
    def __init__(self, offset):
        self.offset = offset

    @trace.instrument(options("method"), name="audit method")
    def method(self, value):
        return self.offset + value

    @classmethod
    @trace.instrument(options("classmethod"), name="audit classmethod")
    def make(cls, value):
        assert cls is Service
        return value + 2

    @staticmethod
    @trace.instrument(options("staticmethod"), name="audit staticmethod")
    def static(value):
        return value + 3


@trace.instrument(options("error"), name="audit error")
def fail():
    raise failure


failure = ValueError("audit application failure")
assert ordinary(7, flag=True, _baml="application-data") == {"value": 8, "flag": True}
service = Service(10)
assert service.method(7) == 17
assert Service.make(7) == 9
assert Service.static(7) == 10
try:
    fail()
except ValueError as caught:
    assert caught is failure
else:
    raise AssertionError("application failure disappeared")


def generator():
    yield 1


async def async_generator():
    yield 1


for unsupported in (generator, async_generator, service.method):
    for decorate in (trace.instrument, trace.instrument(options("unsupported"))):
        try:
            decorate(unsupported)
        except trace.TraceUsageError:
            pass
        else:
            raise AssertionError("unsupported shape was accepted")


@trace.instrument(options("iterator"), name="audit iterator")
def return_iterator():
    return iterator


iterator = iter([1, 2])
assert return_iterator() is iterator
assert list(iterator) == [1, 2]


async def main():
    @trace.instrument(options("async"), name="audit custom async")
    async def asynchronous(value):
        await asyncio.sleep(0)
        assert (await context_baml.current_context_async()).metadata["case"] == "async"
        return value + 1

    assert await asynchronous(8) == 9

    @trace.instrument(options("async_error"), name="audit async error")
    async def async_fail():
        await asyncio.sleep(0)
        raise failure

    try:
        await async_fail()
    except ValueError as caught:
        assert caught is failure
    else:
        raise AssertionError("async failure disappeared")

    ready = [asyncio.Event(), asyncio.Event()]
    release = asyncio.Event()

    @trace.instrument(
        options("marker").context(metadata={"keep": 1, "drop": 2}),
        name="audit reusable callback",
    )
    async def callback(value):
        if value < 2:
            ready[value].set()
            await release.wait()
        current = trace.current_context()
        if value < 2:
            assert current.distinct_id == f"request-{value}"
            assert current.metadata == {
                "audit": "python",
                "case": f"call-{value}",
                "keep": 1,
                "caller": True,
            }
        else:
            assert current.distinct_id == "audit-python"
            assert current.metadata == {
                "audit": "python",
                "case": "marker",
                "keep": 1,
                "drop": 2,
            }
        assert (await context_baml.current_context_async()).metadata == current.metadata
        return value + 10

    async def request(value):
        return await callbacks.call_configured_callback_async(
            callback,
            value,
            options(f"call-{value}").context(
                distinct_id=f"request-{value}", metadata={"drop": None}
            ),
            _baml={"trace": options("caller").context(metadata={"caller": True})},
        )

    pending = [asyncio.create_task(request(value)) for value in range(2)]
    try:
        await asyncio.wait_for(asyncio.gather(*(event.wait() for event in ready)), 5)
    finally:
        release.set()
    assert await asyncio.gather(*pending) == [10, 11]
    assert await callback(2) == 12
    assert trace.current_context().metadata == {}
    assert trace.current_cancel_token() is None


asyncio.run(main())
shutdown_runtime()
print("python host instrumentation cases passed")
