from typing import (
    Any,
    Callable,
    Iterable,
    Literal,
    Protocol,
    TypeVar,
    Type,
    overload,
)
import pytest

ImplName = Literal["foo"]

T = TypeVar("T", bound=Callable[..., Any])
CLS = TypeVar("CLS")

class MyFunctionInferface(Protocol):
    def __call__(self, *, foo: int, bar: str) -> str: ...

class BAMLImpl:
    def run(self, *, foo: int, bar: str) -> str: ...

class BAMLFunction:
    def debug_validate(self) -> None:
        """
        Ensures that the BAMLFunction is in a valid state.
        """
        ...
    def register_impl(self, name: ImplName) -> Callable[[MyFunctionInferface], None]:
        """
        Register an implementation for BAMLFunction.

        Parameters:
        -----------
        name : ImplName
            The name of the implementation.

        Returns:
        --------
        Callable[[MyFunctionInferface], None]
            A decorator that registers the implementation for BAMLFunction.
        """
        ...
    def get_impl(self, name: ImplName) -> BAMLImpl:
        """
        Get the implementation for BAMLFunction.

        Parameters:
        -----------
        name : ImplName
            The name of the implementation.

        Returns:
        --------
        BAMLImpl
            The implementation for BAMLFunction.
        """
        ...
    @overload
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
    @overload
    def test(self, *, exclude_impl: Iterable[ImplName]) -> pytest.MarkDecorator:
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
    @overload
    def test(self, test_class: Type[CLS]) -> Type[CLS]:
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

class BAMLClient:
    MyFunction: BAMLFunction

baml: BAMLClient
