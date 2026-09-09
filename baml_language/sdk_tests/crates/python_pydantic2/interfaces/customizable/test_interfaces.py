"""The same receiver and method assertions run through every native bridge."""

import asyncio
import copy
from collections import UserList
from types import MappingProxyType

import baml_bridge
import baml_sdk
import pytest
from typing_extensions import Never

from baml_bridge import proto
from baml_bridge.baml_py import _live_handle_count
from baml_bridge.errors import BamlError


async def test_host_implementation_explicit_binding():
    class Host:
        def __init__(self):
            self.names = []

        async def greet(self, name: str) -> str:
            self.names.append(name)
            return f"Hello, {name}!"

    implementation = Host()
    greeter = await baml_sdk.GreeterRef.bind(implementation)
    returned = await baml_sdk.pass_greeter_async(greeter)
    assert await greeter.greet("Ada") == "Hello, Ada!"
    assert await returned.label() == "greeter"
    greeter.close()
    assert await baml_sdk.welcome_async(returned, "Grace") == "Hello, Grace!"
    assert implementation.names == ["Ada", "Grace"]
    returned.close()


async def test_host_implementation_required_interfaces():
    class Host:
        def __init__(self):
            self.count = 10

        def add(self, amount: int) -> int:
            self.count += amount
            return self.count

        def current(self) -> int:
            return self.count

    implementation = Host()
    counter = await baml_sdk.ExtendedCounterRef.bind(implementation)
    assert await counter.add(2) == 12
    assert await baml_sdk.add_in_baml_async(counter, 5) == 17
    assert await counter.current() == 17
    assert implementation.count == 17
    counter.close()


async def test_checked_interface_projection_rejects_wrong_bindings():
    original = await baml_sdk.make_text_decoder_async()
    with pytest.raises(TypeError, match="inbound value does not satisfy"):
        await original.as_interface(baml_sdk.DecoderRef[int, Never])
    with pytest.raises(TypeError, match="inbound value does not satisfy"):
        await original.as_interface(baml_sdk.DecoderRef[str, str])
    selected = await original.as_interface(baml_sdk.DecoderRef[str, Never])
    selected.close()
    original.close()


async def test_baml_receiver_roundtrip():
    greeter = await baml_sdk.make_greeter_async("Hello")
    returned = await baml_sdk.pass_greeter_async(greeter)
    assert await greeter.greet("Ada") == "Hello, Ada!"
    assert await returned.greet("Grace") == "Hello, Grace!"
    assert await baml_sdk.welcome_async(returned, "Lin") == "Hello, Lin!"


async def test_concrete_implements_interface_input():
    greeter = await baml_sdk.FriendlyGreeter.new_async(prefix="Hello")
    assert type(greeter) is baml_sdk.FriendlyGreeter
    assert await greeter.greet("Ada") == "Hello, Ada!"
    assert await greeter.label() == "greeter"
    assert await baml_sdk.welcome_async(greeter, "Ada") == "Hello, Ada!"


async def test_default_method_dispatch():
    greeter = await baml_sdk.make_greeter_async("Hello")
    assert await greeter.label() == "greeter"
    assert await baml_sdk.greeter_label_async(greeter) == "greeter"


async def test_owner_state_survives_method_calls():
    counter = await baml_sdk.make_counter_async(10)
    returned = await baml_sdk.pass_counter_async(counter)
    assert await counter.add(2) == 12
    assert await baml_sdk.add_in_baml_async(returned, 5) == 17
    assert await counter.current() == 17
    assert await returned.current() == 17


async def test_record_copy_preserves_live_child():
    counter = await baml_sdk.make_counter_async(10)
    record = await baml_sdk.counter_record_async(counter)
    record.title = "local edit"
    assert await record.counter.add(2) == 12
    assert await counter.current() == 12
    fresh = await baml_sdk.counter_record_async(counter)
    assert fresh.title == "counter"
    assert await fresh.counter.current() == 12


async def test_copied_record_implements_interface_input():
    record = await baml_sdk.marker_record_async("initial")
    assert type(record) is baml_sdk.MarkedRecord
    retained = await baml_sdk.pass_marker_async(record)
    record.text = "local edit"
    assert await baml_sdk.marker_text_async(retained) == "initial"
    assert await baml_sdk.marker_text_async(record) == "local edit"
    retained.close()


