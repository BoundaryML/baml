"""Every marked call must fail strict typing against the generated SDK."""

from collections.abc import Callable

import baml_sdk as b


async def needs_concrete(counter: b.StoredCounter) -> int:
    return await counter.current()


async def wrong_result(counter: b.CounterRef) -> str:
    return str(await counter.current())


async def wrong_keyword(counter: b.CounterRef, *, extra: int = 1) -> int:
    return await counter.add(extra)


async def wrong_keyword_type(counter: b.CounterRef, *, amount: str = "1") -> int:
    return await counter.add(int(amount))


async def wrong_generic(value: str, *, other: str = "") -> str:
    return value + other


async def wrong_nested(factory: Callable[[], b.StoredCounter]) -> int:
    return await factory().current()


async def reject(unrelated: b.FriendlyGreeter) -> None:
    await b.visit_counter_alias_async(10, needs_concrete)  # expected-error
    await b.visit_counter_alias_async(10, wrong_result)  # expected-error
    await b.visit_counter_optional_async(10, wrong_keyword)  # expected-error
    await b.visit_counter_optional_async(10, wrong_keyword_type)  # expected-error
    await b.add_counter_list_async([unrelated], 2)  # expected-error
    await b.add_counter_map_async({"counter": unrelated}, 2)  # expected-error
    await b.call_counter_factory_async(lambda: unrelated)  # expected-error
    await b.use_counter_factory_callback_async(wrong_nested)  # expected-error
    await b.visit_generic_callback_async(3, wrong_generic, _types={"T": int})  # expected-error
    runner = await b.make_counter_runner_async(10)
    await runner.visit(wrong_keyword)  # expected-error
    await runner.choose(3, wrong_generic, _types={"T": int})  # expected-error


def reject_output_alias(
    callback: b.OptionalCounterVisitor, counter: b.CounterRef
) -> None:
    callback(counter, 2)  # expected-error
    callback(counter, extra=2)  # expected-error
    callback(counter, amount="2")  # expected-error
