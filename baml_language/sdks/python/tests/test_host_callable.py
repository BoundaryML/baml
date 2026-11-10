"""Tests for the Python host-callable bridge.

These tests exercise the auto-registration encoder branch and the
dispatch round trip without needing a generated SDK — the test builds an
ad-hoc BAML runtime that declares one function whose first parameter is
a `(int) -> string` callable. A callable takes its type from the program
descriptor alone, so the tests install the descriptor of that program
(`crates/bridge_encode/fixtures/host_callable_program.descriptor`, kept
current by a `bridge_encode` test) as the typemap's for their duration.

Run with:
    cd baml_language/sdks/python
    uv run maturin develop --uv
    uv run pytest tests/test_host_callable.py -v
"""

from __future__ import annotations

import gc
import weakref
from pathlib import Path

import pytest

import baml_bridge.proto as _proto

from baml_bridge import (
    BamlClassInstance,
    BamlPyHandle,
    BamlRuntime,
    BamlTypeMap,
    call_function_sync,
    call_function,
    get_type_map,
    set_type_map,
)
from baml_bridge.baml_py import (
    _live_handle_count,
    _seed_media_handle,
)
from baml_bridge.errors import BamlError, BamlPanic


CALLBACK_BAML = """\
function CallCb(callback: (int) -> string, x: int) -> string {
    callback(x)
}

function CallIntCb(callback: (int) -> int, x: int) -> int {
    callback(x)
}

function ConsumeUnknownCb(callback: (int) -> unknown, x: int) -> int {
    let _ = callback(x);
    x
}

function Collect() -> int {
    baml.sys.collect_garbage();
    0
}

function UnionCbs(callbacks: ((int) -> int throws never)[] | string[]) -> int {
    match (callbacks) {
        let cbs: ((int) -> int throws never)[] => cbs[0](1),
        _ => 0,
    }
}

function AmbiguousCbs(callbacks: ((int) -> int throws never)[] | ((int) -> string throws never)[]) -> int {
    0
}

function TakeUnknown(value: unknown) -> int {
    0
}
"""


_DESCRIPTOR = (
    Path(__file__).resolve().parents[3]
    / "crates"
    / "bridge_encode"
    / "fixtures"
    / "host_callable_program.descriptor"
).read_bytes().strip()


@pytest.fixture(autouse=True, scope="module")
def _program_descriptor():
    previous = get_type_map()
    set_type_map(BamlTypeMap.from_lazy_entries({}, {}, {}, descriptor=_DESCRIPTOR))
    yield
    set_type_map(previous)


def _make_runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": CALLBACK_BAML})


def test_sync_callable_round_trip():
    rt = _make_runtime()

    def cb(x: int) -> str:
        return f"got {x}"

    result = call_function_sync(rt, "CallCb", {"callback": cb, "x": 5})
    assert result.result() == "got 5"


def test_sync_int_callable_round_trip():
    rt = _make_runtime()

    def cb(x: int) -> int:
        return x + 1

    result = call_function_sync(rt, "CallIntCb", {"callback": cb, "x": 41})
    assert result.result() == 42


def test_throwing_callable_surfaces_as_exception():
    rt = _make_runtime()

    def cb(_x: int) -> str:
        raise ValueError("oops")

    with pytest.raises(Exception) as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})
    msg = str(exc_info.value)
    assert "oops" in msg or "ValueError" in msg


def test_lambda_round_trip():
    rt = _make_runtime()

    result = call_function_sync(
        rt, "CallCb", {"callback": lambda x: f"lambda-{x}", "x": 12}
    )
    assert result.result() == "lambda-12"


