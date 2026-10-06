"""The Python host-callable bridge: what the bridge owns while a callback
runs, and what it releases when a value does not encode.

The round trips of a callback through the generated SDK are in
`test_host_callables.py`. These tests look at the bridge's own tables: the
host-callable registry and the handle table.
"""

from __future__ import annotations

import gc

import pytest

import baml_bridge.proto as _proto
import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
from baml_bridge import BamlError, BamlPanic, BamlPyHandle
from baml_bridge.baml_py import _live_handle_count, _seed_generic_media_handle
from baml_bridge.cffi.v1 import baml_outbound_pb2
from baml_sdk.host_callable_tests import call_with_callback


# ---------------------------------------------------------------------------
# Abnormal paths must still complete the call (engine never hangs).
# ---------------------------------------------------------------------------


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_callable_returning_unencodable_surfaces_as_error():
    """A callback whose *result* cannot be encoded completes the call with the
    encoder's `TypeError`: `object()` has no inbound encoding."""

    def cb(_x: int):
        return object()

    with pytest.raises(TypeError, match="object"):
        call_with_callback(callback=cb, x=1)


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_callable_returning_hostile_object_still_completes():
    """A result that raises when the encoder looks at it completes the call
    with an error."""

    class Hostile:
        def __iter__(self):
            raise RuntimeError("hostile iter")

    def cb(_x: int):
        return Hostile()

    with pytest.raises(TypeError, match="Hostile"):
        call_with_callback(callback=cb, x=1)


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_host_result_successful_encode_transfers_capability_clone_to_engine():
    """The handle encodes, so the engine receives its cloned key and drains it
    before it rejects the handle against the `string` that the callback
    declares. The original Python handle stays live."""
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    with pytest.raises(BamlPanic, match="string"):
        call_with_callback(callback=lambda _x: handle, x=7)

    assert _live_handle_count() == before


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_host_result_encode_failure_releases_capability_clone():
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    before = _live_handle_count()

    def cb(_x: int):
        return [handle, object()]

    with pytest.raises(TypeError, match="object"):
        call_with_callback(callback=cb, x=1)

    assert _live_handle_count() == before


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_host_throw_encode_failure_releases_capability_clone():
    key, handle_type = _seed_generic_media_handle()
    handle = BamlPyHandle(key, handle_type)
    # This test collects garbage before its second count, so it does that
    # before the first count too: handles of earlier tests must not count.
    gc.collect()
    before = _live_handle_count()

    raised = BamlError([handle, object()])

    def cb(_x: int):
        raise raised

    # The value of the error does not encode, so the bridge sends the error
    # as an opaque host exception, and the caller gets the same object back.
    with pytest.raises(BamlError) as exc_info:
        call_with_callback(callback=cb, x=1)

    assert exc_info.value is raised
    # The traceback holds the frame that decoded the error, and with it the
    # handle of the decoded value.
    del exc_info
    raised.__traceback__ = None
    gc.collect()
    assert _live_handle_count() == before


# ---------------------------------------------------------------------------
# Encode-error rollback releases callables registered for earlier kwargs.
# ---------------------------------------------------------------------------


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_encode_error_releases_registered_callables(monkeypatch):
    """If a later kwarg fails to encode, every callable registered for an
    earlier kwarg must be released via `release_host_callable` so it doesn't
    leak in the per-process registry."""
    released: list = []
    # Spy on the symbol where `encode_call_args` resolves it (proto's
    # namespace), not its canonical home in `baml_py`.
    real_release = _proto.release_host_callable

    def spy_release(key):
        released.append(key)
        return real_release(key)

    monkeypatch.setattr(_proto, "release_host_callable", spy_release)

    def cb(x: int) -> str:
        return str(x)

    # `bad` is an un-encodable value (an arbitrary object). Dict iteration
    # order is insertion order, so `callback` (registered first) is followed
    # by `bad` (which fails) — exercising the rollback.
    with pytest.raises(TypeError, match="object"):
        _proto.encode_call_args({"callback": cb, "bad": object()}, call_id=3)

    assert len(released) == 1, f"expected exactly one callable to be released on rollback, got {released}"


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_encode_success_does_not_release(monkeypatch):
    """A successful encode must NOT eagerly release the callable — that
    happens later via the engine's GC-timed release path."""
    released: list = []
    monkeypatch.setattr(_proto, "release_host_callable", lambda key: released.append(key))
    # Stub registration too, so the callable is never inserted into the real
    # process-wide table. Otherwise this test would leak a live host-value key:
    # `encode_call_args` registers `cb`, but the bytes are discarded (never sent
    # to the engine) and `release_host_callable` above is a no-op recorder, so
    # nothing would ever release it.
    monkeypatch.setattr(_proto, "register_host_callable", lambda _value, _marker: 999)

    def cb(x: int) -> str:
        return str(x)

    _proto.encode_call_args({"callback": cb, "x": 5}, call_id=4)
    assert released == [], "successful encode should not release the callable"


# ---------------------------------------------------------------------------
# BridgeFailure routing: bridge-layer faults (missing callable for key,
# poisoned registry mutex, no tokio runtime, caught Rust panic in dispatch)
# must surface on the host as `BamlPanic(SdkPanic)`, NOT as an error that
# BAML code can catch (`baml.errors.HostCallable`). The engine side is covered by
# `host_callable_bridge_failure_surfaces_as_internal_error` in
# `crates/bex_engine/tests/host_value_callable.rs`; this test pins the
# Python-side routing.
# ---------------------------------------------------------------------------


# SDK_PARITY_LINT(skip): reads the Python bridge's host-callable registry and handle table
def test_sdk_panic_wire_envelope_decodes_to_baml_panic():
    """An engine that emits
    `BamlOutboundResult { panic: { value: baml.panics.SdkPanic{...} } }`
    (which is what `VmInternalError::BridgeFailure` surfaces as) must
    surface to Python as `BamlPanic`."""
    envelope = baml_outbound_pb2.BamlOutboundResult()
    envelope.panic.value.class_value.name = "baml.panics.SdkPanic"
    msg_field = envelope.panic.value.class_value.fields.add()
    msg_field.key = "message"
    msg_field.value.string_value = "synthetic bridge failure"

    with pytest.raises(BamlPanic, match="synthetic bridge failure") as exc_info:
        _proto.decode_call_result(envelope.SerializeToString())

    assert exc_info.value.class_name == "baml.panics.SdkPanic"
