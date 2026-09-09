"""Every marked call must fail; unspecified associated types stay unspecified."""

from typing_extensions import Never

from baml_sdk import (
    RequiredBaseInput,
    RequiredMiddleRef,
    RequiredRootRef,
    RequiredUnpinnedRef,
    pass_iterable_async,
)
from baml_sdk.baml.iter import IteratorRef


def base(value: RequiredBaseInput[str, str, Never]) -> None:
    pass


async def reject(
    error: RequiredMiddleRef[str, str],
    item: RequiredRootRef[int],
    unpinned: RequiredUnpinnedRef,
    iterator_error: IteratorRef[str, str],
    iterator_item: IteratorRef[int, Never],
) -> None:
    base(error)  # expected-error
    base(item)  # expected-error
    base(unpinned)  # expected-error
    await pass_iterable_async(iterator_error)  # expected-error
    await pass_iterable_async(iterator_item)  # expected-error
