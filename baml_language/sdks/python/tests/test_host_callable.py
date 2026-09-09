"""Tests for the Python host-callable bridge.

These tests exercise the auto-registration encoder branch and the
dispatch round trip without needing a generated SDK — the test builds an
ad-hoc BAML runtime that declares one function whose first parameter is
a `(int) -> string` callable.

Run with:
    cd baml_language/sdks/python
    uv run maturin develop --uv
    uv run pytest tests/test_host_callable.py -v
"""

from __future__ import annotations

import gc
import weakref

import pytest

import baml_bridge.proto as _proto

from baml_bridge import (
    BamlPyHandle,
    BamlRuntime,
    call_function_sync,
    call_function,
    flush_events,
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
"""


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


def test_release_fires_after_collection_with_runtime_open():
    """The engine's last closure owner releases the registration after GC.

    Keeping the default runtime installed must not keep dead callbacks alive.
    Use the public collection operation to test ownership deterministically.
    """
    rt = _make_runtime()

    class CallableObj:
        def __call__(self, x: int) -> str:
            return str(x)

    cb = CallableObj()
    wr = weakref.ref(cb)
    result = call_function_sync(rt, "CallCb", {"callback": cb, "x": 3})
    assert result.result() == "3"
    del cb, result
    flush_events()
    call_function_sync(rt, "baml.sys.collect_garbage", {}).result()
    flush_events()
    gc.collect()
    assert wr() is None
    assert (
        call_function_sync(
            rt, "CallIntCb", {"callback": lambda x: x + 1, "x": 1}
        ).result()
        == 2
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
        return object()  # not encodable by _set_inbound_value

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
    against the callback contract. The original Python handle remains live."""
    rt = _make_runtime()
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    with pytest.raises(BamlPanic) as raised:
        call_function_sync(
            rt,
            "ConsumeUnknownCb",
            {"callback": lambda _x: handle, "x": 7},
        )

    assert raised.value.class_name == "baml.panics.HostContractViolation"
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
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        raise BamlError([handle, object()])

    with pytest.raises(Exception):
        call_function_sync(rt, "ConsumeUnknownCb", {"callback": cb, "x": 1})

    assert _live_handle_count() == before


# ---------------------------------------------------------------------------
# Encode-error rollback releases callables registered for earlier kwargs.
# ---------------------------------------------------------------------------


def test_encode_error_releases_registered_callables(monkeypatch):
    """If a later kwarg fails to encode, every callable registered for an
    earlier kwarg must be released via `release_host_callable` so it doesn't
    leak in the per-process registry."""
    released: list = []
    # Spy on the symbol where `encode_call_args` resolves it (proto's
    # namespace), not its canonical home in `baml_py`. proto re-imports but
    # doesn't re-export it, so the attribute read trips reportPrivateImportUsage.
    real_release = _proto.release_host_callable  # pyright: ignore[reportPrivateImportUsage]

    def spy_release(key):
        released.append(key)
        return real_release(key)

    monkeypatch.setattr(_proto, "release_host_callable", spy_release)

    def cb(x: int) -> str:
        return str(x)

    # `bad` is an un-encodable value (an arbitrary object). Dict iteration
    # order in CPython 3.7+ is insertion order, so `callback` (registered
    # first) is followed by `bad` (which fails) — exercising the rollback.
    with pytest.raises(Exception):
        _proto.encode_call_args({"callback": cb, "bad": object()}, call_id=3)

    assert len(released) == 1, (
        f"expected exactly one callable to be released on rollback, got {released}"
    )


# ---------------------------------------------------------------------------
# BridgeFailure routing: bridge-layer faults (missing callable for key,
# poisoned registry mutex, no tokio runtime, caught Rust panic in dispatch)
# must surface on the host as `BamlPanic(SdkPanic)`, NOT a catchable
# `BamlError(HostCallable)`. The engine side is covered by
# `host_callable_bridge_failure_surfaces_as_internal_error` in
# `crates/bex_engine/tests/host_value_callable.rs`; these tests pin the
# Python-side routing.
# ---------------------------------------------------------------------------


def test_normal_user_exception_preserves_same_host_identity():
    """A permitted host exception comes back as the original Python object."""
    rt = _make_runtime()
    original = ValueError("ordinary user error")

    def cb(_x: int) -> str:
        raise original

    with pytest.raises(ValueError) as exc_info:
        call_function_sync(rt, "CallCb", {"callback": cb, "x": 1})

    assert exc_info.value is original


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


def test_encode_success_does_not_release(monkeypatch):
    """A successful encode must NOT eagerly release the callable — that
    happens later via the engine's GC-timed release path."""
    released: list = []
    monkeypatch.setattr(
        _proto, "release_host_callable", lambda key: released.append(key)
    )
    # Stub registration too, so the callable is never inserted into the real
    # process-wide table. Otherwise this test would leak a live host-value key:
    # `encode_call_args` registers `cb`, but the bytes are discarded (never sent
    # to the engine) and `release_host_callable` above is a no-op recorder, so
    # nothing would ever release it.
    monkeypatch.setattr(_proto, "register_host_callable", lambda _value: 999)

    def cb(x: int) -> str:
        return str(x)

    _proto.encode_call_args({"callback": cb, "x": 5}, call_id=4)
    assert released == [], "successful encode should not release the callable"
