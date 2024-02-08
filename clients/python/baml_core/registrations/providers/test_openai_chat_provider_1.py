import os
from dotenv import load_dotenv

load_dotenv()

OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY", None)


# @pytest.mark.asyncio
# async def test_openai_provider():
#     client = OpenAIChatProvider(
#         provider="baml-openai-chat",
#         retry_policy=None,
#         options={
#             "api_key": OPENAI_API_KEY,
#             "model": "gpt-3.5-turbo",
#             "request_timeout": 45,
#             "max_tokens": 400,
#         },
#     )

#     response = client._stream_chat(
#         [{"role": "user", "content": "Write 2 haikus about a frog."}]
#     )

#     async for message in response:
#         print(message.generated)
