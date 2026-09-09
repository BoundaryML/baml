"""Checked projection of an unpinned requirement into a fully typed caller."""

import asyncio

from typing_extensions import Never, assert_type

import baml_sdk as b
from baml_bridge.baml_py import _pending_transfer_count  # pyright: ignore[reportPrivateUsage]


async def check() -> None:
    original = await b.required_unpinned_async()
    selected = await original.as_interface(b.RequiredBaseRef[str, str, Never])
    assert_type(selected, b.RequiredBaseRef[str, str, Never])
    assert_type(await selected.apply("Ada"), str)
    assert await selected.apply("Ada") == "Ada"
    assert "as_interface" in (type(original).__doc__ or "")

    class UnregisteredSubclass(b.RequiredBaseRef[str, str, Never]):
        pass

    try:
        original.as_interface(UnregisteredSubclass)
    except TypeError as error:
        assert "this reference's SDK" in str(error)
    else:
        raise AssertionError("a target subclass is not the generated return facade")

    pending = original.as_interface(b.RequiredBaseRef[str, str, Never])
    original.close()
    selected.close()
    retained = await pending
    assert await retained.apply("Grace") == "Grace"
    retained.close()
    assert _pending_transfer_count() == 0


if __name__ == "__main__":
    asyncio.run(check())
