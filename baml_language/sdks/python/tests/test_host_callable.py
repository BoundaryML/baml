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
    _seed_generic_media_handle,
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


def test_callable_returning_unencodable_surfaces_as_error():
    """A callback whose *result* cannot be encoded must surface as a BAML
    error, not hang the engine. `object()` has no inbound encoding, so
    `encode_result_inbound` raises a `TypeError`; the dispatch path turns it
    into a thrown `baml.errors.HostCallable` Instance and completes the call."""
    rt = _make_runtime()

    def cb(_x: int):
        return object()  # fits no declared type the bridge can encode

    with pytest.raises(Exception) as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})
    # The important property is that it raised (completed) rather than hung.
    assert exc_info.value is not None


def test_callable_raising_during_result_property_still_completes():
    """A callback that returns an object whose attribute access raises while
    being encoded must still complete the call with an error."""
    rt = _make_runtime()

    class Hostile:
        def __iter__(self):
            raise RuntimeError("hostile iter")

    def cb(_x: int):
        # A non-dict, non-list object with no inbound mapping → TypeError in
        # the encoder; completes as an error.
        return Hostile()

    with pytest.raises(Exception):
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})


def test_host_result_successful_encode_transfers_capability_clone_to_engine():
    """The handle itself encodes successfully, so the engine receives and
    drains its cloned key before rejecting this synthetic generic-media handle
    against the `string` that the callback declares. The original Python
    handle remains live."""
    rt = _make_runtime()
    key, handle_type = _seed_generic_media_handle()
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
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        return [handle, object()]

    with pytest.raises(Exception):
        call_function_sync(rt, "ConsumeUnknownCb", {"callback": cb, "x": 1})

    assert _live_handle_count() == before


def test_host_throw_encode_failure_releases_capability_clone():
    rt = _make_runtime()
    # The runtime and the handle table are process-global. Creating this
    # runtime dropped the previous test's engine heap, but an exception a
    # previous test raised still lingers in the synchronous call plumbing
    # until the next call, and in a traceback cycle until the collector runs;
    # settle both so the baseline counts this test's handles alone.
    call_function_sync(rt, "Collect", {})
    gc.collect()
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        raise BamlError([handle, object()])

    with pytest.raises(Exception):
        call_function_sync(rt, "ConsumeUnknownCb", {"callback": cb, "x": 1})

    # The thrown value cannot be encoded, so the callback's native exception
    # is registered as an opaque host value and re-raised with its identity.
    # The engine's heap holds that registration until it collects the thrown
    # `baml.errors.HostCallable` instance, and the exception's traceback
    # keeps the dispatch frame (and its control handles) alive until then.
    call_function_sync(rt, "Collect", {})
    gc.collect()
    assert _live_handle_count() == before


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


