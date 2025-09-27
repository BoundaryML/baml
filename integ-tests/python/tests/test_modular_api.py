import asyncio
import math
import time
import typing
import json
import os
import pytest
import anthropic
import requests
import boto3
from google import genai
from openai import AsyncOpenAI, OpenAI, AsyncStream, Stream
from openai.types.chat import ChatCompletion, ChatCompletionChunk
from openai.types.responses import Response
from botocore.auth import SigV4Auth
from botocore.awsrequest import AWSRequest
from botocore.exceptions import ProfileNotFound
from urllib.parse import urlsplit
from baml_py import ClientRegistry, HTTPRequest as BamlHttpRequest
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client import types, partial_types


# Some reusable data across tests.

JOHN_DOE_TEXT_RESUME = """
    John Doe
    johndoe@example.com
    (123) 456-7890
    Software Engineer
    Python, JavaScript, SQL

    Education
    University of California, Berkeley (Berkeley, CA)
    Master's in Computer Science

    Experience
    Software Engineer at Google (2020 - Present)
"""

LONG_CACHEABLE_CONTEXT = " ".join(["Reusable cacheable context paragraph." for _ in range(600)])

JOHN_DOE_PARSED_RESUME = types.Resume(
    name="John Doe",
    email="johndoe@example.com",
    phone="(123) 456-7890",
    experience=["Software Engineer at Google (2020 - Present)"],
    education=[
        types.Education(
            institution="University of California, Berkeley",
            location="Berkeley, CA",
            degree="Master's",
            major=["Computer Science"],
            graduation_date=None,
        )
    ],
    skills=["Python", "JavaScript", "SQL"],
)

JOHN_DOE_PARSED_RESUME_PARTIAL = partial_types.Resume(
    name="John Doe",
    email="johndoe@example.com",
    phone="(123) 456-7890",
    experience=["Software Engineer at Google (2020 - Present)"],
    education=[
        partial_types.Education(
            institution="University of California, Berkeley",
            location="Berkeley, CA",
            degree="Master's",
            major=["Computer Science"],
            graduation_date=None,
        )
    ],
    skills=["Python", "JavaScript", "SQL"],
)

JANE_SMITH_TEXT_RESUME = """
    Jane Smith
    janesmith@example.com
    (555) 123-4567
    Data Scientist
    Python, R, TensorFlow, PyTorch, SQL

    Education
    Stanford University (Stanford, CA)
    Ph.D. in Statistics

    Experience
    Senior Data Scientist at Netflix (2019 - Present)
    Machine Learning Engineer at Amazon (2016 - 2019)
"""

JANE_SMITH_PARSED_RESUME = types.Resume(
    name="Jane Smith",
    email="janesmith@example.com",
    phone="(555) 123-4567",
    experience=[
        "Senior Data Scientist at Netflix (2019 - Present)",
        "Machine Learning Engineer at Amazon (2016 - 2019)",
    ],
    education=[
        types.Education(
            institution="Stanford University",
            location="Stanford, CA",
            degree="Ph.D.",
            major=["Statistics"],
            graduation_date=None,
        )
    ],
    skills=["Python", "R", "TensorFlow", "PyTorch", "SQL"],
)


@pytest.mark.asyncio
async def test_modular_openai_gpt4():
    client = AsyncOpenAI()

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # Needs cast because of **req.body
    response = typing.cast(
        ChatCompletion, await client.chat.completions.create(**req.body.json())
    )

    parsed = b.parse.ExtractResume2(response.choices[0].message.content)

    assert parsed == JOHN_DOE_PARSED_RESUME


