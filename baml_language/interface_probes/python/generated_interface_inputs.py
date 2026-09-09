"""Execute the generated input roles; no cast/bind/as-interface is required."""

from __future__ import annotations

import asyncio
import inspect

import baml_sdk as sdk
from baml_bridge._concrete import BamlConcreteRef
from baml_bridge.baml_py import _pending_transfer_count
from baml_sdk.ai import Agent
from baml_sdk.vendor.openai import ResponsesClient


async def main() -> None:
    dual = await sdk.dual_mapper_async()
    assert type(dual) is sdk.DualMapper
    assert await sdk.use_int_mapper_async(dual) == 10
    assert await sdk.use_text_mapper_async(dual) == "Ada!"
    assert await sdk.optional_mapper_async(dual) == "Ada!"
    assert await sdk.optional_mapper_async(None) == "none"
    # Concrete method lookup is ambiguous. Inputs still admit both exact views.
    assert not hasattr(dual, "map")
    proof_names = [
        name for name in vars(sdk.DualMapper) if name.startswith("_baml_input_")
    ]
    assert proof_names
    for name in proof_names:
        assert callable(getattr(dual, name))
    record = sdk.TaggedRecord(name="Ada")
    assert not isinstance(record, BamlConcreteRef)
    assert await sdk.use_tagged_async(record) == "tagged"
    assert record.name == "Ada"

    # Copying a private typing marker cannot register a host implementation.
    class Forged:
        pass

    for name, method in vars(sdk.TaggedRecord).items():
        if name.startswith("_baml_input_"):
            setattr(Forged, name, method)
    try:
        await sdk.use_tagged_async(Forged())
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("typing evidence granted runtime conformance")
    client = await ResponsesClient.new_async(model="input-probe", api_key="unused")
    agent = await Agent.new_async(client=client)
    assert type(agent) is Agent
    assert await client.id() == "openai/input-probe"
    assert "MapperInput_" in sdk.__all__
    assert inspect.isclass(sdk.MapperInput)
    dual.close()
    agent.close()
    client.close()
    assert _pending_transfer_count() == 0
    print(
        "generated interface inputs: dual views, records, checked rejection and client factory passed"
    )


if __name__ == "__main__":
    asyncio.run(main())
