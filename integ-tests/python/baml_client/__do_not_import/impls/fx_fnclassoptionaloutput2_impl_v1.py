# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_gpt35 import GPT35
from ..functions.fx_fnclassoptionaloutput2 import BAMLFnClassOptionalOutput2
from ..types.classes.cls_blah import Blah
from ..types.classes.cls_classoptionaloutput2 import ClassOptionalOutput2
from ..types.partial.classes.cls_blah import PartialBlah
from ..types.partial.classes.cls_classoptionaloutput2 import PartialClassOptionalOutput2
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer
from typing import Optional


import typing
# Impl: v1
# Client: GPT35
# An implementation of FnClassOptionalOutput2.

__prompt_template = """\
Return a json blob for the following input:
{arg}

Answer in JSON using this schema:
{
  "prop1": string | null,
  "prop2": string | null,
  "prop3": {
    "prop4": string | null
  }
}

JSON:\
"""

__input_replacers = {
    "{arg}"
}


# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[Optional[ClassOptionalOutput2]](Optional[ClassOptionalOutput2])  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[PartialClassOptionalOutput2](PartialClassOptionalOutput2)  # type: ignore







async def v1(arg: str, /) -> Optional[ClassOptionalOutput2]:
    response = await GPT35.run_prompt_template(template=__prompt_template, replacers=__input_replacers, params=dict(arg=arg))
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def v1_stream(arg: str, /) -> AsyncStream[Optional[ClassOptionalOutput2], PartialClassOptionalOutput2]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = GPT35.run_prompt_template_stream(template=__prompt_template, replacers=__input_replacers, params=dict(arg=arg))
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAMLFnClassOptionalOutput2.register_impl("v1")(v1, v1_stream)