@pytest.mark.asyncio
async def test_modular_bedrock_manual_cache_point():
    req = await b.request.TestAws("Dr. Pepper")

    body = req.body.json()
    assert isinstance(body["messages"], list)
    assert len(body["messages"]) > 0

    content = body["messages"][0]["content"]
    assert isinstance(content, list)
    original_length = len(content)

    content.insert(1, {"text": LONG_CACHEABLE_CONTEXT})
    content.insert(2, {"cachePoint": {"type": "default"}})

    assert content[1] == {"text": LONG_CACHEABLE_CONTEXT}
    assert content[2] == {"cachePoint": {"type": "default"}}
    assert len(content) == original_length + 2

    # additional.setdefault("cacheConfig", {"type": "default"})

    body_string = json.dumps(body)
    body_bytes = body_string.encode("utf-8")

    url = urlsplit(req.url)

    base_headers = {
        key: value
        for key, value in dict(req.headers).items()
        if value is not None
    }

    headers = {
        **base_headers,
        "content-type": "application/json",
        "accept": "application/json",
        "host": url.netloc,
    }

    try:
        session = boto3.Session()
    except ProfileNotFound:
        session = boto3.Session(profile_name=None)
    credentials = session.get_credentials()
    if credentials is None:
        pytest.skip("AWS credentials not configured")
    frozen = credentials.get_frozen_credentials()

    region = os.environ.get("AWS_REGION") or os.environ.get("AWS_DEFAULT_REGION")
    if not region:
        region = session.region_name
    if not region:
        available = session.get_available_regions("bedrock-runtime")
        region = available[0] if available else None
    if not region:
        pytest.skip("AWS region not configured")

    aws_request = AWSRequest(
        method=req.method,
        url=req.url,
        data=body_bytes,
        headers=headers,
    )

    SigV4Auth(frozen, "bedrock", region).add_auth(aws_request)

    response = requests.post(
        req.url,
        headers={key: str(value) for key, value in aws_request.headers.items()},
        data=body_bytes,
    )
    if not response.ok:
        raise AssertionError(
            f"Bedrock returned {response.status_code}: {response.text}"
        )
    response.raise_for_status()

    payload = response.json()
    content_blocks = payload.get("output", {}).get("message", {}).get("content", [])
    assert isinstance(content_blocks, list)
    text_block = next((block.get("text") for block in content_blocks if "text" in block), "")
    assert text_block


@pytest.mark.asyncio
async def test_modular_anthropic_claude_3_haiku():
    client = anthropic.AsyncAnthropic()

    cr = ClientRegistry()
    cr.set_primary("Claude")

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {"client_registry": cr})

    response = typing.cast(
        anthropic.types.Message, await client.messages.create(**req.body.json())
    )

    parsed = b.parse.ExtractResume2(response.content[0].text)

    assert parsed == JOHN_DOE_PARSED_RESUME


@pytest.mark.asyncio
async def test_modular_google_gemini():
    client = genai.Client()

    cr = ClientRegistry()
    cr.set_primary("Gemini")

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {"client_registry": cr})

    body = req.body.json()
    response = await client.aio.models.generate_content(
        model="gemini-1.5-flash",
        contents=body["contents"],
        config={"safety_settings": [body["safetySettings"]]},
    )

    parsed = b.parse.ExtractResume2(response.text)

    assert parsed == JOHN_DOE_PARSED_RESUME


def test_modular_openai_gpt4_sync():
    client = OpenAI()

    req = sync_b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # Needs cast because of **req.body
    response = typing.cast(
        ChatCompletion, client.chat.completions.create(**req.body.json())
    )

    parsed = sync_b.parse.ExtractResume2(response.choices[0].message.content)

    assert parsed == JOHN_DOE_PARSED_RESUME


@pytest.mark.asyncio
async def test_modular_openai_gpt4_streaming():
    client = AsyncOpenAI()

    req = await b.stream_request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # Needs cast because of **req.body
    response = typing.cast(
        AsyncStream[ChatCompletionChunk],
        await client.chat.completions.create(**req.body.json()),
    )

    llm_response: list[str] = []

    async for chunk in response:
        if len(chunk.choices) > 0 and chunk.choices[0].delta.content is not None:
            llm_response.append(chunk.choices[0].delta.content)

    parsed = b.parse_stream.ExtractResume2("".join(llm_response))

    assert parsed == JOHN_DOE_PARSED_RESUME_PARTIAL


def test_modular_openai_gpt4_streaming_sync():
    client = OpenAI()

    req = sync_b.stream_request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # Needs cast because of **req.body
    response = typing.cast(
        Stream[ChatCompletionChunk], client.chat.completions.create(**req.body.json())
    )

    llm_response: list[str] = []

    for chunk in response:
        if len(chunk.choices) > 0 and chunk.choices[0].delta.content is not None:
            llm_response.append(chunk.choices[0].delta.content)

    parsed = b.parse_stream.ExtractResume2("".join(llm_response))

    assert parsed == JOHN_DOE_PARSED_RESUME_PARTIAL


