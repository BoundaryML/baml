# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_lib._impl.functions import BaseBAMLFunction
from typing import Protocol, runtime_checkable


IThingOutput = str

@runtime_checkable
class IThing(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: str

    Returns:
        str
    """

    async def __call__(self, arg: str, /) -> str:
        ...


class IBAMLThing(BaseBAMLFunction[str, str]):
    def __init__(self) -> None:
        super().__init__(
            "Thing",
            IThing,
            ["v1"],
        )

    async def __call__(self, *args, **kwargs) -> str:
        return await self.get_impl("v1").run(*args, **kwargs)

BAMLThing = IBAMLThing()

__all__ = [ "BAMLThing" ]