@pytest.mark.xfail(
    reason="host-callable release fires only when the engine GCs the "
    "Object::HostClosure on its heap; one BAML call rarely triggers "
    "the GC heuristic. v1 leaks until the engine collects.",
    strict=False,
)
def test_release_fires_on_drop():
    """The Rust side drops its `HostValueArc` once the engine GCs the
    `Object::HostClosure` it allocated for the callable; the release
    callback then removes the Python callable from the registry, and
    dropping the user's last reference makes it collectible.

    The callable sits on the engine's heap and only goes away when
    GC walks it. Driving extra BAML calls *eventually* triggers GC,
    but a single one usually doesn't — hence xfail-strict-false. The
    release path itself is exercised directly by the `bex_external_types`
    unit tests in `host_value::tests::drop_fires_release_once_at_last_clone`.
    """
    rt = _make_runtime()

    class CallableObj:
        def __call__(self, x: int) -> str:
            return str(x)

    cb = CallableObj()
    wr = weakref.ref(cb)
    result = call_function_sync(rt, "CallCb", {"callback": cb, "x": 3})
    assert result.result() == "3"
    del cb
    # Drive enough calls to nudge the engine's GC heuristic. Even with
    # this, a single-callable program may stay below the threshold.
    for _ in range(64):
        _ = call_function_sync(rt, "CallCb", {"callback": lambda _x: "", "x": 0})
    gc.collect()
    assert wr() is None, (
        "expected the host callable to be released after BAML drops its HostClosure"
    )


def test_multiple_callables_have_distinct_keys():
    rt = _make_runtime()
    seen = {"a": 0, "b": 0}

    def cb_a(x: int) -> str:
        seen["a"] += 1
        return f"a:{x}"

    def cb_b(x: int) -> str:
        seen["b"] += 1
        return f"b:{x}"

    assert (
        call_function_sync(rt, "CallCb", {"callback": cb_a, "x": 1}).result() == "a:1"
    )
    assert (
        call_function_sync(rt, "CallCb", {"callback": cb_b, "x": 2}).result() == "b:2"
    )
    assert seen == {"a": 1, "b": 1}


def test_async_callable_runs_to_completion():
    """Async callables are detected via `asyncio.iscoroutine` on the
    return and driven to completion on a fresh asyncio loop in the
    dispatch thread.
    """
    rt = _make_runtime()

    async def cb(x: int) -> str:
        import asyncio

        await asyncio.sleep(0)
        return f"async-{x}"

    result = call_function_sync(rt, "CallCb", {"callback": cb, "x": 4})
    assert result.result() == "async-4"


@pytest.mark.asyncio
async def test_async_outer_call_with_sync_callback():
    rt = _make_runtime()

    def cb(x: int) -> str:
        return f"x{x}"

    result = await call_function(rt, "CallCb", {"callback": cb, "x": 9})
    assert result.result() == "x9"


# ---------------------------------------------------------------------------
# Abnormal paths must still complete the call (engine never hangs).
# ---------------------------------------------------------------------------


def test_callable_returning_unencodable_breaks_the_contract():
    """A callback whose *result* is not a member of the return type it was
    bound to broke its contract: the call completes as a
    `baml.panics.HostContractViolation`, never hangs. `object()` has no BAML
    type, so it is not a `string`."""
    rt = _make_runtime()

    def cb(_x: int):
        return object()  # fits no declared type the bridge can encode

    with pytest.raises(BamlPanic, match="HostContractViolation") as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})
    assert "expected string" in str(exc_info.value)


def test_callable_raising_while_its_result_is_read_raises_that_error():
    """A callback that returns an object which raises while the bridge reads
    it raised, as far as BAML is concerned: the caller gets that exception,
    not a contract violation, and the call completes."""
    rt = _make_runtime()
    raised = RuntimeError("hostile iter")

    class Hostile(list):
        def __iter__(self):
            raise raised

    def cb(_x: int):
        return Hostile()

    with pytest.raises(Exception) as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})
    assert exc_info.value is raised


def test_host_result_successful_encode_transfers_capability_clone_to_engine():
    """The handle itself encodes successfully, so the engine receives and
    drains its cloned key before rejecting this image handle against
    the `string` that the callback declares. The original Python
    handle remains live."""
    rt = _make_runtime()
    key, handle_type = _seed_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    with pytest.raises(BamlPanic, match="string"):
        call_function_sync(
            rt,
            "CallCb",
            {"callback": lambda _x: handle, "x": 7},
        )

    assert _live_handle_count() == before


def test_host_result_encode_failure_releases_capability_clone():
    rt = _make_runtime()
    key, handle_type = _seed_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        return [handle, object()]

    # The list is refused as a whole before anything is cloned for the wire.
    with pytest.raises(BamlPanic, match="no BAML type"):
        call_function_sync(rt, "ConsumeUnknownCb", {"callback": cb, "x": 1})

    assert _live_handle_count() == before


