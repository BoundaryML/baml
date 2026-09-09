"""Execute against the generated shared fixture plus pinned required-interface probe."""

import asyncio

from typing_extensions import Never, assert_type

import baml_sdk as b
from baml_sdk.baml.iter import IteratorRef
from baml_bridge.baml_py import _pending_transfer_count  # pyright: ignore[reportPrivateUsage]


async def check() -> None:
    root = await b.required_root_async()
    assert_type(await root.apply("Ada"), str)
    assert await root.apply("Ada") == "Ada"
    assert_type(await root.echo(42, _types={"U": int}), int)
    assert await root.echo(42, _types={"U": int}) == 42
    pending = root.apply("Grace")
    root.close()
    assert await pending == "Grace"

    items = await b.as_string_iterable_async(["Ada", "Grace"])
    iterator = await items.iter()
    repeated = await iterator.iter()
    assert_type(repeated, IteratorRef[str, Never])
    assert await iterator.next() == "Ada"
    iterator.close()
    items.close()
    assert await repeated.next() == "Grace"
    repeated.close()
    assert _pending_transfer_count() == 0


if __name__ == "__main__":
    asyncio.run(check())
