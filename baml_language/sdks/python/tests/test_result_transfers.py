"""Real Python/native result ownership; no mocked receipt or handle registry."""

from __future__ import annotations

import asyncio
import copy
import gc
import weakref
from typing import Any

import pytest
from pydantic import BaseModel, model_validator

import baml_bridge.proto as proto
from baml_bridge.baml_py import (
    BamlEncodedResult,
    BamlImage,
    BamlAudio,
    BamlVideo,
    BamlPdf,
    BamlRuntime,
    _live_handle_count,
    _pending_transfer_count,
    new_function_call,
    lookup_host_value,
    register_host_callable,
    shutdown_runtime,
)
from baml_bridge.cffi.v1 import baml_handle_pb2, baml_inbound_pb2, baml_outbound_pb2
from baml_bridge.errors import BamlError, BamlFailureValue, BamlPanic, make_sdk_panic
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


SOURCE = """
type Unary = (x: int) -> int throws never
function make() -> Unary throws never { (x: int) -> int { x + 1 } }
function many() -> Unary[] throws never { [make(), make()] }
class Bundle { first: Unary, second: Unary }
function bundle() -> Bundle throws never { Bundle { first: make(), second: make() } }
function fail() -> never throws Bundle { throw bundle() }
function callback(cb: (int) -> int) -> int { cb(1) }
function throw_null() -> never throws null { throw null }
function failure_message(value: baml.errors.InvalidArgument) -> string throws never {
    value.message
}
function builtin_failure() -> never throws baml.errors.InvalidArgument {
    throw baml.errors.InvalidArgument { message: "bad input" }
}
function opaque<T>(value: T) -> T throws never { value }
function visit(cb: (Bundle) -> int) -> int { cb(bundle()) }
function visit_many(cb: (Bundle, Bundle) -> int) -> int { cb(bundle(), bundle()) }
function visit_image(value: image, cb: (image) -> string) -> string { cb(value) }
function visit_audio(value: audio, cb: (audio) -> string) -> string { cb(value) }
function visit_video(value: video, cb: (video) -> string) -> string { cb(value) }
function visit_pdf(value: pdf, cb: (pdf) -> string) -> string { cb(value) }

"""


@pytest.fixture
def runtime():
    saved = get_type_map()
    rt = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    yield rt
    set_type_map(saved)
    shutdown_runtime()
    gc.collect()


def args(name: str, values: dict[str, Any] | None = None) -> bytes:
    return proto.encode_call_args(values or {}, new_function_call(), function_name=name)


def raw(runtime, name: str) -> BamlEncodedResult:
    result = runtime.call_function_sync(args(name))
    assert isinstance(result, BamlEncodedResult)
    return result


def opaque_result(runtime, value):
    # Initial host registration uses the host registry; all subsequent returns
    # and pass-back must use owned table leases. The registry holds arbitrary
    # Py objects; the inbound discriminator declares this one opaque.
    request = baml_inbound_pb2.CallFunctionArgs.FromString(args("opaque"))
    field = request.kwargs.add()
    field.string_key = "value"
    field.value.handle.key = register_host_callable(value)
    field.value.handle.handle_type = baml_handle_pb2.HOST_VALUE_OPAQUE
    return runtime.call_function_sync(request.SerializeToString())


class OpaquePayload:
    pass


def test_opaque_host_reference_owns_copy_and_passback(runtime):
    original = OpaquePayload()
    weak = weakref.ref(original)
    result = opaque_result(runtime, original)
    envelope = baml_outbound_pb2.BamlOutboundResult.FromString(result.payload)
    assert envelope.ok.handle_value.handle_type == baml_handle_pb2.HOST_REFERENCE
    handle = proto.decode_call_result(result)
    del original, result
    cloned = copy.copy(handle)
    assert lookup_host_value(cloned) is weak()
    result = runtime.call_function_sync(args("opaque", {"value": cloned}))
    returned = proto.decode_call_result(result)
    assert lookup_host_value(returned) is weak()
    del handle, cloned, result
    shutdown_runtime()
    gc.collect()
    assert weak() is not None, "the remaining SDK lease owns the host object"
    assert lookup_host_value(returned) is weak()
    del returned
    gc.collect()
    assert weak() is None, "last release needs no later BAML call to drain cleanup"