def test_a_panic_a_callback_raises_stays_that_panic_whatever_it_declares():
    """A `baml.panics.*` value a callback raises is a panic whatever the
    callable declares: the `throws never` callables of `UnionCbs` cannot throw
    an error, but a panic is not an error, and the caller sees that panic,
    never a `HostContractViolation` for a value outside `never`."""
    rt = _make_runtime()
    panic = BamlPanic(
        BamlClassInstance(
            class_name="baml.panics.UserPanic", fields={"message": "tripped"}
        ),
        class_name="baml.panics.UserPanic",
    )

    def cb(_x: int) -> int:
        raise panic

    with pytest.raises(BamlPanic) as exc_info:
        call_function_sync(rt, "UnionCbs", {"callbacks": [cb]})
    message = str(exc_info.value)
    assert "UserPanic" in message and "tripped" in message, message
    assert "HostContractViolation" not in message, message


def test_host_throw_encode_failure_releases_capability_clone():
    rt = _make_runtime()
    # The runtime and the handle table are process-global. Creating this
    # runtime dropped the previous test's engine heap, but an exception a
    # previous test raised still lingers in the synchronous call plumbing
    # until the next call, and in a traceback cycle until the collector runs;
    # settle both so the baseline counts this test's handles alone.
    call_function_sync(rt, "Collect", {})
    gc.collect()
    key, handle_type = _seed_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        raise BamlError([handle, object()])

    # The thrown value cannot be encoded against the declared throws type, so
    # the callable broke its contract: a `HostContractViolation` panic, and
    # the capability clone the encoder took for `handle` is released on the
    # way out rather than leaking with the refused value.
    with pytest.raises(BamlPanic, match="HostContractViolation"):
        call_function_sync(rt, "ConsumeUnknownCb", {"callback": cb, "x": 1})

    # The exception's traceback keeps the dispatch frame (and its control
    # handles) alive until the collector runs.
    call_function_sync(rt, "Collect", {})
    gc.collect()
    assert _live_handle_count() == before


# ---------------------------------------------------------------------------
# A union is decided before anything is registered.
# ---------------------------------------------------------------------------


def test_union_members_register_only_what_ships():
    """`((int) -> int)[] | string[]` with `[cb]`: the function member takes
    the list, the other does not, and exactly one registration per call is
    made, which is the engine's until it collects the call. A union both of
    whose members would take the callable is ambiguous, and registers
    nothing."""
    from baml_bridge.baml_py import _host_value_registry_len

    rt = _make_runtime()
    call_function_sync(rt, "Collect", {})
    gc.collect()
    before = _host_value_registry_len()

    def cb(x: int) -> int:
        return x

    for _ in range(5):
        # The function member takes the list and the callback runs: `cb(1)`.
        assert call_function_sync(rt, "UnionCbs", {"callbacks": [cb]}).result() == 1
    # Only the shipped registrations may remain, one per call.
    assert _host_value_registry_len() == before + 5
    with pytest.raises(TypeError, match="fits more than one"):
        call_function_sync(rt, "AmbiguousCbs", {"callbacks": [cb]})
    assert _host_value_registry_len() == before + 5
    call_function_sync(rt, "Collect", {})
    gc.collect()
    assert _host_value_registry_len() == before


def test_leaves_nest_under_an_open_slot():
    """An image inside a list or dict under `unknown` crosses as itself."""
    from baml_bridge.proto import BamlImage

    rt = _make_runtime()
    image = BamlImage.from_url("https://example.com/a.png", None)
    assert call_function_sync(rt, "TakeUnknown", {"value": [image]}).result() == 0
    assert call_function_sync(rt, "TakeUnknown", {"value": {"k": image}}).result() == 0


def test_a_value_nested_deeper_than_the_wire_carries_is_refused_at_once():
    rt = _make_runtime()
    value: object = 1
    for _ in range(200):
        value = [value]
    with pytest.raises(TypeError, match="nested too deeply"):
        call_function_sync(rt, "TakeUnknown", {"value": value})


# ---------------------------------------------------------------------------
# Encode-error rollback releases callables registered for earlier kwargs.
# ---------------------------------------------------------------------------


