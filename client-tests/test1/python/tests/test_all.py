import pytest
from baml_client import baml

from baml_client.baml_types import (
    Conversation,
    ProposedMessage,
    IMaybePolishText,
)
from baml_client.testing import baml_test
import json
import typing


@baml_test
@pytest.mark.asyncio
async def test_logic() -> typing.Any:
    count = 0
    try:
        async with baml.MaybePolishText.stream(
            ProposedMessage(thread=Conversation(thread=[]), generated_response="test"),
        ) as stream:
            async for x in stream.text_stream:
                print(f"streaming: {x.delta}")

                count += 1
        print(f"chunks: {count}")
        assert count > 0
        print(f"streaming done")

        result = await stream.get_final_response()
    except Exception as e:
        print(f"error: {e}")

    res = await baml.MaybePolishText(
        ProposedMessage(
            thread=Conversation(thread=[]),
            generated_response="i dont have that account ready",
        )
    )
    return res


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
