import pytest
from baml_client import baml

from baml_client.baml_types import (
    Conversation,
    ProposedMessage,
    IMaybePolishText,
)
from baml_client.testing import baml_test
import json

# async def some_traced_fn():
#     pass


# from baml_client import baml
# from baml_client.baml_types import IBlah
# from baml_lib._impl.deserializer import Deserializer


# @baml.Blah.test
# async def test_mytest(BlahImpl: IBlah):
#     deserializer = Deserializer[str](str)
#     param = deserializer.from_string("""asdfasdf""")
#     await BlahImpl(param)

import typing
# Async stream
class AsyncStream:
    stream: typing.Optional[typing.AsyncIterator[str]]
    # def __init__(self, param: typing.AsyncIterator[str]):
    #     self.param = param
    def __init__(self, param: typing.AsyncIterator[str]):
        self.stream = None

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        # Emit a final otel event
        # Look at what args are, as you can get cont
        pass

    async def text_stream(self):
        async for x in self.param:
            yield x



import typing

import contextlib


@contextlib.asynccontextmanager
async def my_custom_stream():
    for i in range(3):
        yield f"hi {i}"
    


@contextlib.asynccontextmanager
async def v1_stream() -> AsyncStream:  # span1
    # llm generator starts, sends events in span1
    


@baml_test
@pytest.mark.asyncio
async def test_logic() -> None:

    s = v1_stream()
    async with s as stream:
        async for x in stream.text_stream():
            print(f"streaming: {x}")

    async with v1_stream() as stream:  # different context
        async for x in stream.text_stream():
            print(f"streaming: {x}")

    count = 0
    async with baml.MaybePolishText.get_impl("v1").stream(
        ProposedMessage(thread=Conversation(thread=[]), generated_response="test"),
    ) as stream:
        async for x in stream.parsed_stream:
            print(f"streaming: {x.json()}")
            count += 1
            # print(f"streaming: {x.dump_json()}")
    print(f"chunks: {count}")
    assert count > 0
    print(f"streaming done")
    result = await stream.get_final_response()

    # print(f"final: {result.value.model_dump_json()}")

    # res = await baml.MaybePolishText(
    #     ProposedMessage(
    #         thread=Conversation(thread=[]),
    #         generated_response="i dont have that account ready",
    #     )
    # )

    # res = baml.MaybePolishText.stream(
    #     ProposedMessage(
    #         thread=Conversation(thread=[]),
    #         generated_response="i dont have that account ready",
    #     )
    # )

    # count = 0
    # async with res as stream:
    #     async for x in stream.parsed_stream:
    #         # print(f"streaming: {x.parsed}")
    #         count += 1
    #         # print(f"streaming: {x.dump_json()}")
    # print(f"chunks: {count}")
    # assert count > 0
    # print(f"streaming done")
    # result = await stream.get_final_response()
    # print(f"final: {result.value}\n\n")

    # Delta stream
    # async with res as stream:
    #     async for x in stream.text_stream:
    #         print(f"delta streaming: {x.delta}")
    #         count += 1
    #         # print(f"streaming: {x.dump_json()}")
    # print(f"delta chunks: {count}")
    # assert count > 0
    # print(f"delta streaming done")
    # result = await stream.get_final_response()
    # print(f"final: {result.has_value} {result.value}")

    # stream = baml.MaybePolishText.stream(
    #     ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    # )
    # print(f"streaming2")
    # async for x in stream:
    #     print(f"streaming: {x}")

    # await baml.MaybePolishText(
    #     ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    # )

    # result = await baml.MaybePolishText.get_impl("v1").run(
    #     ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    # )
    # print(result)


# @baml.MaybePolishText.test
# class TestThingies:
#     def test_with_a_class(self, MaybePolishTextImpl: IMaybePolishText):
#         pass


# class TestMultiple:
#     @baml.MaybePolishText.test
#     def test_with_class2(self, MaybePolishTextImpl: IMaybePolishText):
#         pass

#     @baml.TextPolisher.test
#     def test_with_class3(self, TextPolisherImpl: ITextPolisher):
#         pass


# def test_without_fixture3():
#     pass
