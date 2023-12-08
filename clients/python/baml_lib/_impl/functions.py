"""
This module provides the implementation for BAML functions.
It includes classes and helper functions to register, run and test BAML functions.
"""

import asyncio
import functools
import inspect
import types
import typing

import pytest

from baml_core.otel import trace, create_event

from pytest_baml.exports import baml_function_test


T = typing.TypeVar("T")


def __parse_arg(arg: typing.Any, t: typing.Type[T], _default: T) -> T:
    """
    Parses the argument based on the provided type.

    Args:
        arg: The argument to parse.
        t: The type to parse the argument into.
        _default: The default value to return if parsing fails.

    Returns:
        The parsed argument or the default value if parsing fails.
    """
    if arg is None:
        return _default
    if isinstance(arg, t):
        return arg
    try:
        return t(arg)  # type: ignore
    except (ValueError, TypeError):
        return _default


RET = typing.TypeVar("RET", covariant=True)


class CB(typing.Generic[RET], typing.Protocol):
    """
    Protocol for a callable object.
    """

    def __call__(
        self, *args: typing.Any, **kwargs: typing.Any
    ) -> typing.Awaitable[RET]:
        ...


class BAMLImpl(typing.Generic[RET]):
    """
    Class representing a BAML implementation.
    """

    __cb: CB[RET]

    def __init__(self, cb: CB[RET]) -> None:
        """
        Initializes a BAML implementation.

        Args:
            cb: The callable object to use for the implementation.
        """
        self.__cb = trace(cb)

    async def run(self, *args: typing.Any, **kwargs: typing.Any) -> RET:
        """
        Runs the BAML implementation.

        Args:
            *args: The arguments to pass to the callable object.
            **kwargs: The arguments to pass to the callable object.

        Returns:
            The result of the callable object.
        """
        return await self.__cb(*args, **kwargs)


class BaseBAMLFunction(typing.Generic[RET]):
    """
    Base class for a BAML function.
    """

    __impls: typing.Dict[str, BAMLImpl[RET]]

    def __init__(
        self, name: str, interface: typing.Any, impl_names: typing.List[str]
    ) -> None:
        """
        Initializes a BAML function.

        Args:
            name: The name of the function.
            interface: The interface for the function.
        """
        self.__impl_names = impl_names
        self.__impls = {}
        self.__name = name
        self.__interface = interface

    def debug_validate(self) -> None:
        """
        Validates the BAML function.
        """
        missing_impls = set(self.__impl_names) - set(self.__impls.keys())
        assert (
            len(missing_impls) == 0
        ), f"Some impls not registered: {self.__name}:{' '.join(missing_impls)}"
        for impl in self.__impls.values():
            assert isinstance(impl, BAMLImpl), f"Invalid impl: {impl}"

    def register_impl(self, name: str) -> typing.Callable[[CB[RET]], None]:
        """
        Registers an implementation for the BAML function.

        Args:
            name: The name of the implementation.

        Returns:
            A decorator to use for the implementation function.
        """
        assert (
            name not in self.__impls
        ), f"Already called: register_impl for {self.__name}:{name}"
        assert (
            name in self.__impl_names
        ), f"Unknown impl: {self.__name}:{name}. Valid impl names: {' '.join(self.__impl_names)}"

        def decorator(cb: CB[RET]) -> None:
            # Runtime check
            sig = inspect.signature(cb)
            expected_sig = inspect.signature(self.__interface.__call__)
            sig_params = list(sig.parameters.values())
            expected_sig_params = list(expected_sig.parameters.values())
            if expected_sig_params and expected_sig_params[0].name == "self":
                expected_sig_params = expected_sig_params[1:]
            assert (
                sig_params == expected_sig_params
            ), f"{self.name} {sig} does not match expected signature {expected_sig}"

            cb.__qualname__ = f'{self.__name}[impl:{cb.__qualname__}]' # type: ignore

            if asyncio.iscoroutinefunction(cb):

                @functools.wraps(cb)
                async def wrapper(
                    *args: typing.Any, **kwargs: typing.Any
                ) -> typing.Any:
                    create_event("variant", {"name": name})
                    return await cb(*args, **kwargs)

            else:

                @functools.wraps(cb)
                def wrapper(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
                    create_event("variant", {"name": name})
                    return cb(*args, **kwargs)

            wrapper.__name__ = self.__name

            self.__impls[name] = BAMLImpl(wrapper)

        return decorator

    def get_impl(self, name: str) -> BAMLImpl[RET]:
        """
        Gets an implementation for the BAML function.

        Args:
            name: The name of the implementation.

        Returns:
            The implementation.
        """
        assert (
            name in self.__impl_names
        ), f"Unknown impl: {self.__name}:{name}. Valid impl names: {' '.join(self.__impl_names)}"
        assert (
            name in self.__impls
        ), f"Never called register_impl for {self.__name}:{name}"
        return self.__impls[name]

    @property
    def name(self) -> str:
        """
        Gets the name of the BAML function.

        Returns:
            The name of the function.
        """
        return self.__name

    @property
    def _impls(self) -> typing.Dict[str, BAMLImpl[RET]]:
        """
        Gets the implementations for the BAML function.

        Returns:
            A dictionary of implementations.
        """
        return self.__impls

    def __parametrize_test_methods(
        self,
        test_class: T,
        excluded_impls: typing.Optional[typing.Iterable[str]] = None,
    ) -> T:
        """
        Applies pytest.mark.parametrize to each test method in the test class.

        Args:
            test_class: The test class to parametrize.
            excluded_impls: The implementations to exclude from the test.

        Returns:
            The parametrized test class.
        """
        selected_impls = filter(
            lambda k: k not in (excluded_impls or []), self.__impls.keys()
        )
        decorator = self.__test_wrapper(selected_impls)

        for attr_name, attr_value in vars(test_class).items():
            if isinstance(attr_value, types.FunctionType) and attr_name.startswith(
                "test_"
            ):
                setattr(
                    test_class,
                    attr_name,
                    decorator(attr_value),
                )
        return test_class

    def __test_wrapper(self, impls: typing.Iterable[str]) -> pytest.MarkDecorator:
        """
        Creates a pytest.mark.parametrize decorator for the given implementations.

        Args:
            impls: The implementations to include in the test.

        Returns:
            A pytest.mark.parametrize decorator.
        """

        return baml_function_test(impls=list(impls), owner=self)

    def test(self, *args: typing.Any, **kwargs: typing.Any) -> typing.Any:
        """
        Creates a test for the BAML function.

        Args:
            *args: The arguments for the test.
            **kwargs: The keyword arguments for the test.

        Returns:
            The test.
        """
        if len(args) == 1:
            if len(kwargs) > 0:
                raise ValueError("To specify parameters, use keyword arguments.")

            if callable(args[0]) and inspect.isclass(args[0]):
                return self.__parametrize_test_methods(args[0])
            elif callable(args[0]):
                return self.__test_wrapper(self.__impls.keys())(args[0])
        if len(args) != 0:
            raise ValueError(
                "Only keyword arguments are supported. Otherwise use without ()."
            )

        excluded_impls = __parse_arg(
            kwargs.get("exclude_impl"),
            typing.cast(typing.Type[typing.Iterable[str]], set),
            set(),
        )
        selected_impls = filter(lambda k: k not in excluded_impls, self.__impls.keys())
        return self.__test_wrapper(selected_impls)
