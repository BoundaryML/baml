"""Each marked expression must fail native typing against the generated SDK."""

from typing_extensions import Never

from baml_sdk import (
    FriendlyGreeter,
    MapperRef,
    use_int_mapper_async,
    use_text_mapper_async,
    use_tagged_async,
)


class MethodShaped:
    async def map(self, value: int) -> int:
        return value


async def reject(
    wrong_error: MapperRef[int, Never],
    wrong_item: MapperRef[str, str],
    unrelated: FriendlyGreeter,
    shaped: MethodShaped,
) -> None:
    await use_int_mapper_async(wrong_error)  # expected-error
    await use_int_mapper_async(wrong_item)  # expected-error
    await use_text_mapper_async(unrelated)  # expected-error
    await use_int_mapper_async(shaped)  # expected-error
    await use_tagged_async(unrelated)  # expected-error
