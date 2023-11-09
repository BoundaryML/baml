import asyncio

from baml_core.otel import trace, set_tags
from baml_lib import baml_init
from baml_client import baml
from baml_client.baml_types import ProposedMessage, Conversation
from baml_client.baml_types import Message, MessageSender, UserInfo


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
    # response = await baml.MaybePolishText.get_impl("v1").run(
    #     ProposedMessage(
    #         thread=Conversation(
    #             thread=[Message(sender=MessageSender.RESIDENT, body="are you there")]
    #         ),
    #         generated_response="Hello there.",
    #     )
    # )
    response = await baml.ClassifyTool.get_impl("v1").run(
        UserInfo(
            context="The user is a software engineer", query="Can you explain TDD?"
        )
    )
    return response


@trace
async def main():
    await call_topic_router()


if __name__ == "__main__":
    baml_init()
    asyncio.run(main())
