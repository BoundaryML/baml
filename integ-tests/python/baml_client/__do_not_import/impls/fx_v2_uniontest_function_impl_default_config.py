# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..clients.client_gpt35 import GPT35
from ..functions.fx_v2_uniontest_function import BAMLV2_UnionTest_Function
from ..types.classes.cls_uniontest_returntypev2 import UnionTest_ReturnTypev2
from ..types.partial.classes.cls_uniontest_returntypev2 import PartialUnionTest_ReturnTypev2
from baml_core.jinja.render_prompt import RenderData
from baml_core.provider_manager.llm_response import LLMResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.deserializer import Deserializer
from typing import Union


import typing
# Impl: default_config
# Client: GPT35
# An implementation of V2_UnionTest_Function.

__prompt_template = """\
Return a JSON blob with this schema: 
{{ ctx.output_format }}
JSON:\
"""

# We ignore the type here because baml does some type magic to make this work
# for inline SpecialForms like Optional, Union, List.
__deserializer = Deserializer[UnionTest_ReturnTypev2](UnionTest_ReturnTypev2)  # type: ignore

# Add a deserializer that handles stream responses, which are all Partial types
__partial_deserializer = Deserializer[PartialUnionTest_ReturnTypev2](PartialUnionTest_ReturnTypev2)  # type: ignore

__output_format = """
{
  "prop1": string | bool,
  "prop2": (float | bool)[],
  "prop3": float[] | bool[]
}
""".strip()

__template_macros = [
]


async def default_config(*, input: Union[str, bool]) -> UnionTest_ReturnTypev2:
    response = await GPT35.run_jinja_template(
        jinja_template=__prompt_template,
        output_format=__output_format, template_macros=__template_macros,
        args=dict(input=input)
    )
    deserialized = __deserializer.from_string(response.generated)
    return deserialized


def default_config_stream(*, input: Union[str, bool]
) -> AsyncStream[UnionTest_ReturnTypev2, PartialUnionTest_ReturnTypev2]:
    def run_prompt() -> typing.AsyncIterator[LLMResponse]:
        raw_stream = GPT35.run_jinja_template_stream(
            jinja_template=__prompt_template,
            output_format=__output_format, template_macros=__template_macros,
            args=dict(input=input)
        )
        return raw_stream
    stream = AsyncStream(stream_cb=run_prompt, partial_deserializer=__partial_deserializer, final_deserializer=__deserializer)
    return stream

BAMLV2_UnionTest_Function.register_impl("default_config")(default_config, default_config_stream)