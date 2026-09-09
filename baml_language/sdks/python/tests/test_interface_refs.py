"""Real native interface dispatch and ownership, below generated public APIs.

The small subclasses exercise the decoder registration used by codegen. They
are not evidence that a generator already emits the complete interface API.
"""

from __future__ import annotations

import copy
import gc
from pathlib import Path
from typing import Any
import weakref

import pytest
from pydantic import BaseModel

from baml_bridge import proto
from baml_bridge._interface import BamlInterfaceRef, _method_type_arguments
from baml_bridge.baml_py import (
    BamlRuntime,
    _live_handle_count,
    _pending_transfer_count,
    new_function_call,
    shutdown_runtime,
)
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


class GreeterRef(BamlInterfaceRef):
    __slots__ = ()
    __baml_interface_fqn__ = "user.Greeter"
    __baml_interface_generic_count__ = 0
    __baml_interface_associated_types__ = []

    async def greet(self, name: str) -> str:
        return await self._invoke("greet", {"name": name})

    async def label(self) -> str:
        return await self._invoke("label", {})


class CounterRef(BamlInterfaceRef):
    __slots__ = ()
    __baml_interface_fqn__ = "user.Counter"
    __baml_interface_generic_count__ = 0
    __baml_interface_associated_types__ = []


class Done(BaseModel):
    pass


def test_method_type_choices_preserve_definitions_and_declaration_order():
    definition = proto.BamlType._from_python(str)
    arguments = _method_type_arguments({"U": int, "T": definition}, ("T", "U"))
    assert arguments[0] is definition
    assert arguments[1] == proto.python_type_to_wire_ty(int)
    for invalid in ({}, {"T": definition}, {"T": definition, "U": int, "V": str}):
        with pytest.raises(TypeError, match="must specify exactly"):
            _method_type_arguments(invalid, ("T", "U"))


def test_interface_type_token_encodes_associated_bindings_separately():
    import types
    from typing_extensions import Never

    class SourceToken:
        __baml_interface_fqn__ = "user.Source"
        __baml_interface_generic_count__ = 1
        __baml_interface_associated_types__ = ("Output", "Error")

    token = types.GenericAlias(SourceToken, (int, str, Never))
    wire = proto.python_type_to_wire_ty(token).interface
    assert list(wire.type_args) == [proto.python_type_to_wire_ty(int)]
    assert [(b.name, b.ty) for b in wire.bindings] == [
        ("Output", proto.python_type_to_wire_ty(str)),
        ("Error", proto.python_type_to_wire_ty(Never)),
    ]
    with pytest.raises(
        TypeError, match="requires 1 type arguments and 2 associated bindings"
    ):
        proto.python_type_to_wire_ty(types.GenericAlias(SourceToken, (str, Never)))


SOURCE = (
    (
        Path(__file__).resolve().parents[3]
        / "sdk_tests/fixtures/interfaces/baml_src/main.baml"
    ).read_text()
    + """
interface Relay {
    function next(self) -> Greeter throws never
    function fail(self) -> never throws Greeter
}
class RelayImpl {
    value: Greeter,
    implements Relay {
        function next(self) -> Greeter throws never { self.value }
        function fail(self) -> never throws Greeter { throw self.value }
    }
}
function make_relay() -> Relay throws never {
    RelayImpl { value: make_greeter("Hello") }
}
class ReadError { message: string }
interface Source {
    type Output
    type Error = never
    function read(self) -> Self.Output throws Self.Error
}
class TextSource {
    implements Source {
        type Output = string
        type Error = never
        function read(self) -> string throws never { "Ada" }
    }
}
class FallibleTextSource {
    implements Source {
        type Output = string
        type Error = ReadError
        function read(self) -> string throws ReadError { "Ada" }
    }
}
function make_source() -> Source<Output=string, Error=never> throws never { TextSource {} }
function make_fallible_source() -> Source<Output=string, Error=ReadError> throws never {
    FallibleTextSource {}
}
function read_infallible(value: Source<Output=string, Error=never>) -> string throws never {
    value.read()
}
"""
)


@pytest.fixture
def runtime():
    saved = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={"baml.iter.Done": (__name__, "Done")},
            enums={},
            type_aliases={},
            interface_refs={
                "user.Greeter": (__name__, "GreeterRef"),
                "user.Counter": (__name__, "CounterRef"),
            },
        )
    )
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    yield runtime
    set_type_map(saved)
    shutdown_runtime()
    gc.collect()


def request(name: str, /, **kwargs: Any) -> bytes:
    return proto.encode_call_args(kwargs, new_function_call(), function_name=name)


def call(runtime: BamlRuntime, name: str, /, **kwargs: Any) -> Any:
    return proto.decode_call_result(runtime.call_function_sync(request(name, **kwargs)))