def test_discarded_opaque_result_releases_registration(runtime):
    original = OpaquePayload()
    weak = weakref.ref(original)
    result = opaque_result(runtime, original)
    del original, result
    shutdown_runtime()
    gc.collect()
    assert weak() is None


def test_opaque_passback_encode_failure_releases_partial_clone(runtime):
    original = OpaquePayload()
    weak = weakref.ref(original)
    handle = proto.decode_call_result(opaque_result(runtime, original))
    baseline = _live_handle_count()
    with pytest.raises(TypeError):
        args("opaque", {"value": [handle, object()]})
    assert _live_handle_count() == baseline
    assert lookup_host_value(handle) is original
    del original, handle
    shutdown_runtime()
    gc.collect()
    assert weak() is None


def test_failed_opaque_decode_invalidates_escaped_handle(runtime, monkeypatch):
    original = OpaquePayload()
    weak = weakref.ref(original)
    result = opaque_result(runtime, original)
    escaped = []
    decode = proto._decode_handle

    def reject(handle, type_map):
        escaped.append(decode(handle, type_map))
        raise ValueError("reject host reference")

    monkeypatch.setattr(proto, "_decode_handle", reject)
    with pytest.raises(ValueError, match="reject host reference"):
        proto.decode_call_result(result)
    with pytest.raises(RuntimeError, match="not been adopted"):
        copy.copy(escaped[0])
    del original, result
    shutdown_runtime()
    gc.collect()
    assert weak() is None
    assert lookup_host_value(escaped[0]) is None


def register_result_class(result: BamlEncodedResult, cls: type, *, thrown=False):
    envelope = baml_outbound_pb2.BamlOutboundResult.FromString(result.payload)
    value = envelope.error.value if thrown else envelope.ok
    while value.WhichOneof("value") == "union_variant_value":
        value = value.union_variant_value.value
    assert value.WhichOneof("value") == "class_value"
    # These classes model generated classes or application validators. The
    # exact FQN is read from this real result, not guessed from a test package.
    typemap = BamlTypeMap()
    typemap._class_cache[value.class_value.name] = cls
    set_type_map(typemap)


def test_unread_result_drop_releases_all_handles(runtime):
    baseline = _live_handle_count()
    for _ in range(16):
        result = raw(runtime, "many")
        assert _live_handle_count() == baseline + 2
        assert _pending_transfer_count() == 1
        del result
        gc.collect()
        assert _pending_transfer_count() == 0
        assert _live_handle_count() == baseline


def test_successful_decode_adopts_and_keeps_callable_alive(runtime):
    baseline = _live_handle_count()
    result = raw(runtime, "make")
    closure = proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert closure(3) == 4
    result._discard()  # consumed result cannot revoke adopted ownership
    del result
    assert closure(8) == 9
    del closure
    gc.collect()
    assert _live_handle_count() == baseline


def test_partial_decode_failure_invalidates_escaped_child(runtime, monkeypatch):
    baseline = _live_handle_count()
    result = raw(runtime, "many")
    original = proto._decode_handle
    escaped = []

    def fail_second(handle, type_map):
        if escaped:
            raise ValueError("second child rejected")
        child = original(handle, type_map)
        escaped.append(child)
        with pytest.raises(RuntimeError, match="not been adopted"):
            copy.copy(child._handle)
        return child

    monkeypatch.setattr(proto, "_decode_handle", fail_second)
    with pytest.raises(ValueError, match="second child rejected"):
        proto.decode_call_result(result)
    assert _live_handle_count() == baseline
    assert _pending_transfer_count() == 0
    with pytest.raises(RuntimeError, match="discarded"):
        escaped[0](1)


def test_parse_failure_discards_without_visiting_handles(runtime, monkeypatch):
    baseline = _live_handle_count()
    result = raw(runtime, "many")

    def corrupt_payload(_data):
        # Exercise the real protobuf parser, while keeping the receipt outside
        # those corrupt bytes in the native result object.
        return baml_outbound_pb2.BamlOutboundResult.FromString(b"\xff")

    monkeypatch.setattr(proto, "_decode_call_outcome", corrupt_payload)
    with pytest.raises(Exception):
        proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


