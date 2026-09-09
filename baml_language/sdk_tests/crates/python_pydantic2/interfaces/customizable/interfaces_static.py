"""Static caller contract, checked by the shared fixture's pyright step."""

from typing_extensions import Never, assert_type

from baml_sdk import (
    CounterRef,
    DecoderRef,
    GreeterRef,
    GreeterHost,
    ExtendedCounterHost,
    ExtendedCounterRef,
    counter_record_async,
    make_counter_async,
    make_greeter_async,
    make_text_decoder_async,
)


async def check_interface_callers() -> None:
    greeter = await make_greeter_async("Hello")
    assert_type(greeter, GreeterRef)
    assert_type(await greeter.greet("Ada"), str)
    assert_type(await greeter.label(), str)

    counter = await make_counter_async(3)
    assert_type(counter, CounterRef)
    assert_type(await counter.add(4), int)
    record = await counter_record_async(counter)
    assert_type(record.counter, CounterRef)

    decoder = await make_text_decoder_async()
    assert_type(decoder, DecoderRef[str, Never])


async def check_host_binders(implementation: GreeterHost, counter: ExtendedCounterHost) -> None:
    greeter = await GreeterRef.bind(implementation)
    assert_type(greeter, GreeterRef)
    assert_type(await greeter.greet("Ada"), str)
    extended = await ExtendedCounterRef.bind(counter)
    assert_type(extended, ExtendedCounterRef)
    assert_type(await extended.current(), int)
