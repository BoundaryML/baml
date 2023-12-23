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


IIntFuncOutput = str

@runtime_checkable
class IIntFunc(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: int

    Returns:
        str
    """

    async def __call__(self, arg: int, /) -> str:
        ...


class BAMLIntFuncImpl:
    async def run(self, arg: int, /) -> str:
        ...

class IBAMLIntFunc:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[IIntFunc], IIntFunc]:
        ...

    async def __call__(self, arg: int, /) -> str:
        ...

    def get_impl(self, name: ImplName) -> BAMLIntFuncImpl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the IntFuncInterface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.IntFunc.mock() as mocked:
                    mocked.return_value = ...
                    result = await IntFuncImpl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the IntFuncInterface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.IntFunc.test
            async def test_logic(IntFuncImpl: IIntFunc) -> None:
                result = await IntFuncImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName]) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the IntFuncInterface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.

        Usage:
            ```python
            # All implementations except "v1" will be tested.

            @baml.IntFunc.test(exclude_impl=["v1"])
            async def test_logic(IntFuncImpl: IIntFunc) -> None:
                result = await IntFuncImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the IntFuncInterface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.IntFunc.test
        class TestClass:
            def test_a(self, IntFuncImpl: IIntFunc) -> None:
                ...
            def test_b(self, IntFuncImpl: IIntFunc) -> None:
                ...
        ```
        """
        ...

BAMLIntFunc: IBAMLIntFunc