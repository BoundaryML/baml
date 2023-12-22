# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from typing import Protocol, runtime_checkable


import typing

import pytest
from contextlib import contextmanager
from unittest import mock

ImplName = typing.Literal["v1"]

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)


IStringFuncOutput = str

@runtime_checkable
class IStringFunc(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: str

    Returns:
        str
    """

    async def __call__(self, arg: str, /) -> str:
        ...


class BAMLStringFuncImpl:
    async def run(self, arg: str, /) -> str:
        ...

class IBAMLStringFunc:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[IStringFunc], IStringFunc]:
        ...

    async def __call__(self, arg: str, /) -> str:
        ...

    def get_impl(self, name: ImplName) -> BAMLStringFuncImpl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the StringFuncInterface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.StringFunc.mock() as mocked:
                    mocked.return_value = ...
                    result = await StringFuncImpl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the StringFuncInterface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.StringFunc.test
            async def test_logic(StringFuncImpl: IStringFunc) -> None:
                result = await StringFuncImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName]) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the StringFuncInterface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.

        Usage:
            ```python
            # All implementations except "v1" will be tested.

            @baml.StringFunc.test(exclude_impl=["v1"])
            async def test_logic(StringFuncImpl: IStringFunc) -> None:
                result = await StringFuncImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the StringFuncInterface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.StringFunc.test
        class TestClass:
            def test_a(self, StringFuncImpl: IStringFunc) -> None:
                ...
            def test_b(self, StringFuncImpl: IStringFunc) -> None:
                ...
        ```
        """
        ...

BAMLStringFunc: IBAMLStringFunc
