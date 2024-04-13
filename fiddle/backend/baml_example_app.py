# """
# Run this script to see how the BAML client can be used in Python.

# python -m example_baml_app
# """

# import asyncio
# from baml_client import baml as b
# from datetime import datetime
# from typing import List
# from typing_extensions import TypedDict

# async def extract_resume(resume: str) -> None:
#     """
#     Extracts the resume and prints the extracted data.
#     """
#     print("Parsing resume...")
#     print(resume[:100] + "..." if len(resume) > 100 else resume)
#     parsed_resume = await b.ExtractResume(resume)
#     print(parsed_resume.model_dump_json(indent=2))

#     await asyncio.sleep(1)
#     print("\n\nNow extracting using streaming")
#     async with b.ExtractResume.stream(resume) as stream:
#         async for x in stream.parsed_stream:
#             if x.is_parseable:
#                 print(f"streaming: {x.parsed.model_dump_json()}")
#         response = await stream.get_final_response()
#         if response.has_value:
#             print(f"\n final: {response.value.model_dump_json(indent=2)}")
#         else:
#             print("No final response")


# class ChatMessage(TypedDict):
#     sender: str
#     message: str


# async def classify_chat(messages: List[ChatMessage]) -> None:
#     """
#     Classifies the chat and prints the classification.
#     """
#     print("Classifying chat...")
#     chat = "\n".join(map(lambda m: f'{m["sender"]}: {m["message"]}', messages))
#     print(chat[:100] + "..." if len(chat) > 100 else chat)

#     classification = await b.ClassifyMessage(
#         message=chat, message_date=datetime.now().strftime("%Y-%m-%d")
#     )
#     print("Got categories: ", classification)


# async def main():
#     resume = """
#     John Doe
#     1234 Elm Street
#     Springfield, IL 62701
#     (123) 456-7890

#     Objective: To obtain a position as a software engineer.

#     Education:
#     Bachelor of Science in Computer Science
#     University of Illinois at Urbana-Champaign
#     May 2020 - May 2024

#     Experience:
#     Software Engineer Intern
#     Google
#     May 2022 - August 2022
#     - Worked on the Google Search team
#     - Developed new features for the search engine
#     - Wrote code in Python and C++

#     Software Engineer Intern
#     Facebook
#     May 2021 - August 2021
#     - Worked on the Facebook Messenger team
#     - Developed new features for the messenger app
#     - Wrote code in Python and Java
#     """
#     await extract_resume(resume)

#     messages = [
#         {"sender": "Alice", "message": "I'm having issues with my computer."},
#         {
#             "sender": "Assistant",
#             "message": "I'm sorry to hear that. What seems to be the problem?",
#         },
#         {
#             "sender": "Alice",
#             "message": "It's running really slow. I need to return it. Can I get a refund?",
#         },
#     ]
#     await classify_chat(messages)


# if __name__ == "__main__":
#     asyncio.run(main())