@pytest.mark.asyncio
async def test_returned_interface_dispatches_default_and_passes_back(runtime):
    greeter = call(runtime, "make_greeter", prefix="Hello")
    assert isinstance(greeter, GreeterRef)
    assert await greeter.greet("Ada") == "Hello, Ada!"
    assert await greeter.label() == "greeter"
    returned = call(runtime, "pass_greeter", value=greeter)
    greeter.close()
    assert await returned.greet("Grace") == "Hello, Grace!"
    assert call(runtime, "welcome", value=returned, name="Ada") == "Hello, Ada!"
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
async def test_interface_copies_share_owner_state_and_close_independently(runtime):
    baseline = _live_handle_count()
    counter = call(runtime, "make_counter", initial=3)
    cloned = copy.copy(counter)
    deep = copy.deepcopy(counter)
    assert await counter._invoke("add", {"amount": 4}) == 7
    counter.close()
    counter.close()
    assert await cloned._invoke("current", {}) == 7
    assert call(runtime, "add_in_baml", value=deep, amount=2) == 9
    assert await cloned._invoke("current", {}) == 9
    with pytest.raises(RuntimeError, match="closed"):
        counter._invoke("current", {})
    cloned.close()
    deep.close()
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
async def test_prepared_call_outlives_local_reference(runtime):
    counter = call(runtime, "make_counter", initial=3)
    pending = counter._invoke("add", {"amount": 4})
    counter.close()
    assert await pending == 7


@pytest.mark.asyncio
@pytest.mark.parametrize("replace", [False, True])
async def test_callback_argument_keeps_issuing_runtime_after_call(runtime, replace):
    received = []
    baseline = _live_handle_count()

    async def update(counter):
        assert isinstance(counter, CounterRef)
        received.append(counter)
        return await counter._invoke("add", {"amount": 2})

    encoded = request("visit_counter", initial=10, callback=update)
    # Callback registration must retain this SDK even if another SDK is loaded
    # before BAML invokes it; its interface methods keep that same context.
    set_type_map(BamlTypeMap())
    assert proto.decode_call_result(await runtime.call_function(encoded)) == 12
    counter = received.pop()
    if replace:
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
        with pytest.raises(BamlError, match="closed or replaced"):
            await counter._invoke("current", {})
    else:
        assert await counter._invoke("add", {"amount": 3}) == 15
    counter.close()
    assert _live_handle_count() == baseline
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
async def test_projection_uses_receivers_sdk_after_default_map_changes(runtime):
    baseline = _live_handle_count()
    greeter = call(runtime, "make_greeter", prefix="Hello")
    set_type_map(BamlTypeMap())
    pending = greeter.as_interface(GreeterRef)
    greeter.close()
    selected = await pending
    assert type(selected) is GreeterRef
    assert await selected.greet("Ada") == "Hello, Ada!"
    selected.close()
    assert _pending_transfer_count() == 0
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
async def test_projection_failures_release_ownership_with_global_runtime_open(runtime):
    baseline = _live_handle_count()
    greeter = call(runtime, "make_greeter", prefix="Hello")
    retained = _live_handle_count()
    for _ in range(20):
        with pytest.raises(TypeError):
            await greeter.as_interface(CounterRef)
        selected = await greeter.as_interface(GreeterRef)
        assert await selected.label() == "greeter"
        selected.close()
        assert _pending_transfer_count() == 0
        assert _live_handle_count() == retained
    greeter.close()
    assert _live_handle_count() == baseline
    # Release cannot depend on shutting down the process-wide runtime.
    another = call(runtime, "make_greeter", prefix="Still open")
    assert await another.greet("Ada") == "Still open, Ada!"
    another.close()
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
async def test_projection_rejects_replaced_issuer_before_using_new_runtime(runtime):
    greeter = call(runtime, "make_greeter", prefix="Original")
    BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    with pytest.raises(BamlError, match="closed or replaced"):
        await greeter.as_interface(GreeterRef)
    greeter.close()
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
async def test_wrong_arguments_reject_before_receiver_mutation(runtime):
    counter = call(runtime, "make_counter", initial=3)
    with pytest.raises((TypeError, BamlError)):
        await counter._invoke("add", {"amount": "wrong"})
    with pytest.raises((TypeError, BamlError)):
        await counter._invoke("missing_member", {})
    assert await counter._invoke("current", {}) == 3


@pytest.mark.asyncio
async def test_generic_method_uses_explicit_checked_specialization(runtime):
    echo = call(runtime, "make_echo")
    string = proto.python_type_to_wire_ty(str)
    assert await echo._invoke("echo", {"value": "Ada"}, (string,)) == "Ada"
    with pytest.raises((TypeError, BamlError)):
        await echo._invoke("echo", {"value": 42}, (string,))
    # Omission is currently rejected by the shared method target. Generated
    # inference/specialization ergonomics remain separate implementation work.
    with pytest.raises((TypeError, BamlError)):
        await echo._invoke("echo", {"value": "Ada"})


@pytest.mark.asyncio
async def test_associated_error_pin_is_not_erased_by_identical_method_results(runtime):
    source = call(runtime, "make_source")
    fallible = call(runtime, "make_fallible_source")
    assert await source._invoke("read", {}) == "Ada"
    assert await fallible._invoke("read", {}) == "Ada"
    assert call(runtime, "read_infallible", value=source) == "Ada"
    with pytest.raises(TypeError):
        call(runtime, "read_infallible", value=fallible)
    assert await fallible._invoke("read", {}) == "Ada"