def test_pydantic_rejection_releases_children_kept_by_validator(runtime):
    baseline = _live_handle_count()
    escaped = []

    class Reject(BaseModel):
        first: Any
        second: Any

        @model_validator(mode="after")
        def reject(self):
            escaped.append(self.first)
            raise ValueError("model rejected")

    result = raw(runtime, "bundle")
    register_result_class(result, Reject)
    with pytest.raises(ValueError, match="model rejected"):
        proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline
    with pytest.raises(RuntimeError, match="discarded"):
        escaped[0](1)


def test_pydantic_replacement_releases_unused_decoded_children(runtime):
    baseline = _live_handle_count()

    class Replace(BaseModel):
        first: Any
        second: Any

        @model_validator(mode="before")
        @classmethod
        def replace(cls, _data):
            return {"first": None, "second": None}

    result = raw(runtime, "bundle")
    register_result_class(result, Replace)
    value = proto.decode_call_result(result)
    assert value.first is None and value.second is None
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


def test_reentrant_decode_has_its_own_adoption_transaction(runtime):
    baseline = _live_handle_count()
    nested = []

    class Reenter(BaseModel):
        first: Any
        second: Any

        @model_validator(mode="before")
        @classmethod
        def reenter(cls, _data):
            nested.append(proto.decode_call_result(raw(runtime, "make")))
            raise ValueError("outer model rejected")

    result = raw(runtime, "bundle")
    register_result_class(result, Reenter)
    with pytest.raises(ValueError, match="outer model rejected"):
        proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline + 1
    assert nested[0](4) == 5
    nested.clear()
    gc.collect()
    assert _live_handle_count() == baseline


def test_declared_error_adopts_nested_references_before_raising(runtime):
    baseline = _live_handle_count()
    result = raw(runtime, "fail")
    register_result_class(result, dict, thrown=True)
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert raised.value.value["first"](9) == 10
    assert raised.value.value["second"](10) == 11
    del raised, result
    gc.collect()
    assert _live_handle_count() == baseline


def test_replacing_runtime_discards_pending_result(runtime):
    baseline = _live_handle_count()
    result = raw(runtime, "many")
    BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    assert _live_handle_count() == baseline
    with pytest.raises(RuntimeError, match="closed"):
        proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


def test_native_exception_is_recovered_before_adoption_releases_registration(runtime):
    set_type_map(BamlTypeMap())
    original = ValueError("host failure identity")

    def callback(_value):
        raise original

    result = runtime.call_function_sync(args("callback", {"cb": callback}))
    with pytest.raises(ValueError) as raised:
        proto.decode_call_result(result)
    assert raised.value is original
    assert _pending_transfer_count() == 0


def test_present_null_error_is_distinct_from_missing_error_value(runtime):
    result = raw(runtime, "throw_null")
    envelope = baml_outbound_pb2.BamlOutboundResult.FromString(result.payload)
    assert envelope.error.HasField("value")
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(result)
    assert raised.value.value is None
    assert _pending_transfer_count() == 0
    missing = baml_outbound_pb2.BamlOutboundResult()
    missing.error.SetInParent()
    with pytest.raises(BamlError, match="missing its value"):
        proto.decode_call_result(missing.SerializeToString())


def test_builtin_failure_ignores_application_typemap_and_preserves_passback(
    runtime, monkeypatch
):
    def broken_lookup(_self, _name):
        raise AssertionError("application model lookup must not hide a builtin failure")

    monkeypatch.setattr(BamlTypeMap, "get_class", broken_lookup)
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(raw(runtime, "builtin_failure"))
    value = raised.value.value
    assert isinstance(value, BamlFailureValue)
    assert value.class_name == "baml.errors.InvalidArgument"
    assert value.message == "bad input"
    result = runtime.call_function_sync(args("failure_message", {"value": value}))
    assert proto.decode_call_result(result) == "bad input"
    assert _pending_transfer_count() == 0


def test_missing_function_is_a_caller_error_with_no_generated_sdk(runtime):
    set_type_map(BamlTypeMap())
    with pytest.raises(BamlError, match="Function not found") as raised:
        proto.decode_call_result(raw(runtime, "missing_function"))
    assert raised.value.class_name == "baml.errors.InvalidArgument"
    assert _pending_transfer_count() == 0


