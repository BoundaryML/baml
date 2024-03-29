# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.stream import AsyncStream
from typing import Callable, Optional, Protocol, runtime_checkable


import typing

import pytest
from contextlib import contextmanager
from unittest import mock

ImplName = typing.Literal["v1"]

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)


IFnNamedArgsSingleStringOptionalOutput = str

@runtime_checkable
class IFnNamedArgsSingleStringOptional(Protocol):
    """
    This is the interface for a function.

    Args:
        myString: Optional[str]

    Returns:
        str
    """

    async def __call__(self, *, myString: Optional[str] = None) -> str:
        ...

   

@runtime_checkable
class IFnNamedArgsSingleStringOptionalStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        myString: Optional[str]

    Returns:
        AsyncStream[str, str]
    """

    def __call__(self, *, myString: Optional[str] = None
) -> AsyncStream[str, str]:
        ...
class BAMLFnNamedArgsSingleStringOptionalImpl:
    async def run(self, *, myString: Optional[str] = None) -> str:
        ...
    
    def stream(self, *, myString: Optional[str] = None
) -> AsyncStream[str, str]:
        ...

class IBAMLFnNamedArgsSingleStringOptional:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[IFnNamedArgsSingleStringOptional, IFnNamedArgsSingleStringOptionalStream], None]:
        ...

    async def __call__(self, *, myString: Optional[str] = None) -> str:
        ...

    def stream(self, *, myString: Optional[str] = None
) -> AsyncStream[str, str]:
        ...

    def get_impl(self, name: ImplName) -> BAMLFnNamedArgsSingleStringOptionalImpl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the FnNamedArgsSingleStringOptionalInterface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.FnNamedArgsSingleStringOptional.mock() as mocked:
                    mocked.return_value = ...
                    result = await FnNamedArgsSingleStringOptionalImpl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnNamedArgsSingleStringOptionalInterface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.FnNamedArgsSingleStringOptional.test
            async def test_logic(FnNamedArgsSingleStringOptionalImpl: IFnNamedArgsSingleStringOptional) -> None:
                result = await FnNamedArgsSingleStringOptionalImpl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName] = [], stream: bool = False) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnNamedArgsSingleStringOptionalInterface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.
            stream: bool
                If set, will return a streamable version of the test function.

        Usage:
            ```python
            # All implementations except the given impl will be tested.

            @baml.FnNamedArgsSingleStringOptional.test(exclude_impl=["implname"])
            async def test_logic(FnNamedArgsSingleStringOptionalImpl: IFnNamedArgsSingleStringOptional) -> None:
                result = await FnNamedArgsSingleStringOptionalImpl(...)
            ```

            ```python
            # Streamable version of the test function.

            @baml.FnNamedArgsSingleStringOptional.test(stream=True)
            async def test_logic(FnNamedArgsSingleStringOptionalImpl: IFnNamedArgsSingleStringOptionalStream) -> None:
                async for result in FnNamedArgsSingleStringOptionalImpl(...):
                    ...
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the FnNamedArgsSingleStringOptionalInterface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.FnNamedArgsSingleStringOptional.test
        class TestClass:
            def test_a(self, FnNamedArgsSingleStringOptionalImpl: IFnNamedArgsSingleStringOptional) -> None:
                ...
            def test_b(self, FnNamedArgsSingleStringOptionalImpl: IFnNamedArgsSingleStringOptional) -> None:
                ...
        ```
        """
        ...

BAMLFnNamedArgsSingleStringOptional: IBAMLFnNamedArgsSingleStringOptional
