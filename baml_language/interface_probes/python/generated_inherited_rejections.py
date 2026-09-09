"""Each marked line must fail against the generated inherited caller signatures."""

from baml_sdk import RequiredRootRef, pass_iterable_async
from baml_sdk.baml.iter import IteratorRef


async def check(root: RequiredRootRef[str], iterator: IteratorRef[str, str]) -> None:
    await root.apply(42)  # expected-error: inherited argument is string
    number: int = await root.apply("Ada")  # expected-error: associated result is string
    await pass_iterable_async(iterator)  # expected-error: required Error is never
    print(number)
