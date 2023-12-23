# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.classes.cls_basicclass import BasicClass
from baml_lib._impl.functions import BaseBAMLFunction
from typing import Protocol, runtime_checkable


IClassFuncOutput = str

@runtime_checkable
class IClassFunc(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: BasicClass

    Returns:
        str
    """

    async def __call__(self, arg: BasicClass, /) -> str:
        ...


class IBAMLClassFunc(BaseBAMLFunction[str]):
    def __init__(self) -> None:
        super().__init__(
            "ClassFunc",
            IClassFunc,
            ["version"],
        )

    async def __call__(self, *args, **kwargs) -> str:
        return await self.get_impl("version").run(*args, **kwargs)

BAMLClassFunc = IBAMLClassFunc()

__all__ = [ "BAMLClassFunc" ]