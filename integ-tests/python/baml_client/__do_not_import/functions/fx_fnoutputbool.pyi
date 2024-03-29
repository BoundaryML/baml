# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.stream import AsyncStream
from typing import Callable, Protocol, runtime_checkable


import typing

import pytest
from contextlib import contextmanager
from unittest import mock

ImplName = typing.Literal["v1"]

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)


IFnOutputBoolOutput = bool

@runtime_checkable
class IFnOutputBool(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: str

    Returns:
        bool
    """

    async def __call__(self, arg: str, /) -> bool:
        ...

   

@runtime_checkable
class IFnOutputBoolStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        arg: str

    Returns:
        AsyncStream[bool, bool]
    """

    def __call__(self, arg: str, /) -> AsyncStream[bool, bool]:
        ...
class BAMLFnOutputBoolImpl:
    async def run(self, arg: str, /) -> bool:
        ...
    
    def stream(self, arg: str, /) -> AsyncStream[bool, bool]:
        ...

class IBAMLFnOutputBool:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[IFnOutputBool, IFnOutputBoolStream], None]:
        ...

    async def __call__(self, arg: str, /) -> bool:
        ...

    def stream(self, arg: str, /) -> AsyncStream[bool, bool]:
        ...

    def get_impl(self, name: ImplName) -> BAMLFnOutputBoolImpl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the FnOutputBoolInterface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.FnOutputBool.mock() as mocked:
                    mocked.return_value = ...
                    result = await FnOutputBoolImpl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnOutputBoolInterface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.FnOutputBool.test
            async def test_logic(FnOutputBoolImpl: IFnOutputBool) -> None:
                result = await FnOutputBoolImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName] = [], stream: bool = False) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnOutputBoolInterface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.
            stream: bool
                If set, will return a streamable version of the test function.

        Usage:
            ```python
            # All implementations except the given impl will be tested.

            @baml.FnOutputBool.test(exclude_impl=["implname"])
            async def test_logic(FnOutputBoolImpl: IFnOutputBool) -> None:
                result = await FnOutputBoolImpl(...)
            ```

            ```python
            # Streamable version of the test function.

            @baml.FnOutputBool.test(stream=True)
            async def test_logic(FnOutputBoolImpl: IFnOutputBoolStream) -> None:
                async for result in FnOutputBoolImpl(...):
                    ...
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnOutputBoolInterface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.FnOutputBool.test
        class TestClass:
            def test_a(self, FnOutputBoolImpl: IFnOutputBool) -> None:
                ...
            def test_b(self, FnOutputBoolImpl: IFnOutputBool) -> None:
                ...
        ```
        """
        ...

BAMLFnOutputBool: IBAMLFnOutputBool
