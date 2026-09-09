"""Marked lines must fail native checking; associated choices remain exact."""

from typing_extensions import Never

from baml_sdk import CounterValueRef, UnspecifiedCounterRef


async def check(original: UnspecifiedCounterRef) -> None:
    await original.as_interface(str)  # expected-error: target must be an interface Ref
    selected = await original.as_interface(CounterValueRef[int, Never])
    await selected.update("wrong")  # expected-error: selected Value is int
    text: str = await selected.update(1)  # expected-error: result is int
    print(text)
