import pytest
import typing
import anthropic
import requests
from google import genai
from openai import AsyncOpenAI
from openai.types.chat import ChatCompletion
from dotenv import load_dotenv
from baml_py import ClientRegistry
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client import types

load_dotenv()


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
            graduation_date=None
        )
    ],
    skills=["Python", "JavaScript", "SQL"]
)


@pytest.mark.asyncio
async def test_modular_openai_gpt4():
    client = AsyncOpenAI()

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # Needs cast because of **req.body
    response = typing.cast(ChatCompletion, await client.chat.completions.create(**req.body.json()))

    parsed = b.parse.ExtractResume2(response.choices[0].message.content)

    assert parsed == JOHN_DOE_PARSED_RESUME

@pytest.mark.asyncio
async def test_modular_anthropic_claude_3_haiku():
    client = anthropic.AsyncAnthropic()

    cr = ClientRegistry()
    cr.set_primary("Claude")

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {"client_registry": cr})

    response = typing.cast(anthropic.types.Message, await client.messages.create(**req.body.json()))

    parsed = b.parse.ExtractResume2(response.content[0].text)

    assert parsed == JOHN_DOE_PARSED_RESUME

@pytest.mark.asyncio
async def test_modular_google_gemini():
    client = genai.Client()

    cr = ClientRegistry()
    cr.set_primary("Gemini")

    req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {"client_registry": cr})

    body = req.body.json()
    response = await client.aio.models.generate_content(model="gemini-1.5-pro-001", contents=body["contents"], config={
        "safety_settings": [body["safetySettings"]]
    })

    parsed = b.parse.ExtractResume2(response.text)

    assert parsed == JOHN_DOE_PARSED_RESUME


def test_modular_openai_gpt4_manual_http_request():
    req = sync_b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    # We can also use data=req.body.raw() or data=req.body.text()
    response = requests.post(url=req.url, headers=req.headers, json=req.body.json())

    parsed = sync_b.parse.ExtractResume2(response.json()["choices"][0]["message"]["content"])

    assert parsed == JOHN_DOE_PARSED_RESUME
