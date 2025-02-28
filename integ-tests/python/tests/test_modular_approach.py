import pytest

from openai import AsyncOpenAI
from dotenv import load_dotenv

load_dotenv()
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client import types


@pytest.mark.asyncio
async def test_modular_openai_gpt4():
    client = AsyncOpenAI()

    prompt = await b.prompt.ExtractResume2("""
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

    response = await client.chat.completions.create(model="gpt-4o", messages=prompt["messages"])

    parsed = b.parse.ExtractResume(response.choices[0].message.content)

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
