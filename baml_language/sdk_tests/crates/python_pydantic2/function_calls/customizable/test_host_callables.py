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
from baml_sdk.host_callable_tests import (
    Person,
    ValidationError,
    call_callback_with_optional_args_all_set,
    call_callback_with_optional_args_all_unset,
    call_callback_with_optional_args_partially_set,
    call_int_callback,
    call_repeatedly,
    call_with_callback,
    call_with_class_callback,
    call_with_throwing,
    call_with_two_args,
    call_with_typed_throws,
    call_with_typed_throws_propagating,
    make_adder,
    make_counter,
    make_pair_builder,
)


def test_host_callables_simple_sync_callable_returns_string():
    def cb(x: int) -> str:
        return f"got {x}"

    result = call_with_callback(callback=cb, x=5)
    assert result == "got 5"


def test_host_callables_two_arg_callable_unpacks_positional_args():
    def cb(x: int, prefix: str) -> str:
        return f"{prefix}:{x}"

    result = call_with_two_args(callback=cb, x=7, prefix="answer")
    assert result == "answer:7"


def test_host_callables_int_return_callable_round_trip():
    def cb(x: int) -> int:
        return x * 2

    result = call_int_callback(callback=cb, x=21)
    assert result == 42


# SDK_PARITY_LINT(skip): C# covers this canonical behavior in its native integration harness
def test_baml_closure_is_a_native_callable_with_host_language_arguments():
    add_ten = make_adder(offset=10)
    assert callable(add_ten)
    assert add_ten(5) == 15
    assert add_ten(value=7) == 17


# SDK_PARITY_LINT(skip): C# covers this canonical behavior in its native integration harness
def test_baml_closure_decodes_multiple_args_and_structured_return_values():
    build = make_pair_builder(base=30)
    assert build(12, "Ada") == Person(name="Ada", age=42)
    assert build(delta=5, label="Grace") == Person(name="Grace", age=35)


# SDK_PARITY_LINT(skip): C# covers this canonical behavior in its native integration harness
def test_baml_closure_is_reusable_and_retains_mutable_captures():
    next_value = make_counter(start=40)
    assert next_value() == 41
    assert next_value() == 42


def test_host_callables_throwing_callable_round_trips_original_python_exception():
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


def test_host_callables_throwing_callable_keyerror_round_trips_with_identity():
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


def test_host_callables_throwing_callable_custom_python_exception_round_trips_with_identity():
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


def test_host_callables_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml():
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


def test_host_callables_throwing_callable_bamlerror_propagates_back_with_typed_fields():
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


def test_host_callables_throwing_async_callable_round_trips_original_python_exception():
    """Async callables go through the same `run_if_coroutine` dispatch
    path; native exceptions raised inside the coroutine should round-trip
    by identity just like the sync case."""
    raised = ValueError("async nope")

    async def cb(_x: int) -> str:
        raise raised

    with pytest.raises(ValueError) as exc_info:
        call_with_callback(callback=cb, x=1)
    assert exc_info.value is raised


def test_host_callables_multiple_throws_in_flight_do_not_collide_in_registry():
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
def test_host_callables_release_fires_on_drop_of_callable():
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


def test_host_callables_lambda_round_trip():
    """Lambdas are callable and not pydantic models, so they hit the
    callable-encoding branch in `_set_inbound_value`.
    """
    result = call_with_callback(callback=lambda x: f"lambda-{x}", x=99)
    assert result == "lambda-99"


def test_host_callables_async_callable_runs_to_completion():
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


def test_host_callables_multiple_callable_keys_are_distinct():
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


def test_host_callables_class_callback_round_trips_pydantic_model():
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


def test_host_callables_call_repeatedly_invokes_callback_n_times():
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


def test_host_callables_call_repeatedly_with_zero_n_returns_empty_list():
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


def test_host_callables_call_with_throwing_in_baml_catches_host_callable_error():
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
# A host callable whose *own* type carries optional parameters
# (`(x: int, y?: int, z?: int) -> int`). Defaults aren't allowed inside a
# callable type — only the `?` optional marker — so the host's own
# language-level default is the only source of a value when BAML omits the arg.
# `y` and `z` cross the boundary by name, so each can be supplied or omitted
# independently; an omitted optional is dropped before dispatch and the host's
# default fills it. The callback returns `x*100 + y*10 + z` so each test can read
# off exactly which optionals were delivered.
# ---------------------------------------------------------------------------


def optional_args_cb(x: int, y: int = 8, z: int = 9) -> int:
    return x * 100 + y * 10 + z


def test_host_callables_optional_args_all_unset_apply_host_defaults():
    """`callback(x)` supplies neither optional. Both are dropped before dispatch,
    so the Python callback runs with only `x` and its own defaults fill `y`/`z`
    (8 and 9), yielding `5*100 + 8*10 + 9 = 589`."""
    assert call_callback_with_optional_args_all_unset(
        callback=optional_args_cb, x=5
    ) == [589]


def test_host_callables_optional_args_partially_set_deliver_by_name():
    """Two calls each supplying exactly one optional by name:
    `callback(x, y = 2)` (→ `500 + 20 + 9 = 529`) then `callback(x, z = 3)`
    (→ `500 + 80 + 3 = 583`). Optionals cross by name, so each supplied value is
    delivered as a keyword and the omitted one falls back to the host default
    (`y`→8, `z`→9) — including the case where the *leading* optional `y` is
    skipped while `z` is supplied."""
    assert call_callback_with_optional_args_partially_set(
        callback=optional_args_cb, x=5
    ) == [529, 583]


def test_host_callables_optional_args_all_set_deliver_both():
    """`callback(x, y = 2, z = 3)` supplies both optionals; both arrive by name
    and override the host defaults, yielding `500 + 20 + 3 = 523`."""
    assert call_callback_with_optional_args_all_set(
        callback=optional_args_cb, x=5
    ) == [523]
