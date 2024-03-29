# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, Optional, Protocol, runtime_checkable


IFnStringOptionalOutput = str

@runtime_checkable
class IFnStringOptional(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: Optional[str]

    Returns:
        str
    """

    async def __call__(self, arg: Optional[str] = None, /) -> str:
        ...

   

@runtime_checkable
class IFnStringOptionalStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        arg: Optional[str]

    Returns:
        AsyncStream[str, str]
    """

    def __call__(self, arg: Optional[str] = None, /) -> AsyncStream[str, str]:
        ...
class IBAMLFnStringOptional(BaseBAMLFunction[str, str]):
    def __init__(self) -> None:
        super().__init__(
            "FnStringOptional",
            IFnStringOptional,
            ["v1"],
        )

    async def __call__(self, *args, **kwargs) -> str:
        return await self.get_impl("v1").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[str, str]:
        res = self.get_impl("v1").stream(*args, **kwargs)
        return res

BAMLFnStringOptional = IBAMLFnStringOptional()

__all__ = [ "BAMLFnStringOptional" ]
