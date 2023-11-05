from baml_lib import baml_init
import asyncio

from baml_core.otel import trace, set_tags
from baml_client import baml
from baml_client.baml_types import ProposedMessage, Conversation


@trace
async def test_azure_default():
    set_tags(a="bar", b="car")
    response = await baml.AZURE_DEFAULT.run_chat(
        [
            {
                "role": "system",
                "content": "Address the users questions to the bset of your abilities.",
            },
            {"role": "user", "content": "I need a lawnmower"},
        ]
    )
    return response


@trace
async def call_topic_router():
    response = await baml.MaybePolishText.get_impl("v1").run(
        ProposedMessage(thread=Conversation(thread=[]), generated_response="boooooo")
    )
    return response


@trace
async def main():
    await asyncio.gather(test_azure_default(), call_topic_router())


if __name__ == "__main__":
    baml_init()
    asyncio.run(main())