def test_sdk_panic_payload_does_not_depend_on_generated_models(monkeypatch):
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


@pytest.mark.asyncio
async def test_async_result_is_owned_until_decoded(runtime):
    baseline = _live_handle_count()
    result = await runtime.call_function(args("many"))
    assert isinstance(result, BamlEncodedResult)
    assert _pending_transfer_count() == 1
    values = proto.decode_call_result(result)
    assert _pending_transfer_count() == 0
    assert len(values) == 2
    del values, result
    gc.collect()
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
async def test_unobserved_completed_future_releases_its_result(runtime):
    baseline = _live_handle_count()
    future = runtime.call_function(args("many"))

    async def wait_until_ready(pending):
        while not pending.done():
            await asyncio.sleep(0.01)

    await asyncio.wait_for(wait_until_ready(future), timeout=10)
    assert _pending_transfer_count() == 1
    del future
    # Let PyO3's completion task release its temporary Python future owner.
    await asyncio.sleep(0)
    gc.collect()
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
async def test_cancelled_consumer_discards_delivered_but_undecoded_result(runtime):
    baseline = _live_handle_count()
    received = asyncio.Event()
    resume = asyncio.Event()

    async def consumer():
        result = await runtime.call_function(args("many"))
        received.set()
        await resume.wait()
        return proto.decode_call_result(result)

    task = asyncio.create_task(consumer())
    await asyncio.wait_for(received.wait(), timeout=10)
    assert _pending_transfer_count() == 1
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    del task
    await asyncio.sleep(0)
    gc.collect()
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


def test_callback_arguments_adopt_before_entry_and_outlive_call(runtime):
    class Bundle(BaseModel):
        first: Any
        second: Any

    result = raw(runtime, "bundle")
    register_result_class(result, Bundle)
    result._discard()
    retained = []

    def callback(bundle):
        retained.append(bundle.first)
        # Do not reenter a sync BAML call on this Tokio worker; checking a
        # handle clone proves adoption without crossing that separate limit.
        copy.copy(bundle.first._handle)
        return 9

    result = runtime.call_function_sync(args("visit", {"cb": callback}))
    assert proto.decode_call_result(result) == 9
    assert retained[0](4) == 5


def test_callback_decode_rejection_invalidates_refs_before_user_body(runtime):
    escaped = []
    entered = []

    class RejectSecond(BaseModel):
        first: Any
        second: Any

        @model_validator(mode="after")
        def reject(self):
            escaped.append(self.first)
            if len(escaped) == 2:
                raise ValueError("callback argument rejected")
            return self

    result = raw(runtime, "bundle")
    register_result_class(result, RejectSecond)
    result._discard()
    baseline = _live_handle_count()

    def callback(*values):
        entered.append(values)
        return 0

    result = runtime.call_function_sync(args("visit_many", {"cb": callback}))
    with pytest.raises(Exception, match="callback argument rejected"):
        proto.decode_call_result(result)
    assert len(escaped) == 2
    assert not entered
    for child in escaped:
        with pytest.raises(RuntimeError, match="discarded"):
            copy.copy(child._handle)
    del result
    gc.collect()
    assert _live_handle_count() == baseline


@pytest.mark.parametrize(
    "kind,wrapper,mime",
    [
        ("image", BamlImage, "image/png"),
        ("audio", BamlAudio, "audio/wav"),
        ("video", BamlVideo, "video/mp4"),
        ("pdf", BamlPdf, "application/pdf"),
    ],
)
def test_media_callback_payload_survives_original_wrapper(runtime, kind, wrapper, mime):
    url = f"https://example.com/{kind}"
    original = wrapper.from_url(url, mime_type=mime)
    retained = []

    def callback(value):
        assert isinstance(value, wrapper)
        assert value.mime_type() == mime
        retained.append(value)
        return value.url()

    result = runtime.call_function_sync(
        args(f"visit_{kind}", {"value": original, "cb": callback})
    )
    assert proto.decode_call_result(result) == url
    del original, result
    gc.collect()
    assert retained[0].url() == url
    assert retained[0].mime_type() == mime
