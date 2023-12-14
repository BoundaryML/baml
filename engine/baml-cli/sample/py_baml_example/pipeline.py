
from termcolor import colored
from datetime import datetime
from baml_client import baml as b
from baml_client.tracing import trace
from baml_client.baml_types import Intent, Conversation, Message, UserType
from .utils import loading_animation


class End:
    def __init__(self, msg: str):
        self.msg = msg

    def __str__(self):
        return self.msg

    def __repr__(self):
        return self.msg




@trace
async def pipeline(convo: Conversation) -> str | End:
    """
    This function returns the message that the AI should send to the user.
    """

    # First we call an intent classifier. 
    # This is strongly typed! Note that intents is a List[Intent] type.
    with loading_animation('> Classifying'):
        intents = await b.ClassifyIntent(query=convo.display)

    if Intent.BookMeeting in intents:
        # If the user wants to book a meeting, we need to extract the meeting details.
        with loading_animation('> Extracting meeting details'):
            partial_info = await b.ExtractMeetingRequestInfoPartial(convo=convo, now=datetime.now().isoformat())

        with loading_animation('> Checking if we have all the details'):
            check = await b.GetNextQuestion(partial_info)

        if check.requirements_complete:
            # TODO: actually book the meeting by calling a real API
            return End(f"""\
I have scheduled a meeting for you:
{partial_info.model_dump_json()}\
            """)
        elif check.follow_up_question:
            return check.follow_up_question
        else:
            return 'Sorry, I did not understand your request. What do you mean?'
    if len(intents) == 0:
        return "I'm a bot that can book meetings. How can I help you?"
    else:
        return End(f'Sorry! I only know how to book meetings. You tried to: {intents}')

@trace
async def convo_demo():

    convo = Conversation(messages=[])
    while True:
        user_query = input(f'{colored("User", "yellow")}: ')
        try:
            convo.messages.append(Message(content=user_query, user=UserType.User))
            next_message = await pipeline(convo)
            print(f'{colored("AI", "yellow")}: {next_message}')
            if isinstance(next_message, End):
                break
            else:
                convo.messages.append(Message(content=next_message, user=UserType.AI))
        except Exception as e:
            print(f'{colored("Error", "red")}: {e}')
            break
            
