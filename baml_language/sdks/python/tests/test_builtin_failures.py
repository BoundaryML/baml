"""Builtin runtime failures (`baml.errors.*` / `baml.panics.*`) must decode
without a generated SDK.

Before this fix every thrown class went through the application typemap, so
with no SDK loaded an `SdkPanic` (or a host callback's own exception, carried
as `baml.errors.HostCallable`) surfaced as an "Unknown class FQN" `BamlError`
that masked the real diagnostic. Now a builtin failure the typemap does not
model decodes to the SDK-owned `BamlFailureValue`.

Only the *no generated SDK* half lives here: it needs an empty typemap, which
the generated-SDK fixture crate (`sdk_tests/crates/python_pydantic2`) cannot
provide. The other half — a generated model still wins, a missing function is
an `InvalidArgument`, host exceptions rehydrate through the generated
`HostCallable` model — runs there.

Run with:
    cd baml_language/sdks/python
    uv run maturin develop --uv
    uv run pytest tests/test_builtin_failures.py -v
"""

from __future__ import annotations

from typing import Any

import pytest

import baml_bridge.proto as proto
from baml_bridge import call_function_sync
from baml_bridge.baml_py import BamlRuntime, new_function_call
from baml_bridge.cffi.v1 import baml_outbound_pb2
from baml_bridge.errors import BamlError, BamlFailureValue, BamlPanic, make_sdk_panic
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


SOURCE = """
function failure_message(value: baml.errors.InvalidArgument) -> string throws never {
    value.message
}
function builtin_failure() -> never throws baml.errors.InvalidArgument {
    throw baml.errors.InvalidArgument { message: "bad input" }
}
function call_callback(callback: (int) -> string, x: int) -> string {
    callback(x)
}
"""


@pytest.fixture
def empty_typemap():
    saved = get_type_map()
    set_type_map(BamlTypeMap())
    yield
    set_type_map(saved)


@pytest.fixture
def runtime(empty_typemap):
    return BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})


def args(name: str, values: dict[str, Any] | None = None) -> bytes:
    return proto.encode_call_args(values or {}, new_function_call(), function_name=name)


def raw(runtime, name: str, values: dict[str, Any] | None = None) -> bytes:
    return runtime.call_function_sync(args(name, values))


def test_sdk_panic_payload_does_not_depend_on_generated_models(
    empty_typemap, monkeypatch
):
    def broken_lookup(_self, _name):
        raise AssertionError("panic decoding must not run application model code")

    monkeypatch.setattr(BamlTypeMap, "get_class", broken_lookup)
    original = make_sdk_panic("setup failed")
    assert isinstance(original.value, BamlFailureValue)
    assert original.value.message == "setup failed"
    envelope = baml_outbound_pb2.BamlOutboundResult()
    envelope.panic.value.class_value.name = "baml.panics.SdkPanic"
    field = envelope.panic.value.class_value.fields.add()
    field.key = "message"
    field.value.string_value = "delivery failed"
    with pytest.raises(BamlPanic) as raised:
        proto.decode_call_result(envelope.SerializeToString())
    assert raised.value.value.class_name == "baml.panics.SdkPanic"
    assert raised.value.value.message == "delivery failed"
    assert "delivery failed" in str(raised.value)


def test_builtin_failure_decodes_without_generated_models_and_preserves_passback(
    runtime, monkeypatch
):
    def broken_lookup(_self, _name):
        raise AssertionError("no generated model exists; lookup must not run")

    monkeypatch.setattr(BamlTypeMap, "get_class", broken_lookup)
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(raw(runtime, "builtin_failure"))
    value = raised.value.value
    assert isinstance(value, BamlFailureValue)
    assert value.class_name == "baml.errors.InvalidArgument"
    assert value.message == "bad input"
    assert raised.value.class_name == "baml.errors.InvalidArgument"
    # Passing the caught failure back keeps its builtin class identity.
    result = raw(runtime, "failure_message", {"value": value})
    assert proto.decode_call_result(result) == "bad input"


def test_host_callback_exception_keeps_identity_without_generated_models(runtime):
    """A callback's own exception rides through BAML as
    `baml.errors.HostCallable`. With no generated model for that class the
    payload decodes to a `BamlFailureValue` whose `_handle` field still
    rehydrates the *original* exception object — never a `BamlPanic`, and
    never an "unknown class" decode failure."""
    original = ValueError("ordinary user error")

    def cb(_x: int) -> str:
        raise original

    with pytest.raises(ValueError) as raised:
        call_function_sync(runtime, "call_callback", {"callback": cb, "x": 1})
    assert raised.value is original
    assert not isinstance(raised.value, BamlPanic)
