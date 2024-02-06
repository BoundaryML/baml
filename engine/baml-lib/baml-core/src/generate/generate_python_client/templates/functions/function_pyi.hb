import typing

import pytest
from contextlib import contextmanager
from unittest import mock

{{#if has_impls}}
ImplName = typing.Literal[{{#each impls}}"{{this}}"{{#unless @last}}, {{/unless}}{{/each}}]
{{else}}
ImplName = type(None)
{{/if}}

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)


{{> interface}}

class BAML{{name}}Impl:
    {{> method_def func_name="run" unnamed_args=this.unnamed_args args=this.args return=this.return}}
        ...
    
    def stream(self, {{> func_params unnamed_args=this.unnamed_args args=this.args}}) -> typing.AsyncIterator[BAMLStreamResponse[{{return.0.type}}, Partial{{return.0.type}}]]:
        ...

class IBAML{{name}}:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[
        [I{{name}}, 
        Callable[[{{> arg_types_list unnamed_args=this.unnamed_args args=this.args}}], typing.AsyncIterator[BAMLStreamResponse[{{return.0.type}}, Partial{{return.0.type}}]]]
        ], 
        None
    ]:
        ...

    {{> method_def func_name="__call__" unnamed_args=this.unnamed_args args=this.args return=this.return}}
        ...

    def stream(self, {{> func_params unnamed_args=this.unnamed_args args=this.args}}) -> typing.AsyncIterator[BAMLStreamResponse[{{return.0.type}}, Partial{{return.0.type}}]]:
        ...

    def get_impl(self, name: ImplName) -> BAML{{name}}Impl:
        ...

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        """
        Utility for mocking the {{name}}Interface.

        Usage:
            ```python
            # All implementations are mocked.

            async def test_logic() -> None:
                with baml.{{name}}.mock() as mocked:
                    mocked.return_value = ...
                    result = await {{name}}Impl(...)
                    assert mocked.called
            ```
        """
        ...

    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the {{name}}Interface.

        Args:
            test_function : T
                The test function to be decorated.

        Usage:
            ```python
            # All implementations will be tested.

            @baml.{{name}}.test
            async def test_logic({{name}}Impl: I{{name}}) -> None:
                result = await {{name}}Impl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName]) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the {{name}}Interface.

        Args:
            exclude_impl : Iterable[ImplName]
                The names of the implementations to exclude from testing.

        Usage:
            ```python
            # All implementations except the given impl will be tested.

            @baml.{{name}}.test(exclude_impl=["implname"])
            async def test_logic({{name}}Impl: I{{name}}) -> None:
                result = await {{name}}Impl(...)
            ```
        """
        ...

    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the {{name}}Interface.

        Args:
            test_class : Type[CLS]
                The test class to be decorated.

        Usage:
        ```python
        # All implementations will be tested in every test method.

        @baml.{{name}}.test
        class TestClass:
            def test_a(self, {{name}}Impl: I{{name}}) -> None:
                ...
            def test_b(self, {{name}}Impl: I{{name}}) -> None:
                ...
        ```
        """
        ...

BAML{{name}}: IBAML{{name}}
