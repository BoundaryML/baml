# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_gpt35 import GPT35
from ..functions.fx_fntestoutputadapter import BAMLFnTestOutputAdapter
from ..types.classes.cls_modifiedoutput import ModifiedOutput
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer


import typing
# Impl: v1
# Client: GPT35
# An implementation of FnTestOutputAdapter.

__prompt_template = """\
Question: What is the capital of France?

Return in this format:
{
  "REASONING": string,
  "ANSWER": string
}

JSON:\
"""

__input_replacers = {
}


# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[ModifiedOutput](ModifiedOutput)  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[ModifiedOutput](ModifiedOutput)  # type: ignore


def output_adapter(arg: ModifiedOutput) -> str:
    return arg.answer




async def v1(arg: str, /) -> str:
    response = await GPT35.run_prompt_template(template=__prompt_template, replacers=__input_replacers, params=dict(arg=arg))
    deserialized = __deserializer.from_string(response.generated)
    return output_adapter(deserialized)


def v1_stream(arg: str, /) -> AsyncStream[str, str]:
    raise NotImplementedError("Stream functions do not support output adapters")

BAMLFnTestOutputAdapter.register_impl("v1")(v1, v1_stream)