def test_encode_error_releases_registered_callables():
    """If a later argument fails to encode, every callable registered for an
    earlier argument must be released again, so it does not leak in the
    per-process registry. The call never reaches the engine."""
    from baml_bridge.baml_py import _host_value_registry_len

    rt = _make_runtime()

    def cb(x: int) -> str:
        return str(x)

    before = _host_value_registry_len()
    # Dict iteration order in CPython 3.7+ is insertion order, so `callback`
    # (registered first) is followed by `x` (an `object()` where an `int` is
    # declared, which fails) — exercising the rollback.
    with pytest.raises(TypeError, match="argument `x`"):
        call_function_sync(rt, "CallCb", {"callback": cb, "x": object()})

    assert _host_value_registry_len() == before


# ---------------------------------------------------------------------------
# BridgeFailure routing: bridge-layer faults (missing callable for key,
# poisoned registry mutex, no tokio runtime, caught Rust panic in dispatch)
# must surface on the host as `BamlPanic(SdkPanic)`, NOT a catchable
# `BamlError(HostCallable)`. The engine side is covered by
# `host_callable_bridge_failure_surfaces_as_internal_error` in
# `crates/bex_engine/tests/host_value_callable.rs`; these tests pin the
# Python-side routing.
# ---------------------------------------------------------------------------


def test_normal_user_exception_is_reraised_and_is_not_a_BamlPanic():
    """Regression guard for the error vs panic dichotomy. A *normal* user
    exception raised by the lambda is a user-level error (catchable), not a
    bridge-layer fault: the caller gets the exception object the callback
    raised, never a `BamlPanic`. If a future change accidentally routed every
    host throw through `send_dispatch_bridge_failure`, the test fails because
    `BamlPanic` subclasses `BaseException` (not `Exception`), so the
    `pytest.raises(Exception)` check below would miss it.
    """
    rt = _make_runtime()
    raised = ValueError("ordinary user error")

    def cb(_x: int) -> str:
        raise raised

    with pytest.raises(Exception) as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})

    assert exc_info.value is raised


def test_sdk_panic_wire_envelope_decodes_to_BamlPanic():
    """Pins the Python-side panic-arm decode path. An engine that emits
    `BamlOutboundResult { panic: { value: baml.panics.SdkPanic{...} } }`
    (which is what `VmInternalError::BridgeFailure` surfaces as) must
    surface to Python as `BamlPanic`, not `BamlError`. Complements the
    engine-level test that pins the inverse direction (a host
    `BridgeFailure` produces this envelope).
    """
    from baml_bridge.errors import BamlError, BamlPanic
    from baml_bridge.cffi.v1 import baml_outbound_pb2

    # `decode_call_result` reads the process-wide typemap to materialize the
    # panic value's class. `baml.panics.SdkPanic` is part of the BAML std
    # namespace, so any initialized runtime makes it resolvable.
    _make_runtime()

    # Build the smallest envelope that round-trips: a `panic` arm with a
    # `baml.panics.SdkPanic` ClassValue holding a single `message` field.
    envelope = baml_outbound_pb2.BamlOutboundResult()
    envelope.panic.value.class_value.name = "baml.panics.SdkPanic"
    msg_field = envelope.panic.value.class_value.fields.add()
    msg_field.key = "message"
    msg_field.value.string_value = "synthetic bridge failure"

    with pytest.raises(BaseException) as exc_info:
        _proto.decode_call_result(envelope.SerializeToString())

    assert isinstance(exc_info.value, BamlPanic), (
        f"expected BamlPanic for a panic-arm envelope, got "
        f"{type(exc_info.value).__name__}"
    )
    assert not isinstance(exc_info.value, BamlError), (
        "BamlPanic must NOT also be a BamlError — they're disjoint"
    )




# ---------------------------------------------------------------------------
# Host values BAML sends back: each key counts its references, and a decoded
# handle holds one of its own.
# ---------------------------------------------------------------------------

# The descriptor states `ConsumeUnknownCb`'s parameters; the body here hands
# back the host value inside the host error the callback raised.
HOST_VALUE_BAML = """\
function ConsumeUnknownCb(callback: (int) -> unknown, x: int) -> unknown {
    callback(x) catch (e) {
        let thrown: baml.errors.HostCallable => thrown._handle,
        _ => null,
    }
}

function TakeUnknown(value: unknown) -> int {
    0
}

function Collect() -> int {
    baml.sys.collect_garbage();
    0
}
"""


