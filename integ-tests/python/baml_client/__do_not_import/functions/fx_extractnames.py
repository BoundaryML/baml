# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, List, Protocol, runtime_checkable


IExtractNamesOutput = List[str]

@runtime_checkable
class IExtractNames(Protocol):
    """
    This is the interface for a function.

    Args:
        input: str

    Returns:
        List[str]
    """

    async def __call__(self, *, input: str) -> List[str]:
        ...

   

@runtime_checkable
class IExtractNamesStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        input: str

    Returns:
        AsyncStream[List[str], List[str]]
    """

    def __call__(self, *, input: str
) -> AsyncStream[List[str], List[str]]:
        ...
class IBAMLExtractNames(BaseBAMLFunction[List[str], List[str]]):
    def __init__(self) -> None:
        super().__init__(
            "ExtractNames",
            IExtractNames,
            ["default_config"],
        )

    async def __call__(self, *args, **kwargs) -> List[str]:
        return await self.get_impl("default_config").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[List[str], List[str]]:
        res = self.get_impl("default_config").stream(*args, **kwargs)
        return res

BAMLExtractNames = IBAMLExtractNames()

__all__ = [ "BAMLExtractNames" ]
