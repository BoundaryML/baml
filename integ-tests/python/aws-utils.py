import asyncio

import boto3
from baml_client import b

client = boto3.client("bedrock")


def create_inference_profile():
    response = client.create_inference_profile(
        inferenceProfileName="Claude37",
        description="Application profile for Claude 3.7 Sonnet",
        modelSource={
            "copyFrom": "arn:aws:bedrock:us-east-1::foundation-model/anthropic.claude-3-5-sonnet-20241022-v2:0"
        },
    )

    return response


async def test_bedrock_inference_profile():
    res = await b.TestAwsInferenceProfile("Hello, world!")
    print(res)


async def test_bedrock_inference_profile_streaming():
    res = b.stream.TestAwsInferenceProfile("Hello, world!")
    async for chunk in res:
        print(chunk)


if __name__ == "__main__":
    # profile = create_inference_profile()
    # print(f"Created inference profile: {profile}")
    # asyncio.run(test_bedrock_inference_profile())
    asyncio.run(test_bedrock_inference_profile_streaming())
