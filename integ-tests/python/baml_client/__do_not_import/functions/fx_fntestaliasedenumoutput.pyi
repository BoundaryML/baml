# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.enums.enm_testenum import TestEnum
from baml_core.stream import AsyncStream
from typing import Callable, Protocol, runtime_checkable


import typing

import pytest
from contextlib import contextmanager
from unittest import mock

ImplName = typing.Literal["v1"]

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)


IFnTestAliasedEnumOutputOutput = TestEnum

@runtime_checkable
class IFnTestAliasedEnumOutput(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: str

    Returns:
        TestEnum
    """

    async def __call__(self, arg: str, /) -> TestEnum:
        ...

   

@runtime_checkable
class IFnTestAliasedEnumOutputStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        arg: str

    Returns:
        AsyncStream[TestEnum, TestEnum]
    """

    def __call__(self, arg: str, /) -> AsyncStream[TestEnum, TestEnum]:
        ...
class BAMLFnTestAliasedEnumOutputImpl:
    async def run(self, arg: str, /) -> TestEnum:
        ...
    
    def stream(self, arg: str, /) -> AsyncStream[TestEnum, TestEnum]:
        ...

class IBAMLFnTestAliasedEnumOutput:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[IFnTestAliasedEnumOutput, IFnTestAliasedEnumOutputStream], None]:
        ...

    async def __call__(self, arg: str, /) -> TestEnum:
        ...

    def stream(self, arg: str, /) -> AsyncStream[TestEnum, TestEnum]:
        ...

    def get_impl(self, name: ImplName) -> BAMLFnTestAliasedEnumOutputImpl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the FnTestAliasedEnumOutputInterface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.FnTestAliasedEnumOutput.mock() as mocked:
                    mocked.return_value = ...
                    result = await FnTestAliasedEnumOutputImpl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnTestAliasedEnumOutputInterface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.FnTestAliasedEnumOutput.test
            async def test_logic(FnTestAliasedEnumOutputImpl: IFnTestAliasedEnumOutput) -> None:
                result = await FnTestAliasedEnumOutputImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName] = [], stream: bool = False) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnTestAliasedEnumOutputInterface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.
            stream: bool
                If set, will return a streamable version of the test function.

        Usage:
            ```python
            # All implementations except the given impl will be tested.

            @baml.FnTestAliasedEnumOutput.test(exclude_impl=["implname"])
            async def test_logic(FnTestAliasedEnumOutputImpl: IFnTestAliasedEnumOutput) -> None:
                result = await FnTestAliasedEnumOutputImpl(...)
            ```

            ```python
            # Streamable version of the test function.

            @baml.FnTestAliasedEnumOutput.test(stream=True)
            async def test_logic(FnTestAliasedEnumOutputImpl: IFnTestAliasedEnumOutputStream) -> None:
                async for result in FnTestAliasedEnumOutputImpl(...):
                    ...
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnTestAliasedEnumOutputInterface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.FnTestAliasedEnumOutput.test
        class TestClass:
            def test_a(self, FnTestAliasedEnumOutputImpl: IFnTestAliasedEnumOutput) -> None:
                ...
            def test_b(self, FnTestAliasedEnumOutputImpl: IFnTestAliasedEnumOutput) -> None:
                ...
        ```
        """
        ...

BAMLFnTestAliasedEnumOutput: IBAMLFnTestAliasedEnumOutput
