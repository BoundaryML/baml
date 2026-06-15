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
from baml_sdk.baml import BamlError
from baml_sdk.baml.errors import AccessError
from baml_sdk.host_callable_tests import (
    Person,
    ValidationError,
    call_int_callback,
    call_optional_callback_omitted,
    call_optional_callback_supplied,
    call_optional_int_callback_supplied,
    call_repeatedly,
    call_with_callback,
    call_with_class_callback,
    call_with_throwing,
    call_with_two_args,
    call_with_typed_throws,
    call_with_typed_throws_propagating,
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


def test_throwing_callable_round_trips_original_python_exception():
    """A native Python exception raised inside a host callable surfaces
    back to the caller as the *same* exception object (`raised is caught`
    identity), not flattened into a `BamlError(HostCallable(...))`
    wrapper. The bridge registers the exception in its host-value
    registry on the inbound throw, BAML transports the
    `baml.errors.HostCallable` Instance with the handle in `_handle`,
    and the outbound decoder looks the handle up to re-raise the
    original."""
    raised = ValueError("nope")

    def cb(_x: int) -> str:
        raise raised

    with pytest.raises(ValueError) as exc_info:
        call_with_callback(callback=cb, x=1)
    assert exc_info.value is raised
    assert str(exc_info.value) == "nope"


def test_throwing_callable_keyerror_round_trips_with_identity():
    """The native-exception rehydration path is class-agnostic: any
    `BaseException` subclass round-trips by reference, not just
    `ValueError`. Different exception classes should not collide in the
    bridge's host-value registry."""
    raised = KeyError("missing")

    def cb(_x: int) -> str:
        raise raised

    with pytest.raises(KeyError) as exc_info:
        call_with_callback(callback=cb, x=1)
    assert exc_info.value is raised


def test_throwing_callable_custom_python_exception_round_trips_with_identity():
    """A user-defined Python exception subclass also round-trips by
    identity — the bridge doesn't care about the concrete type."""

    class MyDomainError(Exception):
        def __init__(self, message: str, code: int) -> None:
            super().__init__(message)
            self.code = code

    raised = MyDomainError("custom domain failure", code=42)

    def cb(_x: int) -> str:
        raise raised

    with pytest.raises(MyDomainError) as exc_info:
        call_with_callback(callback=cb, x=1)
    assert exc_info.value is raised
    assert exc_info.value.code == 42


def test_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml():
    """When the host raises `BamlError(value=<codegenned BAML class>)`,
    the bridge unwraps the inner pydantic model and emits it as that
    real BAML class on the wire — not as an opaque `HostCallable` with
    a stringified `class_name`. The BAML side's typed `catch
    (e: ValidationError) { ... }` matches structurally and reads
    `e.code` / `e.message` / `e.fields` as real fields, demonstrating
    the typed-error contract holds end-to-end across the host boundary."""

    def cb(_x: int) -> str:
        raise BamlError(
            ValidationError(
                code=4,
                message="bad shape",
                fields=["name", "age", "email", "phone"],
            ),
        )

    result = call_with_typed_throws(callback=cb, x=1)
    assert result == "caught: bad shape"


def test_throwing_callable_bamlerror_propagates_back_with_typed_fields():
    """The same `BamlError(ValidationError(...))` raised by the host,
    when *not* caught in BAML, propagates back out to Python with all
    typed fields preserved on the pydantic instance — the engine
    transports it as a real `ValidationError` class instance both
    directions, never collapsing to a stringified HostCallable
    metadata blob."""
    raised = BamlError(
        ValidationError(
            code=7,
            message="propagated through",
            fields=["x", "y"],
        ),
    )

    def cb(_x: int) -> str:
        raise raised

    with pytest.raises(BamlError) as exc_info:
        call_with_typed_throws_propagating(callback=cb, x=1)
    decoded = exc_info.value.value
    assert isinstance(decoded, ValidationError)
    assert decoded.code == 7
    assert decoded.message == "propagated through"
    assert decoded.fields == ["x", "y"]


def test_throwing_async_callable_round_trips_original_python_exception():
    """Async callables go through the same `run_if_coroutine` dispatch
    path; native exceptions raised inside the coroutine should round-trip
    by identity just like the sync case."""
    raised = ValueError("async nope")

    async def cb(_x: int) -> str:
        raise raised

    with pytest.raises(ValueError) as exc_info:
        call_with_callback(callback=cb, x=1)
    assert exc_info.value is raised


def test_multiple_throws_in_flight_do_not_collide_in_registry():
    """Each host throw mints a fresh host-value key; calls in quick
    succession must not see the wrong original exception. Exercises the
    `next_key()` minting + per-call cleanup invariant of the
    host-value registry."""
    raised_first = ValueError("first")
    raised_second = ValueError("second")

    def cb_first(_x: int) -> str:
        raise raised_first

    def cb_second(_x: int) -> str:
        raise raised_second

    with pytest.raises(ValueError) as ei1:
        call_with_callback(callback=cb_first, x=1)
    with pytest.raises(ValueError) as ei2:
        call_with_callback(callback=cb_second, x=2)
    assert ei1.value is raised_first
    assert ei2.value is raised_second
    assert ei1.value is not ei2.value


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


def test_call_with_throwing_in_baml_catches_host_callable_error():
    """The BAML `catch (e)` clause around a host-callable invocation now
    intercepts a host-thrown `baml.errors.HostCallable` and returns the
    recovery branch.

    The fixture (`call_with_throwing`) declares the callback's throws
    contract as `baml.errors.HostCallable` and wraps the call site in
    ``catch (e) { _ => "caught:" + e.class_name }``. The host raises
    `RuntimeError`; the bridge wraps it as a `HostCallable` Instance with
    `class_name="RuntimeError"`; the engine's throws-contract check
    accepts it (since the declared `E` is `HostCallable`); the throw is
    injected through the VM's unwinder and caught by the BAML `catch`;
    the recovery expression returns the string. Earlier in the branch
    this surfaced as an unhandled Python exception — the engine now
    threads sysop throws through the same unwinder a `throw` opcode uses,
    so an in-BAML `catch` can intercept them like any other throw.
    """

    def cb(_x: int) -> str:
        raise RuntimeError("boom from host")

    result = call_with_throwing(callback=cb, x=1)
    assert result == "caught:RuntimeError"


# ---------------------------------------------------------------------------
# Optional args × host callables (the combination).
#
# Optional args (test_optional_args.py) and host callables (the cases above)
# were each covered in isolation but never together: a host callable whose
# *own* type carries an optional parameter (`(x: int, y?: int) -> ...`).
# Defaults aren't allowed inside a callable type — only the `?` optional
# marker — so the host's own language-level default is the only possible
# source of a value when BAML omits the arg.
# ---------------------------------------------------------------------------


def test_optional_arg_callable_supplied_passes_value_positionally():
    """When BAML supplies the callable's optional arg (`callback(x, y = 9)`),
    the engine resolves the named arg into the positional arg list, so the
    Python callback receives both values positionally — overriding its own
    default. Exercises the optional-args surface across the host boundary."""
    seen: list[tuple[int, int]] = []

    def cb(x: int, y: int = -1) -> str:
        seen.append((x, y))
        return f"{x}:{y}"

    result = call_optional_callback_supplied(callback=cb, x=5)
    assert result == "5:9"
    assert seen == [(5, 9)]


def test_optional_arg_callable_supplied_int_return():
    """Same supplied-optional path with a non-string (`int`) return value —
    covers the result-encode branch for an optional-arg host callable."""

    def cb(x: int, y: int = 7) -> int:
        return x * 10 + y

    assert call_optional_int_callback_supplied(callback=cb, x=5) == 150


def test_optional_arg_callable_omitted_raises_access_error():
    """Omitting the callable's optional arg at the BAML call site
    (`callback(x)`) currently fails. Unlike a BAML→BAML call — where the
    engine resolves an omitted optional natively — the omitted-arg sentinel
    cannot be converted to an owned value for the host wire, so the engine
    raises `baml.errors.AccessError` *before* the callback ever runs. Pins
    the current limitation: a host callable's optional arg cannot be omitted,
    so the host's own default never gets a chance to apply."""
    invoked: list[tuple[int, int]] = []

    def cb(x: int, y: int = 0) -> str:
        invoked.append((x, y))
        return f"{x}:{y}"

    with pytest.raises(BamlError) as exc_info:
        call_optional_callback_omitted(callback=cb, x=5)
    assert isinstance(exc_info.value.value, AccessError)
    assert invoked == []  # the engine fails before dispatching to the host
