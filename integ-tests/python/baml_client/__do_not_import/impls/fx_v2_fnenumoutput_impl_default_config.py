# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_gpt35 import GPT35
from ..functions.fx_v2_fnenumoutput import BAMLV2_FnEnumOutput
from ..types.enums.enm_enumoutput2 import EnumOutput2
from baml_core.jinja.render_prompt import RenderData
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer


import typing
# Impl: default_config
# Client: GPT35
# An implementation of V2_FnEnumOutput.

__prompt_template = """\
Choose one of these values randomly. Before you give the answer, write out an unrelated haiku about the ocean.

{{ ctx.output_schema(prefix=null) }}\
"""

# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[EnumOutput2](EnumOutput2)  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[EnumOutput2](EnumOutput2)  # type: ignore

__output_format = """
"VALUE_ENUM as string"

VALUE_ENUM
---
ONE
TWO
THREE
""".strip()

__template_macros = [
]


async def default_config(*, input: str) -> EnumOutput2:
    response = await GPT35.run_jinja_template(
        jinja_template=__prompt_template,
        output_format=__output_format, template_macros=__template_macros,
        args=dict(input=input)
    )
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def default_config_stream(*, input: str
) -> AsyncStream[EnumOutput2, EnumOutput2]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = GPT35.run_jinja_template_stream(
            jinja_template=__prompt_template,
            output_format=__output_format, template_macros=__template_macros,
            args=dict(input=input)
        )
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAMLV2_FnEnumOutput.register_impl("default_config")(default_config, default_config_stream)