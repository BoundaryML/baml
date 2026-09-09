"""Concrete receiver transport using the real native runtime.

These tests cover ownership below the generated facade layer. They do not
claim the generic fallback has the public methods of a generated class.
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
from baml_bridge._concrete import BamlConcreteRef
from baml_bridge._interface import BamlInterfaceRef
from baml_bridge.baml_py import (
    BamlRuntime,
    _live_handle_count,
    _pending_transfer_count,
    new_function_call,
    shutdown_runtime,
)
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


SOURCE = (
    (
        Path(__file__).resolve().parents[3]
        / "sdk_tests/fixtures/interfaces/baml_src/main.baml"
    ).read_text()
    + """
function concrete_counter(initial: int) -> StoredCounter throws never {
    StoredCounter { count: initial }
}
class ConcreteRecord { title: string, counter: StoredCounter }
function concrete_record(value: StoredCounter) -> ConcreteRecord throws never {
    ConcreteRecord { title: "copied", counter: value }
}
function throw_concrete(value: StoredCounter) -> never throws StoredCounter { throw value }
interface ConcreteSource {
    type Output
    function read(self) -> Self.Output throws never
    function echo<U>(self, value: U) -> U throws never { value }
}
class ConcreteBox<T> {
    value: T,
    function replace(self, value: T) -> T throws never { self.value = value; self.value }
    function local_echo<U>(self, value: U) -> U throws never { value }
    implements ConcreteSource {
        type Output = T
        function read(self) -> T throws never { self.value }
    }
}
function concrete_box() -> ConcreteBox<string> throws never { ConcreteBox<string> { value: "Ada" } }
"""
)


class ConcreteRecord(BaseModel):
    title: str
    counter: BamlConcreteRef


@pytest.fixture
def runtime():
    saved = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={"user.ConcreteRecord": (__name__, "ConcreteRecord")},
            enums={},
            type_aliases={},
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


def test_factory_receiver_passes_directly_to_interface(runtime):
    greeter = call(runtime, "FriendlyGreeter.new", prefix="Hello")
    assert isinstance(greeter, BamlConcreteRef)
    assert not isinstance(greeter, BamlInterfaceRef)
    assert call(runtime, "welcome", value=greeter, name="Ada") == "Hello, Ada!"
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
async def test_concrete_copies_and_interface_views_share_state(runtime):
    baseline = _live_handle_count()
    counter = call(runtime, "concrete_counter", initial=3)
    cloned = copy.copy(counter)
    deep = copy.deepcopy(counter)
    view = call(runtime, "pass_counter", value=counter)
    counter.close()
    gc.collect()
    assert await view._invoke("add", {"amount": 4}) == 7
    assert call(runtime, "add_in_baml", value=cloned, amount=2) == 9
    assert deep.to_data() == {"count": 9}
    assert await deep.to_data_async() == {"count": 9}
    view.close()
    cloned.close()
    deep.close()
    assert _live_handle_count() == baseline
    with pytest.raises(RuntimeError, match="closed"):
        counter.to_data()


@pytest.mark.asyncio
async def test_prepared_owned_call_keeps_receiver_after_local_close(runtime):
    counter = call(runtime, "concrete_counter", initial=3)
    pending = counter._to_pyhandle()._call_owned_function(
        request("add_in_baml", value=counter, amount=4)
    )
    counter.close()
    assert proto.decode_call_result(await pending) == 7


def test_copied_record_keeps_concrete_child_live(runtime):
    counter = call(runtime, "concrete_counter", initial=3)
    record = call(runtime, "concrete_record", value=counter)
    assert isinstance(record, ConcreteRecord)
    assert record.title == "copied"
    child = record.counter
    assert isinstance(child, BamlConcreteRef)
    counter.close()
    assert call(runtime, "add_in_baml", value=child, amount=2) == 5
    assert child.to_data() == {"count": 5}
    record.title = "local"
    assert child.to_data() == {"count": 5}


def test_concrete_error_adopts_receiver_before_raising(runtime):
    counter = call(runtime, "concrete_counter", initial=3)
    with pytest.raises(BamlError) as raised:
        call(runtime, "throw_concrete", value=counter)
    counter.close()
    retained = raised.value.value
    assert isinstance(retained, BamlConcreteRef)
    assert call(runtime, "add_in_baml", value=retained, amount=2) == 5
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("replace", [False, True])
async def test_concrete_data_conversion_rejects_closed_issuer(runtime, replace):
    counter = call(runtime, "concrete_counter", initial=3)
    if replace:
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    else:
        shutdown_runtime()
    baseline = _live_handle_count()
    with pytest.raises(BamlError, match="closed or replaced"):
        counter.to_data()
    with pytest.raises(BamlError, match="closed or replaced"):
        await counter.to_data_async()
    assert _live_handle_count() == baseline
    if replace:
        assert _pending_transfer_count() == 0


def test_rejected_concrete_decode_discards_even_escaped_wrapper(runtime, monkeypatch):
    baseline = _live_handle_count()
    result = runtime.call_function_sync(request("concrete_counter", initial=3))
    escaped = []
    decode = proto._decode_handle

    def reject(handle, type_map):
        escaped.append(decode(handle, type_map))
        raise ValueError("reject concrete receiver")

    monkeypatch.setattr(proto, "_decode_handle", reject)
    with pytest.raises(ValueError, match="reject concrete receiver"):
        proto.decode_call_result(result)
    with pytest.raises(RuntimeError, match="not been adopted"):
        copy.copy(escaped[0])
    assert _live_handle_count() == baseline
    assert _pending_transfer_count() == 0


def test_concrete_encode_failure_releases_partial_clones(runtime):
    counter = call(runtime, "concrete_counter", initial=3)
    baseline = _live_handle_count()
    with pytest.raises(TypeError):
        request("unused", first=counter, invalid=object())
    assert _live_handle_count() == baseline
    assert counter.to_data() == {"count": 3}


def test_missing_event_loop_drains_owned_call_arguments(runtime):
    class Callback:
        def __call__(self, value: str) -> str:
            return value

    counter = call(runtime, "concrete_counter", initial=3)
    callback = Callback()
    weak = weakref.ref(callback)
    with pytest.raises(RuntimeError, match="no running event loop"):
        counter._to_pyhandle()._call_owned_function(
            request("unused", value=counter, callback=callback)
        )
    del callback
    gc.collect()
    assert weak() is None


def test_display_name_does_not_select_a_copied_class(runtime):
    # A decoder must not guess declaration identity from the diagnostic name.
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={"user.StoredCounter": ("builtins", "str")},
            enums={},
            type_aliases={},
        )
    )
    counter = call(runtime, "concrete_counter", initial=3)
    assert isinstance(counter, BamlConcreteRef)
    assert counter.to_data() == {"count": 3}


def interface_pattern(name: str):
    return proto.baml_type_pb2.BamlTy(
        **{"interface": proto.baml_type_pb2.BamlTyInterface(name=name)}
    )


@pytest.mark.asyncio
async def test_concrete_method_target_invokes_default_and_retains_prepared_receiver(
    runtime,
):
    greeter = call(runtime, "FriendlyGreeter.new", prefix="Hello")
    pattern = interface_pattern("user.Greeter")
    assert (
        await greeter._invoke_concrete(
            "user.FriendlyGreeter", pattern, "greet", {"name": "Ada"}
        )
        == "Hello, Ada!"
    )
    pending = greeter._invoke_concrete("user.FriendlyGreeter", pattern, "label", {})
    greeter.close()
    assert await pending == "greeter"
    counter = call(runtime, "concrete_counter", initial=3)
    assert (
        await counter._invoke_concrete(
            "user.StoredCounter",
            interface_pattern("user.Counter"),
            "add",
            {"amount": 4},
        )
        == 7
    )
    assert call(runtime, "add_in_baml", value=counter, amount=2) == 9


@pytest.mark.asyncio
async def test_concrete_method_rejection_cleans_up_callback_arguments(runtime):
    counter = call(runtime, "concrete_counter", initial=3)

    class Callback:
        def __call__(self):
            raise AssertionError("invalid target invoked a callback")

    callback = Callback()
    weak = weakref.ref(callback)
    with pytest.raises(TypeError, match="does not match the receiver's declaration"):
        await counter._invoke_concrete(
            "user.FriendlyGreeter",
            interface_pattern("user.Counter"),
            "add",
            {"amount": callback},
        )
    del callback
    gc.collect()
    assert weak() is None
    assert _pending_transfer_count() == 0
    assert counter.to_data() == {"count": 3}


def test_concrete_method_failed_scheduling_releases_prepared_arguments(runtime):
    counter = call(runtime, "concrete_counter", initial=3)

    class Callback:
        def __call__(self):
            raise AssertionError("unscheduled target invoked a callback")

    callback = Callback()
    weak = weakref.ref(callback)
    with pytest.raises(RuntimeError, match="no running event loop"):
        counter._invoke_concrete(
            "user.StoredCounter",
            interface_pattern("user.Counter"),
            "add",
            {"amount": callback},
        )
    del callback
    gc.collect()
    assert weak() is None


@pytest.mark.asyncio
async def test_concrete_method_uses_captured_class_slots_and_separate_method_arguments(
    runtime,
):
    box = call(runtime, "concrete_box")
    pattern = interface_pattern("user.ConcreteSource")
    # The compiler's class-slot recipe is interpreted by the engine. It is
    # not a host choice of the concrete object's T parameter.
    binding = getattr(pattern, "interface").bindings.add()
    binding.name = "Output"
    binding.ty.type_var.index = 0
    binding.ty.type_var.name = "$sdk$class$0"
    assert await box._invoke_concrete("user.ConcreteBox", pattern, "read", {}) == "Ada"
    assert (
        await box._invoke_concrete(
            "user.ConcreteBox",
            pattern,
            "echo",
            {"value": 9},
            (proto.python_type_to_wire_ty(int),),
        )
        == 9
    )
    binding.ty.type_var.index = 99
    with pytest.raises(TypeError, match="unresolved class arguments"):
        await box._invoke_concrete("user.ConcreteBox", pattern, "read", {})


@pytest.mark.asyncio
async def test_inherent_methods_keep_generic_receiver_state_and_pending_ownership(
    runtime,
):
    box = call(runtime, "concrete_box")
    with pytest.raises(TypeError):
        await box._invoke_concrete("user.ConcreteBox", None, "replace", {"value": 9})
    assert (
        await box._invoke_concrete(
            "user.ConcreteBox", None, "replace", {"value": "Grace"}
        )
        == "Grace"
    )
    assert box.to_data() == {"value": "Grace"}
    with pytest.raises(TypeError, match="no inherent method"):
        await box._invoke_concrete("user.ConcreteBox", None, "read", {})
    pending = box._invoke_concrete(
        "user.ConcreteBox",
        None,
        "local_echo",
        {"value": 9},
        (proto.python_type_to_wire_ty(int),),
    )
    box.close()
    assert await pending == 9
    assert _pending_transfer_count() == 0


def test_concrete_method_encoder_rolls_back_arguments_when_method_type_encoding_fails(
    runtime,
):
    counter = call(runtime, "concrete_counter", initial=3)

    class Callback:
        def __call__(self):
            raise AssertionError("unencoded call invoked a callback")

    callback = Callback()
    weak = weakref.ref(callback)
    with pytest.raises(TypeError):
        counter._invoke_concrete(
            "user.StoredCounter",
            interface_pattern("user.Counter"),
            "add",
            {"amount": callback},
            (object(),),
        )
    del callback
    gc.collect()
    assert weak() is None
    assert _pending_transfer_count() == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("replace", [False, True])
async def test_concrete_method_rejects_closed_issuer_and_releases_new_arguments(
    runtime, replace
):
    counter = call(runtime, "concrete_counter", initial=3)
    if replace:
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    else:
        shutdown_runtime()

    class Callback:
        def __call__(self):
            raise AssertionError("closed issuer invoked a callback")

    callback = Callback()
    weak = weakref.ref(callback)
    with pytest.raises(BamlError, match="closed or replaced"):
        await counter._invoke_concrete(
            "user.StoredCounter",
            interface_pattern("user.Counter"),
            "add",
            {"amount": callback},
        )
    del callback
    gc.collect()
    assert weak() is None


@pytest.mark.asyncio
async def test_concrete_method_uses_its_receiver_map_for_record_arguments_and_results(
    runtime,
):
    box = call(runtime, "concrete_box")
    counter = call(runtime, "concrete_counter", initial=3)
    record = ConcreteRecord(title="Ada", counter=counter)
    pattern = interface_pattern("user.ConcreteSource")
    binding = getattr(pattern, "interface").bindings.add()
    binding.name = "Output"
    binding.ty.CopyFrom(proto.python_type_to_wire_ty(str))
    record_type = proto.python_type_to_wire_ty(ConcreteRecord)
    original = get_type_map()
    unrelated = BamlTypeMap()
    set_type_map(unrelated)
    try:
        returned = await box._invoke_concrete(
            "user.ConcreteBox",
            pattern,
            "echo",
            {"value": record},
            (record_type,),
        )
        assert type(returned) is ConcreteRecord
        assert returned.title == "Ada"
        assert returned.counter.to_data() == {"count": 3}
        assert get_type_map() is unrelated
    finally:
        set_type_map(original)
