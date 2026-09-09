"""Execute generated concrete APIs, including class/default-method frames.

Generate the shared interfaces fixture with ../baml/concrete_facades.baml
added to baml_src, then run with that generated SDK on PYTHONPATH.
"""

from __future__ import annotations

import asyncio
import copy
import gc

import baml_sdk as sdk
from baml_sdk.ai import Agent
from baml_sdk.vendor.openai import ResponsesClient
from baml_bridge import proto
from baml_bridge.baml_py import _pending_transfer_count
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


async def main() -> None:
    # Real stdlib factories and a concrete client input; no network request.
    client = await ResponsesClient.new_async(model="facade-probe", api_key="unused")
    assert type(client) is ResponsesClient
    assert await client.id() == "openai/facade-probe"
    agent = await Agent.new_async(client=client)
    assert type(agent) is Agent
    agent.close()
    client.close()

    greeter = await sdk.FriendlyGreeter.new_async(prefix="Hello")
    assert type(greeter) is sdk.FriendlyGreeter
    assert await greeter.greet("Ada") == "Hello, Ada!"
    assert await greeter.label() == "greeter"
    assert await sdk.welcome_async(greeter, "Ada") == "Hello, Ada!"
    assert not hasattr(greeter, "prefix")
    pending = greeter.greet("Grace")
    greeter.close()
    assert await pending == "Hello, Grace!"

    box = await sdk.facade_box_async()
    assert type(box) is sdk.FacadeBox
    assert not hasattr(box, "value")
    assert await box.read() == "Ada"
    assert await box.replace("Grace") == "Grace"
    assert await box.read() == "Grace"
    assert await box.echo(9, _types={"U": int}) == 9
    same = await box.same()
    assert type(same) is sdk.FacadeBox
    assert await same.replace("Lin", marker=1) == "Lin"
    assert await box.read() == "Lin"
    record = await sdk.facade_record_async(box)
    assert isinstance(record.value, sdk.FacadeBox)
    assert await record.value.read() == "Lin"

    saved = get_type_map()
    set_type_map(BamlTypeMap.from_lazy_entries({}, {}, {}))
    try:
        # A method's type tokens and record codecs come from its captured SDK.
        echoed = await box.echo(
            record, _types={"U": sdk.FacadeRecord[sdk.FacadeBox[str]]}
        )
        assert isinstance(echoed.value, sdk.FacadeBox)
        assert await echoed.value.read() == "Lin"
    finally:
        set_type_map(saved)

    try:
        await box.replace(9)
    except TypeError:
        pass
    else:
        raise AssertionError("wrong concrete type reached the body")
    assert await same.read() == "Lin"
    sibling = copy.copy(box)
    box.close()
    assert await sibling.read() == "Lin"
    assert await same.to_data_async() == {"value": "Lin"}
    assert (
        proto._ty_to_python_type(
            proto.python_type_to_wire_ty(sdk.FacadeBox[str]), saved
        )
        == sdk.FacadeBox[str]
    )
    sibling.close()
    same.close()
    record.value.close()
    echoed.value.close()
    gc.collect()
    assert _pending_transfer_count() == 0
    print("generated concrete facade methods, generic records and ownership passed")


if __name__ == "__main__":
    asyncio.run(main())
