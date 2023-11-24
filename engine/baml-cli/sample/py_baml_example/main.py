# Bug: deserialize for parsing an object with just a single field (should work)
"""
Run this script to see how the BAML client can be used in Python.

python -m py_baml_example.main
"""

import asyncio
from datetime import datetime
from baml_client import baml
from baml_client.baml_types import Intent, Conversation, Message, UserType


class End:
    def __init__(self, msg: str):
        self.msg = msg

    def __str__(self):
        return self.msg

    def __repr__(self):
        return self.msg


async def pipeline(convo: Conversation) -> str | End:
    """
    This function returns the message that the AI should send to the user.
    """

    # First we call an intent classifier. 
    # This is strongly typed! Note that intents is a List[Intent] type.
    # intents = await baml.IntentClassifier.get_impl('v1').run(convo.messages[-1].content)
    intents = await baml.ClassifyIntent(query=convo.messages[-1].content)

    if Intent.BookMeeting in intents:

        # If the user wants to book a meeting, we need to extract the meeting details.
        partial_info = await baml.ExtractMeetingRequestInfoPartial(convo=convo, now=datetime.now().isoformat())
        check = await baml.GetNextQuestion(partial_info)

        if check.complete:
            # TODO: actually book the meeting by calling a real API
            return End(f"""\
I have scheduled a meeting for you:
{partial_info.model_dump_json()}\
            """)
        elif check.follow_up_question:
            return check.follow_up_question
        else:
            return 'Sorry, I did not understand your request. What do you mean?'
    else:
        return End(f'Sorry! I only know how to book meetings. {intents}')

async def main():

    convo = Conversation(messages=[])
    while True:
        user_query = input('User:')
        try:
            convo.messages.append(Message(content=user_query, user=UserType.User))
            next_message = await pipeline(convo)
            print(f'AI: {next_message}')
            if isinstance(next_message, End):
                break
            else:
                convo.messages.append(Message(content=next_message, user=UserType.AI))
        except Exception as e:
            print(f'Error: {e}')
            break
            


if __name__ == '__main__':
    asyncio.run(main())
