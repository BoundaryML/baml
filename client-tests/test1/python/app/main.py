import asyncio
import logging.config
import logging


from baml_client.tracing import trace, set_tags

# from baml_core.otel import trace, set_tags
from baml_client import baml
from baml_client.baml_types import ProposedMessage, Conversation
from baml_client.baml_types import Message, MessageSender

# Load the logging configuration
logging.config.fileConfig("logging.conf")
# logging.basicConfig(level=logging.INFO)
# Create a logger
logger = logging.getLogger(__name__)

# Use the logger
logger.debug("This is a debug message")
logger.info("This is an info message")


@trace
async def test_azure_default():
    set_tags(a="bar", b="car")
    response = await baml.ResilientGPT4.run_chat(
        [
            {
                "role": "system",
                "content": "Address the users questions to the bset of your abilities.",
            },
            {"role": "user", "content": "I need a lawnmower"},
        ]
    )
    await baml.Blah.get_impl("v1").run("blah")
    return response


@trace
async def call_topic_router():
    # response = await baml.MaybePolishText.get_impl("v1").run(
    #     ProposedMessage(
    #         thread=Conversation(
    #             thread=[Message(sender=MessageSender.RESIDENT, body="are you there")]
    #         ),
    #         generated_response="Hello there.",
    #     )
    # )
    response = await baml.ClassifyTool.get_impl("v1").run(
        context="The user is a software engineer", query="Can you explain TDD?"
    )
    return response


@trace
async def main():
    res = await call_topic_router()
    print(res)


if __name__ == "__main__":
    # for logger_name in logging.Logger.manager.loggerDict:
    #     print(logger_name)
    # baml.configure(base_url="http://localhost:3000/api")
    logger.info("About to run things")
    asyncio.run(main())
    logger.info("Hello there!")
