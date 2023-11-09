import asyncio
from baml_client.tracing import trace, set_tags

# from baml_core.otel import trace, set_tags
from baml_lib import baml_init
from baml_client import baml
from baml_client.baml_types import ProposedMessage, Conversation
from baml_client.baml_types import Message, MessageSender, ClassifyRequest


@trace
async def test_azure_default():
    set_tags(a="bar", b="car")
    response = await baml.ResilientGPT4.run_chat(
        [
            {
                "role": "system",
                "content": "Address the users questions to the bset of your abilities.",
            },
            {"role": "user", "content": "I need a lawnmower"},
        ]
    )
    # await baml.Blah.get_impl("v1").run("blah")
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
        ClassifyRequest(
            context="The user is a software engineer", query="Can you explain TDD?"
        )
    )
    return response


@trace
async def main():
    await call_topic_router()


if __name__ == "__main__":
    baml_init()
    asyncio.run(test_azure_default())
