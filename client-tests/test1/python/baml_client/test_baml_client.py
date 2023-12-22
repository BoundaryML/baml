# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from .__do_not_import.generated_baml_client import baml
from .baml_types import BasicClass, Categories, IEnumFunc, IGenerateUserChatPrompts, INamedfunc, ZenfetchBotDocumentBase, ZenfetchBotDocumentBaseList
from baml_lib._impl.deserializer import Deserializer
from json5 import dumps # type: ignore, loads # type: ignore


@baml.EnumFunc.test
async def test_incredible_jade(EnumFuncImpl: IEnumFunc):
    deserializer = Deserializer[Categories](Categories) # type: ignore
    param = deserializer.from_string(dumps(TWO))
    await EnumFuncImpl(param)


@baml.Namedfunc.test
async def test_minor_harlequin(NamedfuncImpl: INamedfunc):
    case = loads("""
{"name":null,"address":null}
""")
    deserializer_name = Deserializer[BasicClass](BasicClass) # type: ignore
    name = deserializer_name.from_string(case["name"])
    deserializer_address = Deserializer[str](str) # type: ignore
    address = deserializer_address.from_string(case["address"])
    await NamedfuncImpl(
        name=name,
        address=address
    )


@baml.Namedfunc.test
async def test_nearby_silver(NamedfuncImpl: INamedfunc):
    case = loads("""
{"name":{"name":"asesef 'hello'","age":1,"address":"\"herollo\""},"address":"asdfasdf"}
""")
    deserializer_name = Deserializer[BasicClass](BasicClass) # type: ignore
    name = deserializer_name.from_string(case["name"])
    deserializer_address = Deserializer[str](str) # type: ignore
    address = deserializer_address.from_string(case["address"])
    await NamedfuncImpl(
        name=name,
        address=address
    )


@baml.GenerateUserChatPrompts.test
async def test_substantial_crimson(GenerateUserChatPromptsImpl: IGenerateUserChatPrompts):
    deserializer = Deserializer[ZenfetchBotDocumentBaseList](ZenfetchBotDocumentBaseList) # type: ignore
    param = deserializer.from_string(dumps({"list_of_documents":[{"title":"hello \"there\"","topic":"[ \"hello\"]"}]}))
    await GenerateUserChatPromptsImpl(param)


