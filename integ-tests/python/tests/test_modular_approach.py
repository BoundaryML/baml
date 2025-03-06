import pytest
import typing
import anthropic
from openai import AsyncOpenAI
from openai.types.chat import ChatCompletion
from dotenv import load_dotenv

load_dotenv()
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client import types


@pytest.mark.asyncio
async def test_modular_openai_gpt4():
    client = AsyncOpenAI()

    req = await b.request.ExtractResume2("""
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

    """)

    # Needs cast because of **req.body
    response = typing.cast(ChatCompletion, await client.chat.completions.create(**req.body))

    parsed = b.parse.ExtractResume2(response.choices[0].message.content)

    assert parsed == types.Resume(
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

def test_modular_anthropic_claude_3_haiku():
    client = anthropic.Anthropic()

    req = sync_b.request.ExtractResumeClaude("""
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

    """)

    response = client.messages.create(**req.body)

    parsed = sync_b.parse.ExtractResumeClaude(response.content[0].text)

    assert parsed == types.Resume(
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
