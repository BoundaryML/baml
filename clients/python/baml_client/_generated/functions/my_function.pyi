import typing

import pytest

ImplName = typing.Literal["foo"]

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])
CLS = typing.TypeVar("CLS", bound=type)

@typing.runtime_checkable
class MyFunctionInferface(typing.Protocol):
    """
    This is the interface for a function.

    Args:
        foo (int): Description for the foo parameter.
        bar (str): Description for the bar parameter.

    Returns:
        str: Description for the return type.
    """

    def __call__(self, *, foo: int, bar: str) -> str: ...

class BAMLMyFunctionImpl:
    def run(self, *, foo: int, bar: str) -> str: ...

class BAMLMyFunction:
    def register_impl(
        self, name: ImplName
    ) -> typing.Callable[[MyFunctionInferface], MyFunctionInferface]: ...
    def get_impl(self, name: ImplName) -> BAMLMyFunctionImpl: ...
    @typing.overload
    def test(self, test_function: T) -> T:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the MyFunctionInterface.

        Parameters:
        -----------
        test_function : T
            The test function to be decorated.

        Usage:
        ------
        ```python
        # All implementations will be tested.

        @baml.MyFunction.test
        def test_logic(MyFunctionTestHandler: MyFunctionInferface) -> None:
            result = MyFunctionTestHandler(foo=42, bar="baz")
            assert (
                result == "foo: 42, bar: baz"
            ), f"Expected 'foo: 42, bar: baz' but got {result}"
        ```
        """
        ...
    @typing.overload
    def test(self, *, exclude_impl: typing.Iterable[ImplName]) -> pytest.MarkDecorator:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the MyFunctionInterface.

        Parameters:
        -----------
        exclude_impl : Iterable[ImplName]
            The names of the implementations to exclude from testing.

        Usage:
        ------
        ```python
        # All implementations except "foo" will be tested.

        @baml.MyFunction.test(exclude_impl=["foo"])
        def test_logic(MyFunctionTestHandler: MyFunctionInferface) -> None:
            result = MyFunctionTestHandler(foo=42, bar="baz")
            assert (
                result == "foo: 42, bar: baz"
            ), f"Expected 'foo: 42, bar: baz' but got {result}"
        ```
        """
        ...
    @typing.overload
    def test(self, test_class: typing.Type[CLS]) -> typing.Type[CLS]:
        """
        Provides a pytest.mark.parametrize decorator to facilitate testing different implementations of
        the MyFunctionInterface.

        Parameters:
        -----------
        test_class : Type[CLS]
            The test class to be decorated.

        Usage:
        ------
        ```python
        # All implementations will be tested in every test method.

        @baml.MyFunction.test
        class TestClass:
            def test_a(self, MyFunctionHandler: MyFunctionInferface) -> None:
                ...
            def test_b(self, MyFunctionHandler: MyFunctionInferface) -> None:
                ...
        """
        ...