@pytest.mark.asyncio
async def test_nonclass_array_and_string_interface_receivers(runtime):
    for name, kwargs in (
        ("as_string_iterable", {"values": ["a", "b"]}),
        ("as_character_iterable", {"value": "ab"}),
    ):
        items = call(runtime, name, **kwargs)
        iterator = await items._invoke("iter", {})
        items.close()
        assert await iterator._invoke("next", {}) == "a"
        assert call(runtime, "next_in_baml", value=iterator) == "b"
        assert isinstance(await iterator._invoke("next", {}), Done)


@pytest.mark.asyncio
async def test_nullable_iterator_preserves_null_item_separately_from_done(runtime):
    iterator = call(runtime, "nullable_iterator", values=[None, "Ada"])
    assert await iterator._invoke("next", {}) is None
    assert await iterator._invoke("next", {}) == "Ada"
    assert isinstance(await iterator._invoke("next", {}), Done)


@pytest.mark.asyncio
async def test_method_results_use_captured_sdk_typemap(runtime):
    relay = call(runtime, "make_relay")
    set_type_map(BamlTypeMap())
    greeter = await relay._invoke("next", {})
    assert isinstance(greeter, GreeterRef)
    assert await greeter.greet("Ada") == "Hello, Ada!"


@pytest.mark.asyncio
async def test_method_error_adopts_live_interface_before_raising(runtime):
    relay = call(runtime, "make_relay")
    with pytest.raises(BamlError) as raised:
        await relay._invoke("fail", {})
    greeter = raised.value.value
    relay.close()
    assert isinstance(greeter, GreeterRef)
    assert await greeter.label() == "greeter"
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("replace", [False, True])
async def test_closed_issuer_is_not_replaced_by_current_global_runtime(
    runtime, replace
):
    counter = call(runtime, "make_counter", initial=3)
    if replace:
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    else:
        shutdown_runtime()
    with pytest.raises(BamlError, match="closed or replaced"):
        await counter._invoke("add", {"amount": 7})
    counter.close()


def test_invalid_decoder_registration_discards_interface_lease(runtime):
    baseline = _live_handle_count()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={},
            enums={},
            type_aliases={},
            interface_refs={"user.Greeter": ("builtins", "str")},
        )
    )
    result = runtime.call_function_sync(request("make_greeter", prefix="Hello"))
    with pytest.raises(BamlError, match="not an SDK reference"):
        proto.decode_call_result(result)
    assert _live_handle_count() == baseline
    assert _pending_transfer_count() == 0


def test_encoding_interface_rolls_back_on_later_invalid_value(runtime):
    greeter = call(runtime, "make_greeter", prefix="Hello")
    baseline = _live_handle_count()
    with pytest.raises(TypeError):
        request("unused", first=greeter, invalid=object())
    assert _live_handle_count() == baseline
    assert call(runtime, "greeter_label", value=greeter) == "greeter"


def test_missing_event_loop_releases_prepared_callback_arguments(runtime):
    class Callback:
        def __call__(self, value: str) -> str:
            return value

    greeter = call(runtime, "make_greeter", prefix="Hello")
    callback = Callback()
    weak = weakref.ref(callback)
    # Preparation consumes argument registrations before scheduling. This
    # fails at Python future construction, before even resolving the member.
    with pytest.raises(RuntimeError, match="no running event loop"):
        greeter._invoke("label", {"unused": callback})
    del callback
    gc.collect()
    assert weak() is None, (
        "scheduling failure must drain releases without a later SDK call"
    )


@pytest.mark.asyncio
async def test_retained_callable_uses_its_sdk_and_original_handle(runtime, monkeypatch):
    baseline = _live_handle_count()
    factory = call(runtime, "make_counter_factory", initial=10)
    cloned = copy.copy(factory)
    factory.close()
    set_type_map(BamlTypeMap())
    counter = cloned()
    assert type(counter) is CounterRef
    assert await counter._invoke("add", {"amount": 2}) == 12

    def unexpected_registration(_callable):
        raise AssertionError("a BAML callable must keep its native handle")

    monkeypatch.setattr(proto, "register_host_callable", unexpected_registration)
    returned = proto.decode_call_result(
        runtime.call_function_sync(request("call_counter_factory", factory=cloned)),
        type_map=counter._type_map,
    )
    assert type(returned) is CounterRef
    assert await returned._invoke("add", {"amount": 3}) == 15
    assert await counter._invoke("current", {}) == 15
    cloned.close()
    counter.close()
    returned.close()
    assert _live_handle_count() == baseline
    assert _pending_transfer_count() == 0


def test_retained_callable_rejects_replaced_runtime(runtime):
    factory = call(runtime, "make_counter_factory", initial=10)
    BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    with pytest.raises(BamlError, match="closed or replaced"):
        factory()
    factory.close()
    assert _pending_transfer_count() == 0
