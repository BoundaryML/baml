# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.classes.cls_linteroutput import LinterOutput
from ..types.partial.classes.cls_linteroutput import PartialLinterOutput
from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, List, Protocol, runtime_checkable


IChainOfThoughtOutput = List[LinterOutput]

@runtime_checkable
class IChainOfThought(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: str

    Returns:
        List[LinterOutput]
    """

    async def __call__(self, arg: str, /) -> List[LinterOutput]:
        ...

   

@runtime_checkable
class IChainOfThoughtStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        arg: str

    Returns:
        AsyncStream[List[LinterOutput], List[LinterOutput]]
    """

    def __call__(self, arg: str, /) -> AsyncStream[List[LinterOutput], List[LinterOutput]]:
        ...
class IBAMLChainOfThought(BaseBAMLFunction[List[LinterOutput], List[LinterOutput]]):
    def __init__(self) -> None:
        super().__init__(
            "ChainOfThought",
            IChainOfThought,
            ["version1"],
        )

    async def __call__(self, *args, **kwargs) -> List[LinterOutput]:
        return await self.get_impl("version1").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[List[LinterOutput], List[LinterOutput]]:
        res = self.get_impl("version1").stream(*args, **kwargs)
        return res

BAMLChainOfThought = IBAMLChainOfThought()

__all__ = [ "BAMLChainOfThought" ]