async def test_inherited_method_preserves_receiver_state():
    counter = await baml_sdk.make_extended_counter_async(10)
    assert await counter.add(2) == 12
    parent = await baml_sdk.pass_counter_async(counter)
    assert await parent.add(3) == 15
    assert await counter.current() == 15
    counter.close()
    assert await parent.current() == 15
    parent.close()


async def test_inherited_call_survives_reference_release():
    counter = await baml_sdk.make_extended_counter_async(10)
    pending = counter.add(2)
    counter.close()
    assert await pending == 12


async def test_checked_interface_projection_preserves_receiver():
    original = await baml_sdk.make_unspecified_counter_async(10)
    checked = await original.as_interface(baml_sdk.CounterValueRef[int, Never])
    assert await checked.update(2) == 12
    with pytest.raises(TypeError):
        await original.as_interface(baml_sdk.CounterValueRef[str, Never])
    with pytest.raises(TypeError):
        await original.as_interface(baml_sdk.CounterValueRef[int, str])
    repeated = await original.as_interface(baml_sdk.CounterValueRef[int, Never])
    original.close()
    checked.close()
    assert await repeated.update(3) == 15
    repeated.close()


async def test_checked_interface_projection_survives_reference_release():
    original = await baml_sdk.make_unspecified_counter_async(10)
    pending = original.as_interface(baml_sdk.CounterValueRef[int, Never])
    original.close()
    checked = await pending
    assert await checked.update(2) == 12
    checked.close()


async def test_callback_argument_preserves_receiver():
    received = []
    baseline = _live_handle_count()

    async def update(counter):
        assert type(counter) is baml_sdk.CounterRef
        received.append(counter)
        return await counter.add(2)

    assert await baml_sdk.visit_counter_async(10, update) == 12
    counter = received.pop()
    assert await counter.add(3) == 15
    assert await baml_sdk.add_in_baml_async(counter, 5) == 20
    counter.close()
    assert _live_handle_count() == baseline


async def test_spawned_interface_error_preserves_receiver(monkeypatch):
    received = []
    baseline = _live_handle_count()

    # Intercept the default terminating reporter, retaining the actual native
    # receipt. Decoding below must produce this SDK's generated CounterRef.
    def capture_report(result, **kwargs):
        if kwargs:
            # Ordinary generated calls supply their captured SDK type map.
            return proto.decode_call_result(result, **kwargs)
        received.append(result)

    monkeypatch.setattr(baml_bridge, "decode_call_result", capture_report)
    try:
        assert await baml_sdk.report_counter_async(10) == 1
        for _ in range(100):
            if received:
                break
            await asyncio.sleep(0.01)
            await baml_sdk.baml.sys.collect_garbage_async()
        assert len(received) == 1
        with pytest.raises(BamlError) as raised:
            proto.decode_call_result(received[0])
        counter = raised.value.value
        assert type(counter) is baml_sdk.CounterRef
        assert await counter.add(2) == 12
        assert await baml_sdk.add_in_baml_async(counter, 3) == 15
        counter.close()
        assert _live_handle_count() == baseline
    finally:
        for result in received:
            result._discard()


async def test_callable_reference_preserves_interface_receiver():
    baseline = _live_handle_count()
    factory = await baml_sdk.make_counter_factory_async(10)
    clone = copy.copy(factory)
    factory.close()
    counter = clone()
    assert type(counter) is baml_sdk.CounterRef
    assert await counter.add(2) == 12
    returned = await baml_sdk.call_counter_factory_async(clone)
    assert await returned.add(3) == 15
    assert await counter.current() == 15
    clone.close()
    returned.close()
    counter.close()
    assert _live_handle_count() == baseline


async def test_async_callback_alias_preserves_receiver():
    async def update(counter):
        assert type(counter) is baml_sdk.CounterRef
        return await counter.add(2)

    assert await baml_sdk.visit_counter_alias_async(10, update) == 12


async def test_optional_callback_preserves_keyword_arguments():
    seen = []

    async def update(counter, *, amount=1):
        seen.append(amount)
        return await counter.add(amount)

    assert await baml_sdk.visit_counter_optional_async(10, update) == 23
    assert seen == [1, 2]


