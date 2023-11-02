import asyncio
from baml_core.otel import trace
from baml_client import baml
from baml_client.baml_types import (
    ProposedMessage,
)


@trace
async def test_azure_default():
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
        ProposedMessage(
            thread_id=[],
            generated_response="I'm having trouble with my computer.",
        )
    )
    return response


@trace
async def main():
    await asyncio.gather(test_azure_default(), call_topic_router())


if __name__ == "__main__":
    asyncio.run(main())
