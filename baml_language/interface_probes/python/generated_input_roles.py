"""Directional inputs against the generated shared interfaces SDK.

Check with strict Pyright. No casts, suppression, or private helper imports:
the native caller should use ordinary callbacks and container values.
"""

from collections import UserList
from collections.abc import Awaitable, Callable
from types import MappingProxyType

from typing_extensions import assert_type

import baml_sdk as b


async def update(counter: b.CounterRef, *, amount: int = 1) -> int:
    return await counter.add(amount)


async def choose(value: int, *, other: int = 0) -> int:
    return value + other


async def consume(factory: Callable[[], b.CounterRef]) -> int:
    return await factory().add(2)


async def check(counter: b.StoredCounter) -> None:
    callback: Callable[[b.CounterRef], Awaitable[int]] = update
    assert_type(await b.visit_counter_async(10, callback), int)
    assert_type(await b.visit_counter_alias_async(10, update), int)
    assert_type(await b.visit_counter_optional_async(10, update), int)
    assert_type(await b.add_counter_list_async(UserList([counter]), 2), list[int])
    assert_type(await b.add_counter_list_async((counter,), 2), list[int])
    assert_type(
        await b.add_counter_map_async(MappingProxyType({"counter": counter}), 2),
        list[int],
    )
    assert_type(await b.use_counter_factory_callback_async(consume), int)
    assert_type(
        await b.visit_generic_callback_async(3, choose, _types={"T": int}), int
    )
    assert_type(await b.call_counter_factory_async(lambda: counter), b.CounterRef)
    assert_type(
        await b.call_counter_factory_async(lambda: b.make_stored_counter_async(10)),
        b.CounterRef,
    )
    factory = await b.make_counter_factory_async(10)
    assert_type(factory(), b.CounterRef)
    assert_type(await b.call_counter_factory_async(factory), b.CounterRef)
    runner = await b.make_counter_runner_async(10)
    assert_type(await runner.visit(update), int)
    assert_type(await runner.choose(3, choose, _types={"T": int}), int)


def check_output_aliases(
    callback: b.OptionalCounterVisitor, concrete: b.StoredCounter
) -> None:
    assert_type(callback(concrete), int)
    assert_type(callback(concrete, amount=2), int)