async def test_interface_container_inputs_accept_implementations():
    counter = await baml_sdk.make_stored_counter_async(10)
    assert type(counter) is baml_sdk.StoredCounter
    assert await baml_sdk.add_counter_list_async(UserList([counter]), 2) == [12]
    assert await baml_sdk.add_counter_list_async((counter,), 3) == [15]
    assert await baml_sdk.add_counter_map_async(
        MappingProxyType({"counter": counter}), 4
    ) == [19]
    counter.close()


async def test_nested_callback_preserves_interface_receiver():
    async def consume(factory):
        counter = factory()
        try:
            assert type(counter) is baml_sdk.CounterRef
            return await counter.add(2)
        finally:
            counter.close()

    assert await baml_sdk.use_counter_factory_callback_async(consume) == 12


async def test_generic_callback_preserves_optional_type_relationship():
    async def choose(value: int, *, other: int = 0) -> int:
        return value + other

    assert (
        await baml_sdk.visit_generic_callback_async(3, choose, _types={"T": int}) == 6
    )


async def test_awaitable_callback_produces_interface_result():
    class ProducedCounter:
        def __await__(self):
            async def produce():
                return await baml_sdk.make_stored_counter_async(10)

            return produce().__await__()

    counter = await baml_sdk.call_counter_factory_async(lambda: ProducedCounter())
    assert type(counter) is baml_sdk.CounterRef
    assert await counter.add(2) == 12
    counter.close()


async def test_interface_method_callback_preserves_optional_types():
    async def update(counter: baml_sdk.CounterRef, *, amount: int = 1) -> int:
        return await counter.add(amount)

    async def choose(value: int, *, other: int = 0) -> int:
        return value + other

    runner = await baml_sdk.make_counter_runner_async(10)
    assert await runner.visit(update) == 12
    assert await runner.choose(3, choose, _types={"T": int}) == 6
    runner.close()


async def test_generic_interface_method_preserves_type_arguments():
    bounded = await baml_sdk.make_bounded_method_async()
    try:
        assert (
            await bounded.check(_types={"T": baml_sdk.MarkedRecord, "U": int})
            == "checked"
        )
    finally:
        bounded.close()
    echo = await baml_sdk.make_echo_async()
    counter = await baml_sdk.make_counter_async(10)
    record = await baml_sdk.counter_record_async(counter)
    try:
        assert await echo.echo("Ada", _types={"T": str}) == "Ada"
        flavor = await echo.echo(
            baml_sdk.InterfaceFlavor.Vanilla, _types={"T": baml_sdk.InterfaceFlavor}
        )
        assert flavor is baml_sdk.InterfaceFlavor.Vanilla
        assert await echo.echo(
            [flavor], _types={"T": list[baml_sdk.InterfaceFlavor]}
        ) == [flavor]
        assert (
            await echo.echo(flavor, _types={"T": baml_sdk.InterfaceFlavor | None})
            is flavor
        )
        assert (
            await echo.echo(None, _types={"T": baml_sdk.InterfaceFlavor | None}) is None
        )
        copied = await echo.echo(record, _types={"T": baml_sdk.CounterRecord})
        try:
            assert type(copied) is baml_sdk.CounterRecord
            assert await copied.counter.add(2) == 12
            assert await counter.current() == 12
        finally:
            copied.counter.close()
    finally:
        record.counter.close()
        counter.close()
        echo.close()


async def test_generic_interface_method_rejects_wrong_type_arguments():
    bounded = await baml_sdk.make_bounded_method_async()
    try:
        with pytest.raises(TypeError):
            await bounded.check(_types={"T": int, "U": int})
    finally:
        bounded.close()
    echo = await baml_sdk.make_echo_async()
    try:
        with pytest.raises(TypeError, match="does not satisfy"):
            await echo.echo(42, _types={"T": str})
        with pytest.raises(TypeError):
            await echo.echo("Missing", _types={"T": baml_sdk.InterfaceFlavor})
        assert await echo.echo("Ada", _types={"T": str}) == "Ada"
    finally:
        echo.close()
