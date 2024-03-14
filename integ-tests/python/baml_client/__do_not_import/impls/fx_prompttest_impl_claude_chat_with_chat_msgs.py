# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_claude import Claude
from ..functions.fx_prompttest import BAMLPromptTest
from baml_core.provider_manager.llm_provider_base import LLMChatMessage
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer
from typing import List


import typing
# Impl: claude_chat_with_chat_msgs
# Client: Claude
# An implementation of PromptTest.

__prompt_template: List[LLMChatMessage] = [
{
    "role": "system",
    "content": """\
You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {arg}\
"""
}
,
{
    "role": "user",
    "content": """\
Tell me a haiku about {arg}\
"""
}

]

__input_replacers = {
    "{arg}",
    "{arg}"
}


# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[str](str)  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[str](str)  # type: ignore







async def claude_chat_with_chat_msgs(arg: str, /) -> str:
    response = await Claude.run_chat_template(__prompt_template, replacers=__input_replacers, params=dict(arg=arg))
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def claude_chat_with_chat_msgs_stream(arg: str, /) -> AsyncStream[str, str]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = Claude.run_chat_template_stream(__prompt_template, replacers=__input_replacers, params=dict(arg=arg))
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAMLPromptTest.register_impl("claude_chat_with_chat_msgs")(claude_chat_with_chat_msgs, claude_chat_with_chat_msgs_stream)