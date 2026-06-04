"""End-to-end tests for the host-callable round trip.

The BAML fixture declares three functions whose first parameter is a
typed `Callable`. The Python test passes a normal Python callable; the
bridge auto-registers it via `register_host_callable` and emits the
appropriate `Handle{HOST_VALUE_CALLABLE}` wire entry; the engine binds
it to an `Object::HostClosure`; when BAML invokes it the
`call_host_value` sysop fires the dispatch FFI; the Python dispatch
callback in `bridge_python::host_value` invokes the user function and
encodes the result back to the engine.
"""

from __future__ import annotations

import gc
import weakref

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.host_callable_tests import (
    Person,
    call_int_callback,
    call_repeatedly,
    call_with_callback,
    call_with_class_callback,
    call_with_throwing,
    call_with_two_args,
)


def test_simple_sync_callable_returns_string():
    def cb(x: int) -> str:
        return f"got {x}"

    result = call_with_callback(callback=cb, x=5)
    assert result == "got 5"


def test_two_arg_callable_unpacks_positional_args():
    def cb(x: int, prefix: str) -> str:
        return f"{prefix}:{x}"

    result = call_with_two_args(callback=cb, x=7, prefix="answer")
    assert result == "answer:7"


def test_int_return_callable_round_trip():
    def cb(x: int) -> int:
        return x * 2

    result = call_int_callback(callback=cb, x=21)
    assert result == 42


def test_throwing_callable_surfaces_as_baml_error():
    def cb(_x: int) -> str:
        raise ValueError("nope")

    with pytest.raises(Exception) as exc_info:
        call_with_callback(callback=cb, x=1)
    # The host-callable error round-trips the class name and message
    # via `HostCallableError` proto → `OpErrorKind::HostCallable` →
    # `root.errors.HostCallable` throw → Python exception.
    msg = str(exc_info.value)
    assert "nope" in msg or "ValueError" in msg


@pytest.mark.xfail(
    reason="host-callable release fires only when the engine GCs the "
    "Object::HostClosure on its heap; one BAML call rarely triggers "
    "the GC heuristic, so for now the callable leaks until the engine "
    "collects.",
    strict=False,
)
def test_release_fires_on_drop_of_callable():
    """After BAML finishes invoking the callable and the engine GCs the
    `Object::HostClosure` it allocated, the registered release callback
    removes the Python callable from the bridge's host-value table.
    Dropping the user's last reference then leaves the object
    unreachable for the cycle collector.
    """

    class CallableObj:
        def __call__(self, x: int) -> str:
            return str(x)

    cb = CallableObj()
    wr = weakref.ref(cb)
    result = call_with_callback(callback=cb, x=3)
    assert result == "3"
    del cb
    gc.collect()
    assert wr() is None, "host callable should be released after BAML drops it"


def test_lambda_round_trip():
    """Lambdas are callable and not pydantic models, so they hit the
    callable-encoding branch in `_set_inbound_value`.
    """
    result = call_with_callback(callback=lambda x: f"lambda-{x}", x=99)
    assert result == "lambda-99"


def test_async_callable_runs_to_completion():
    """Async callables are detected (via `asyncio.iscoroutine` on the
    return value) and run to completion on a fresh asyncio loop inside
    the dispatch thread."""

    async def cb(x: int) -> str:
        # Minimal awaitable body — exercises the coroutine path.
        import asyncio
        await asyncio.sleep(0)
        return f"async-{x}"

    result = call_with_callback(callback=cb, x=4)
    assert result == "async-4"


def test_multiple_callable_keys_are_distinct():
    """Two separately-registered callables must produce two distinct
    keys; invoking one must not call the other."""

    counter = {"a": 0, "b": 0}

    def cb_a(x: int) -> str:
        counter["a"] += 1
        return f"a:{x}"

    def cb_b(x: int) -> str:
        counter["b"] += 1
        return f"b:{x}"

    assert call_with_callback(callback=cb_a, x=1) == "a:1"
    assert call_with_callback(callback=cb_b, x=2) == "b:2"
    assert counter == {"a": 1, "b": 1}


def test_class_callback_round_trips_pydantic_model():
    """A user-defined `Person` class round-trips through the callable
    boundary: BAML encodes the `Person` for the engine→host call; the
    Python dispatcher decodes it into the codegen-emitted pydantic
    model; the user callback receives a `Person` instance.
    """

    def cb(p: Person) -> str:
        return f"{p.name} is {p.age}"

    person = Person(name="Ada", age=37)
    result = call_with_class_callback(callback=cb, p=person)
    assert result == "Ada is 37"


def test_call_repeatedly_invokes_callback_n_times():
    """Exercises N round-trips through `SysOp::BamlHostCallHostValue`:
    BAML's `for` loop invokes the callable for each iteration; the
    result list collects every callback return value.
    """

    invocations: list[int] = []

    def cb(x: int) -> str:
        invocations.append(x)
        return f"item-{x}"

    results = call_repeatedly(callback=cb, n=5)
    assert results == [f"item-{i}" for i in range(5)]
    assert invocations == list(range(5))


def test_call_repeatedly_with_zero_n_returns_empty_list():
    """N == 0 should produce no callback invocations and an empty
    result list — covers the loop's zero-iteration edge case.
    """

    invocations: list[int] = []

    def cb(x: int) -> str:
        invocations.append(x)
        return ""

    results = call_repeatedly(callback=cb, n=0)
    assert results == []
    assert invocations == []


def test_call_with_throwing_surfaces_declared_host_callable_error():
    """The BAML catch fixture currently surfaces the host-callable error.

    Root-thread `BamlHostCallHostValue` errors leave the VM as an unhandled
    `baml.errors.HostCallable` before the BAML `catch` expression can intercept
    them. Keep that current behavior covered without hiding it behind xfail.
    """

    def cb(_x: int) -> str:
        raise RuntimeError("boom from host")

    with pytest.raises(Exception) as exc_info:
        call_with_throwing(callback=cb, x=1)

    msg = str(exc_info.value)
    assert "baml.errors.HostCallable" in msg
    assert "RuntimeError" in msg or "boom from host" in msg
