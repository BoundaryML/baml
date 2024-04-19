# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_gpt35 import GPT35
from ..functions.fx_v2_testfnnamedargssingleenumlist import BAMLV2_TestFnNamedArgsSingleEnumList
from ..types.enums.enm_namedargssingleenumlist2 import NamedArgsSingleEnumList2
from baml_core.jinja.render_prompt import RenderData
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer
from typing import List


import typing
# Impl: default_config
# Client: GPT35
# An implementation of V2_TestFnNamedArgsSingleEnumList.

__prompt_template = """\
Print these values back to me:
{{myArg}}\
"""

# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[str](str)  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[str](str)  # type: ignore

__output_format = """
string
""".strip()

__template_macros = [
]


async def default_config(*, myArg: List[NamedArgsSingleEnumList2]) -> str:
    response = await GPT35.run_jinja_template(
        jinja_template=__prompt_template,
        output_format=__output_format, template_macros=__template_macros,
        args=dict(myArg=myArg)
    )
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def default_config_stream(*, myArg: List[NamedArgsSingleEnumList2]
) -> AsyncStream[str, str]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = GPT35.run_jinja_template_stream(
            jinja_template=__prompt_template,
            output_format=__output_format, template_macros=__template_macros,
            args=dict(myArg=myArg)
        )
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAMLV2_TestFnNamedArgsSingleEnumList.register_impl("default_config")(default_config, default_config_stream)