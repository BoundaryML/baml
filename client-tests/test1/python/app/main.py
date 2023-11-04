from baml_core import baml_init
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
            {"role": "user", "content": "I'm having trouble with my computer."},
        ]
    )
    return response


@trace
async def call_topic_router():
    response = await baml.MaybePolishText.get_impl("v1").run(
        ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    )
    return response


@trace
async def main():
    results = await asyncio.gather(test_azure_default())
    for r in results:
        print(r)


if __name__ == "__main__":
    baml_init()
    asyncio.run(main())
