"""Native input acceptance, checked against actual generated SDK stubs.

Generate the shared interfaces fixture plus the BAML files in this probe.
This positive consumer must pass without casts or explicit projections.
"""

from typing_extensions import Never, assert_type

from baml_sdk import (
    DualMapper,
    FriendlyGreeter,
    MapperInput_,
    MapperRef,
    TaggedRecord,
    dual_mapper_async,
    optional_mapper_async,
    use_int_mapper_async,
    use_tagged_async,
    use_text_mapper_async,
    welcome_async,
)
from baml_sdk.ai import Agent
from baml_sdk.vendor.openai import ResponsesClient


def accepts_int(value: MapperInput_[int, str]) -> None:
    pass


def accepts_text(value: MapperInput_[str, Never]) -> None:
    pass


async def check_inputs(ref: MapperRef[int, str]) -> None:
    concrete = await FriendlyGreeter.new_async(prefix="Hello")
    assert_type(await welcome_async(concrete, "Ada"), str)
    dual = await dual_mapper_async()
    assert_type(dual, DualMapper)
    accepts_int(dual)
    accepts_text(dual)
    accepts_int(ref)
    assert_type(await use_int_mapper_async(dual), int)
    assert_type(await use_text_mapper_async(dual), str)
    assert_type(await optional_mapper_async(dual), str)
    assert_type(await optional_mapper_async(None), str)
    assert_type(await use_tagged_async(TaggedRecord(name="Ada")), str)
    client = await ResponsesClient.new_async(model="test", api_key="unused")
    assert_type(await Agent.new_async(client=client), Agent)
