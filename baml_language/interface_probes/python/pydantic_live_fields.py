"""Simulate proposed async live-field adapters with real Pydantic validation.

This is a host-side model experiment, not an execution of the unimplemented
general-interface ABI. Run from ``baml_language`` with:

    sdks/python/.venv/bin/python interface_probes/python/pydantic_live_fields.py
"""

from __future__ import annotations

import asyncio
import copy
from dataclasses import dataclass

import pydantic


@dataclass
class Item:
    label: str


class CounterHost(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(validate_assignment=True, strict=True)

    count: int
    items: list[Item]


class CounterAdapter:
    """Generated-adapter shape: retain the host, never cache its fields."""

    __slots__ = ("_host",)
    _count = pydantic.TypeAdapter(int, config=pydantic.ConfigDict(strict=True))
    _items = pydantic.TypeAdapter(list[Item], config=pydantic.ConfigDict(strict=True))

    def __init__(self, host: CounterHost) -> None:
        self._host = host

    async def get_count(self) -> int:
        return self._count.validate_python(self._host.count)

    async def set_count(self, value: int) -> None:
        checked = self._count.validate_python(value)
        self._host.count = checked

    async def get_items(self) -> list[Item]:
        # Deliberately rejected design: a copied aggregate cannot be a live field.
        # Validate and snapshot here to expose the missing nested mutation.
        checked = self._items.validate_python(self._host.items)
        return copy.deepcopy(checked)


class CounterRef:
    """Generated-proxy shape exposing only explicit async field operations."""

    __slots__ = ("_adapter",)

    def __init__(self, adapter: CounterAdapter) -> None:
        object.__setattr__(self, "_adapter", adapter)

    def __setattr__(self, name: str, value: object) -> None:
        if name == "count":
            raise AttributeError("live field 'count' requires: await ref.set_count(value)")
        raise AttributeError(f"cannot assign generated live reference attribute {name!r}")

    async def get_count(self) -> int:
        return await self._adapter.get_count()

    async def set_count(self, value: int) -> None:
        await self._adapter.set_count(value)

    async def get_items(self) -> list[Item]:
        return await self._adapter.get_items()


async def main() -> None:
    host = CounterHost(count=1, items=[Item("first")])
    adapter = CounterAdapter(host)
    ref = CounterRef(adapter)
    assert adapter._host is host

    host.count = 2
    assert await ref.get_count() == 2
    await ref.set_count(3)
    assert host.count == 3

    # Native code can bypass Pydantic's assignment hook. Validate on reads too.
    object.__setattr__(host, "count", "invalid")
    try:
        await ref.get_count()
    except pydantic.ValidationError as error:
        invalid_read = error.errors()[0]["type"]
    else:
        raise AssertionError("getter accepted invalid host state")
    object.__setattr__(host, "count", 3)

    # There is no syntax for awaiting an attribute assignment in Python.
    try:
        compile("async def bad(ref):\n    await ref.count = 4\n", "<probe>", "exec")
    except SyntaxError as error:
        assignment_syntax = error.msg
    else:
        raise AssertionError("awaited attribute assignment unexpectedly compiled")
    try:
        ref.count = 4
    except AttributeError as error:
        proxy_rejection = str(error)
    else:
        raise AssertionError("plain assignment shadowed a live field")

    snapshot = await ref.get_items()
    snapshot.append(Item("local-only"))
    assert [item.label for item in snapshot] == ["first", "local-only"]
    assert [item.label for item in host.items] == ["first"]

    # validate_assignment does not see in-place mutations of nested containers.
    host.items.append("invalid")  # type: ignore[arg-type]
    try:
        await ref.get_items()
    except pydantic.ValidationError as error:
        invalid_nested_read = error.errors()[0]["type"]
    else:
        raise AssertionError("getter accepted an invalid in-place list mutation")
    host.items.pop()

    print(
        {
            "same_host_object": adapter._host is host,
            "host_count_after_bridge_set": host.count,
            "invalid_get_rejected_as": invalid_read,
            "await_assignment": assignment_syntax,
            "plain_assignment": proxy_rejection,
            "snapshot_items": [item.label for item in snapshot],
            "host_items": [item.label for item in host.items],
            "invalid_nested_read_rejected_as": invalid_nested_read,
        }
    )


if __name__ == "__main__":
    asyncio.run(main())