def _host_value_runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": HOST_VALUE_BAML})


def _collect(rt: BamlRuntime) -> None:
    call_function_sync(rt, "Collect", {})
    gc.collect()


def _raising(error: BaseException):
    def callback(_x: int):
        raise error

    return callback


def test_a_returned_host_value_outlives_the_engine_and_goes_back_in():
    """The decoded handle holds the key after the engine let go of it, so it
    can be sent again; once the handle is gone every reference is given
    back."""
    from baml_bridge.baml_py import _host_value_registry_len, lookup_host_value

    rt = _host_value_runtime()
    _collect(rt)
    before = _host_value_registry_len()
    error = ValueError("kept")
    handle = call_function_sync(
        rt, "ConsumeUnknownCb", {"callback": _raising(error), "x": 0}
    ).result()
    assert isinstance(handle, BamlPyHandle)
    _collect(rt)
    assert lookup_host_value(handle) is error
    for _ in range(3):
        assert call_function_sync(rt, "TakeUnknown", {"value": handle}).result() == 0
    _collect(rt)
    assert lookup_host_value(handle) is error
    del handle
    _collect(rt)
    assert _host_value_registry_len() == before


def test_a_host_value_decoded_twice_is_the_same_object():
    from baml_bridge.baml_py import _host_value_registry_len, lookup_host_value

    rt = _host_value_runtime()
    _collect(rt)
    before = _host_value_registry_len()
    error = ValueError("one")
    callback = _raising(error)
    first = call_function_sync(
        rt, "ConsumeUnknownCb", {"callback": callback, "x": 0}
    ).result()
    second = call_function_sync(
        rt, "ConsumeUnknownCb", {"callback": callback, "x": 0}
    ).result()
    assert lookup_host_value(first) is error
    assert lookup_host_value(second) is error
    del first, second
    _collect(rt)
    assert _host_value_registry_len() == before


def test_an_outbound_host_value_is_pinned_until_it_is_decoded():
    """A collection between the engine sending a host value and Python
    reading it cannot give the key back: the pin holds it, and decoding lets
    go of exactly that one row."""
    from baml_bridge.baml_py import (
        _host_value_registry_len,
        lookup_host_value,
        new_function_call,
    )

    rt = _host_value_runtime()
    _collect(rt)
    before = _host_value_registry_len()
    error = ValueError("pinned")
    args = _proto.encode_call_args(
        {"callback": _raising(error), "x": 0},
        new_function_call(),
        function_name="ConsumeUnknownCb",
    )
    result = rt.call_function_sync(args, args.kwargs, args.cancel)
    del args
    _collect(rt)
    rows = _live_handle_count()
    handle = _proto.decode_call_result(result)
    assert _live_handle_count() == rows - 1
    assert lookup_host_value(handle) is error
    del handle
    _collect(rt)
    assert _host_value_registry_len() == before


def test_a_refused_send_of_a_received_key_gives_back_only_its_reference():
    """A later value failing to ship rolls back the reference the earlier
    send of a received key took, and nothing more: the handle still holds
    its own."""
    from baml_bridge.baml_py import _host_value_registry_len, lookup_host_value

    rt = _host_value_runtime()
    _collect(rt)
    before = _host_value_registry_len()
    error = ValueError("rolled back")
    handle = call_function_sync(
        rt, "ConsumeUnknownCb", {"callback": _raising(error), "x": 0}
    ).result()
    _collect(rt)
    # An engine handle whose row is gone: cloning it for the wire fails after
    # the host value earlier in the list has shipped.
    key, kind = _seed_media_handle()
    _proto._release_wire_handle(key)
    stale = BamlPyHandle(key, kind)
    for _ in range(3):
        with pytest.raises(RuntimeError, match="invalid handle"):
            call_function_sync(rt, "TakeUnknown", {"value": [handle, stale]})
        assert _host_value_registry_len() == before + 1
    assert lookup_host_value(handle) is error
    assert call_function_sync(rt, "TakeUnknown", {"value": handle}).result() == 0
    del handle
    _collect(rt)
    assert _host_value_registry_len() == before
