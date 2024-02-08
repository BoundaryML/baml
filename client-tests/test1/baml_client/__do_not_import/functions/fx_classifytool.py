# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.classes.cls_classifyresponse import ClassifyResponse
from ..types.enums.enm_tool import Tool
from ..types.partial.classes.cls_classifyresponse import PartialClassifyResponse
from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, Protocol, runtime_checkable


IClassifyToolOutput = ClassifyResponse

@runtime_checkable
class IClassifyTool(Protocol):
    """
    This is the interface for a function.

    Args:
        query: str
        context: str

    Returns:
        ClassifyResponse
    """

    async def __call__(self, *, query: str, context: str) -> ClassifyResponse:
        ...

   

@runtime_checkable
class IClassifyToolStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        query: str
        context: str

    Returns:
        AsyncStream[ClassifyResponse, PartialClassifyResponse]
    """

    def __call__(self, *, query: str, context: str
) -> AsyncStream[ClassifyResponse, PartialClassifyResponse]:
        ...
class IBAMLClassifyTool(BaseBAMLFunction[ClassifyResponse, PartialClassifyResponse]):
    def __init__(self) -> None:
        super().__init__(
            "ClassifyTool",
            IClassifyTool,
            ["v1"],
        )

    async def __call__(self, *args, **kwargs) -> ClassifyResponse:
        return await self.get_impl("v1").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[ClassifyResponse, PartialClassifyResponse]:
        res = self.get_impl("v1").stream(*args, **kwargs)
        return res

BAMLClassifyTool = IBAMLClassifyTool()

__all__ = [ "BAMLClassifyTool" ]
