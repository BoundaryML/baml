"""Required-interface inputs must preserve the compiler's exact bindings."""

from typing_extensions import Never, assert_type

from baml_sdk import (
    RequiredBaseInput,
    RequiredMiddleInput,
    RequiredMiddleRef,
    RequiredRootInput,
    RequiredRootRef,
    as_string_iterable_async,
    pass_iterable_async,
    required_middle_async,
    required_root_async,
    use_required_base_async,
)
from baml_sdk.baml.iter import IterableInput, IterableRef, IteratorInput, IteratorRef


def base(value: RequiredBaseInput[str, str, Never]) -> None:
    pass


def middle(value: RequiredMiddleInput[str, Never]) -> None:
    base(value)


def root(value: RequiredRootInput[str]) -> None:
    middle(value)
    base(value)


def iterator_input(value: IteratorInput[str, Never]) -> None:
    iterable_input(value)


def iterable_input(value: IterableInput[str, Never]) -> None:
    pass


async def check() -> None:
    first = await required_root_async()
    assert_type(first, RequiredRootRef[str])
    root(first)
    middle(first)
    base(first)
    assert_type(await use_required_base_async(first), str)
    second = await required_middle_async()
    assert_type(second, RequiredMiddleRef[str, Never])
    base(second)
    items = await as_string_iterable_async(["Ada"])
    iterator = await items.iter()
    assert_type(iterator, IteratorRef[str, Never])
    iterable_input(iterator)
    assert_type(await pass_iterable_async(iterator), IterableRef[str, Never])
