import asyncio
from generated.clients import AZURE_DEFAULT
from generated.functions import TopicRouter
from gloo_py import trace


@trace()
async def test_azure_default():
    response = await AZURE_DEFAULT.run(
        "customer-service",
        prompt=[
            {
                "role": "system",
                "content": "Address the users questions to the bset of your abilities.",
            },
            {"role": "user", "content": "I'm having trouble with my computer."},
        ],
    )
    return response


@trace()
async def call_topic_router():
    response = await TopicRouter("v1", "I'm having trouble with my computer.")
    return response


@trace()
async def main():
    await asyncio.gather(test_azure_default(), call_topic_router())


if __name__ == "__main__":
    asyncio.run(main())
