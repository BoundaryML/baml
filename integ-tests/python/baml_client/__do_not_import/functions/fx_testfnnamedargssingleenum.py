# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.enums.enm_namedargssingleenum import NamedArgsSingleEnum
from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, Protocol, runtime_checkable


ITestFnNamedArgsSingleEnumOutput = str

@runtime_checkable
class ITestFnNamedArgsSingleEnum(Protocol):
    """
    This is the interface for a function.

    Args:
        myArg: NamedArgsSingleEnum

    Returns:
        str
    """

    async def __call__(self, *, myArg: NamedArgsSingleEnum) -> str:
        ...

   

@runtime_checkable
class ITestFnNamedArgsSingleEnumStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        myArg: NamedArgsSingleEnum

    Returns:
        AsyncStream[str, str]
    """

    def __call__(self, *, myArg: NamedArgsSingleEnum
) -> AsyncStream[str, str]:
        ...
class IBAMLTestFnNamedArgsSingleEnum(BaseBAMLFunction[str, str]):
    def __init__(self) -> None:
        super().__init__(
            "TestFnNamedArgsSingleEnum",
            ITestFnNamedArgsSingleEnum,
            ["v1"],
        )

    async def __call__(self, *args, **kwargs) -> str:
        return await self.get_impl("v1").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[str, str]:
        res = self.get_impl("v1").stream(*args, **kwargs)
        return res

BAMLTestFnNamedArgsSingleEnum = IBAMLTestFnNamedArgsSingleEnum()

__all__ = [ "BAMLTestFnNamedArgsSingleEnum" ]