def test_modular_openai_gpt4_manual_http_request():
    req = sync_b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # We can also use data=req.body.raw() or data=req.body.text()
    response = requests.post(url=req.url, headers=req.headers, json=req.body.json())

    parsed = sync_b.parse.ExtractResume2(
        response.json()["choices"][0]["message"]["content"]
    )

    assert parsed == JOHN_DOE_PARSED_RESUME


def to_openai_jsonl(req: BamlHttpRequest) -> str:
    line = json.dumps(
        {
            "custom_id": req.id,
            "method": "POST",
            "url": "/v1/chat/completions",
            "body": req.body.json(),
        }
    )

    return f"{line}\n"


@pytest.mark.asyncio
async def test_openai_batch_api():
    client = AsyncOpenAI()

    john_req, jane_req = await asyncio.gather(
        b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME),
        b.request.ExtractResume2(JANE_SMITH_TEXT_RESUME),
    )

    jsonl = to_openai_jsonl(john_req) + to_openai_jsonl(jane_req)

    batch_input_file = await client.files.create(
        file=jsonl.encode("utf-8"),
        purpose="batch",
    )

    batch = await client.batches.create(
        input_file_id=batch_input_file.id,
        endpoint="/v1/chat/completions",
        completion_window="24h",
        metadata={"description": "BAML Modular API Python Batch Integ Test"},
    )

    backoff = 1
    attempts = 0
    max_attempts = 30

    # Constant backoff, we'll wait approximately 30 seconds before we give up.
    # Usually the batch completes in 8 to 15 seconds but sometimes it takes
    # longer. Note that if this fails it doesn't necessarily mean that there's
    # a bug in the test or that assertions are wrong, it just means that OpenAI
    # takes too long to process the batch.
    while True:
        batch = await client.batches.retrieve(batch.id)
        attempts += 1

        if batch.status == "completed":
            break

        if attempts >= max_attempts:
            try:
                await client.batches.cancel(batch.id)
            finally:
                pytest.fail("Batch failed to complete in time")

        await asyncio.sleep(backoff)
        # back_off *= 2 # Exponential backoff.

    # If status == "completed" then output_file_id is not None
    assert batch.output_file_id is not None

    output = await client.files.content(batch.output_file_id)

    expected = {
        john_req.id: JOHN_DOE_PARSED_RESUME,
        jane_req.id: JANE_SMITH_PARSED_RESUME,
    }

    received: dict[str, types.Resume] = {}

    for line in output.text.splitlines():
        result = json.loads(line)
        llm_response = result["response"]["body"]["choices"][0]["message"]["content"]

        parsed = b.parse.ExtractResume2(llm_response)
        received[result["custom_id"]] = parsed

    assert received == expected


@pytest.mark.asyncio
async def test_modular_openai_responses():
    """Test openai-responses provider using the modular API"""
    from openai import AsyncOpenAI

    client = AsyncOpenAI()

    # Use TestOpenAIResponses from the providers directory
    req = await b.request.TestOpenAIResponses("mountains")

    # The openai-responses provider should use the /v1/responses endpoint.

    response = typing.cast(
        Response, await client.responses.create(**req.body.json())
    )

    parsed = b.parse.TestOpenAIResponses(response.output_text)

    assert isinstance(parsed, str)
    assert len(parsed) > 0


@pytest.mark.asyncio
async def test_modular_vertex():
    start = time.time()
    # warm up with #1
    _ = await b.request.TestVertex("Hello, world!")
    end = time.time()
    warmup_duration = (end - start) * 1000

    # now load 10, they should fast
    durations = []
    for _ in range(10):
        start = time.time()
        _ = await b.request.TestVertex("Hello, world!")
        end = time.time()
        durations.append((end - start) * 1000)

    mean_duration = sum(durations) / len(durations)
    std_dev = 0
    for duration in durations:
        std_dev += (duration - mean_duration) ** 2
    std_dev = math.sqrt(std_dev / len(durations))

    print(f"Warmup: {warmup_duration:.1f}ms")
    print(f"Mean: {mean_duration:.1f}ms ± {std_dev:.1f}ms")
    # we should be well under 50ms cause there's no network latency
    assert mean_duration < 50, "Vertex should be cached"
    # this should be pretty low
    assert std_dev < 5, "Vertex should be cached"
