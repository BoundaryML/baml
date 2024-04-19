"""
Run this script to see how the BAML client can be used in Python.

python -m example_baml_app
"""

import asyncio
from baml_client import baml as b
from baml_client.baml_types import Message
from datetime import datetime
from typing import List

async def extract_resume(resume: str) -> None:
    """
    Extracts the resume and prints the extracted data.
    """
    print("\n\nExtracting a resume while streaming...")
    async with b.ExtractResume.stream(text=resume) as stream:
        async for x in stream.parsed_stream:
            if x.is_parseable:
                print(f"streaming: {x.parsed.model_dump_json()}")
        response = await stream.get_final_response()
        if response.has_value:
            print(f"\n final: {response.value.model_dump_json(indent=2)}")
        else:
            print("No final response")


async def classify_conversation(messages: List[Message]) -> None:
    """
    Classifies the chat and prints the classification.
    """
    classification = await b.ClassifyConversation(messages=messages)
    print("Got categories: ", classification)


async def main():
    resume = """
    John Doe
    1234 Elm Street
    Springfield, IL 62701
    (123) 456-7890

    Objective: To obtain a position as a software engineer.

    Education:
    Bachelor of Science in Computer Science
    University of Illinois at Urbana-Champaign
    May 2020 - May 2024

    Experience:
    Software Engineer Intern
    Google
    May 2022 - August 2022
    - Worked on the Google Search team
    - Developed new features for the search engine
    - Wrote code in Python and C++

    Software Engineer Intern
    Facebook
    May 2021 - August 2021
    - Worked on the Facebook Messenger team
    - Developed new features for the messenger app
    - Wrote code in Python and Java
    """
    await extract_resume(resume)

    messages = [
        Message(role="user", message="I'm having issues with my computer."),
        Message(role="assistant",
            message="I'm sorry to hear that. What seems to be the problem?",
        ),
        Message(role="user",
            message="It's running really slow. I need to return it. Can I get a refund?",
        ),
    ]
    await classify_conversation(messages)


if __name__ == "__main__":
    asyncio.run(main())
