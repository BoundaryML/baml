"""
Run this script to see how the BAML client can be used in Python.

python -m py_baml_example.main
"""

import asyncio
from datetime import datetime
from baml_client import baml
from baml_client.baml_types import Intent

async def main():
    user_query = input('AI: How can i help you?')

    try:
        # First we call an intent classifier. 
        # This is strongly typed! Note that intents is a List[Intent] type.
        intents = await baml.ClassifyIntent.get_impl('v1').run(user_query)

        if Intent.BookMeeting in intents:

            # If the user wants to book a meeting, we need to extract the meeting details.
            meeting_request = await baml.ExtractMeetingRequestInfo.get_impl('v2').run(
                query=user_query, now=datetime.now().isoformat())

            # TODO: We may want a loop here to continue collecting information in
            # case the user did not provide all the information we need.

            print('AI: I have booked a meeting with the following information:')
            print(f'AI: Title: {meeting_request.topic}')
            # TODO: Parse into python datetime
            print(f'AI: Date: {meeting_request.when}')
            print(f'AI: Attendees: {meeting_request.attendees}')
            
        else:
            print('AI: Sorry, I did not understand your request')
    except Exception as e:
        print(f'AI: Sorry, I encountered an error: {e}')


if __name__ == '__main__':
    asyncio.run(main())
