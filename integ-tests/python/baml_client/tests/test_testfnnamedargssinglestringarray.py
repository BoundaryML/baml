# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..__do_not_import.generated_baml_client import baml
from ..baml_types import ITestFnNamedArgsSingleStringArray, ITestFnNamedArgsSingleStringArrayStream
from baml_lib._impl.deserializer import Deserializer
from json import dumps
from pytest_baml.ipc_channel import BaseIPCChannel
from typing import Any, List


@baml.TestFnNamedArgsSingleStringArray.test(stream=True)
async def test_ministerial_beige(TestFnNamedArgsSingleStringArrayImpl: ITestFnNamedArgsSingleStringArrayStream, baml_ipc_channel: BaseIPCChannel):
    def to_str(item: Any) -> str:
        if isinstance(item, str):
            return item
        return dumps(item)

    case = {"myStringArray": ["hello there!\n\nhow are you.", "im doing fine'"], }
    deserializer_myStringArray = Deserializer[List[str]](List[str]) # type: ignore
    myStringArray = deserializer_myStringArray.from_string(to_str(case["myStringArray"]))
    async with TestFnNamedArgsSingleStringArrayImpl(
        myStringArray=myStringArray
    ) as stream:
        async for response in stream.parsed_stream:
            baml_ipc_channel.send("partial_response", response.json())

        await stream.get_final_response()
