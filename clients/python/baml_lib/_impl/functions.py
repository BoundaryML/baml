"""
This module provides the implementation for BAML functions.
It includes classes and helper functions to register, run and test BAML functions.
"""

import asyncio
import functools
import inspect
import types
import typing
from unittest import mock
from typing import Callable, Any, Dict, Optional, Type
from types import TracebackType
import pytest

from contextlib import contextmanager
from baml_core.otel import trace, create_event
from baml_core.stream import AsyncStream
from pytest_baml.exports import baml_function_test, baml_function_stream_test


T = typing.TypeVar("T")


def _parse_arg(arg: typing.Any, t: typing.Type[T], _default: T) -> T:
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
RET2 = typing.TypeVar("RET2")
PARTIAL_RET = typing.TypeVar("PARTIAL_RET")


class AsyncGenWrapper(typing.Generic[RET, PARTIAL_RET]):
    def __init__(
        self,
        name: str,
        gen_factory: Callable[..., AsyncStream[RET, PARTIAL_RET]],
        *args: typing.Any,
        **kwargs: typing.Any,
    ) -> None:
        self.name = name
        self.gen_factory = trace(gen_factory)
        self.args = args
        self.kwargs = kwargs
        self.gen_instance: typing.Optional[AsyncStream[RET, PARTIAL_RET]] = None

    async def __aenter__(self) -> "AsyncStream[RET, PARTIAL_RET]":
        self.gen_instance = self.gen_factory(*self.args, **self.kwargs)

        resp = await self.gen_instance.__aenter__()
        create_event("variant", {"name": self.name})

        return resp

    async def __aexit__(
        self,
        exc_type: Optional[Type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[TracebackType],
    ) -> None:
        if not self.gen_instance:
            raise ValueError("The async generator has not been initialized.")
        await self.gen_instance.__aexit__(exc_type, exc_val, exc_tb)


class CB(typing.Generic[RET], typing.Protocol):
    """
    Protocol for a callable object.
    """

    def __call__(
        self, *args: typing.Any, **kwargs: typing.Any
    ) -> typing.Awaitable[RET]: ...


class STREAM_CB(typing.Generic[RET2, PARTIAL_RET], typing.Protocol):
    """
    Protocol for a callable object.
    """

    def __call__(
        self, *args: typing.Any, **kwargs: typing.Any
    ) -> AsyncStream[RET2, PARTIAL_RET]: ...


class BAMLImpl(typing.Generic[RET, PARTIAL_RET]):
    """
    Class representing a BAML implementation.
    """

    __cb: CB[RET]
    __stream_cb: STREAM_CB[RET, PARTIAL_RET]

    def __init__(self, cb: CB[RET], stream_cb: STREAM_CB[RET, PARTIAL_RET]) -> None:
        """
        Initializes a BAML implementation with separate callbacks for regular and stream operations.

        Args:
            cb: The callable object to use for the non-streaming implementation.
            stream_cb: The callable object to use for the streaming implementation.
        """
        self.__cb = trace(cb)
        self.__stream_cb = stream_cb

    async def run(self, *args: Any, **kwargs: Any) -> RET:
        """
        Runs the BAML implementation for non-streaming operations.

        Args:
            *args: The arguments to pass to the callable object.
            **kwargs: The keyword arguments to pass to the callable object.

        Returns:
            The result of the callable object for non-streaming operations.
        """
        return await self.__cb(*args, **kwargs)

    def stream(self, *args: Any, **kwargs: Any) -> AsyncStream[RET, PARTIAL_RET]:
        """
        Streams the BAML implementation.

        Args:
            *args: The arguments to pass to the callable object.
            **kwargs: The keyword arguments to pass to the callable object.

        Returns:
            The result of the callable object for streaming operations.
        """
        res = self.__stream_cb(*args, **kwargs)
        return res


class BaseBAMLFunction(typing.Generic[RET, PARTIAL_RET]):
    """
    Base class for a BAML function.
    """

    __impls: Dict[str, BAMLImpl[RET, PARTIAL_RET]]

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
        missing_impls = set(self.__impl_names) - set(self.__impls.keys())
        assert (
            len(missing_impls) == 0
        ), f"Some impls not registered: {self.__name}:{' '.join(missing_impls)}"
        for impl in self.__impls.values():
            assert isinstance(impl, BAMLImpl), f"Invalid impl: {impl}"

    def register_impl(
        self, name: str
    ) -> Callable[[CB[RET], STREAM_CB[RET, PARTIAL_RET]], None]:
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

        def decorator(cb: CB[RET], stream_cb: STREAM_CB[RET, PARTIAL_RET]) -> None:
            wrapped_cb = self.__register_cb(name, cb)
            wrapped_stream_cb = self.__register_stream_cb(name, stream_cb)
            self.__impls[name] = BAMLImpl(wrapped_cb, wrapped_stream_cb)

        return decorator

    def __register_cb(self, name: str, cb: CB[RET]) -> typing.Any:
        return self.__register_impl_fn(name, cb)

    def __register_stream_cb(
        self, name: str, stream_cb: STREAM_CB[RET, PARTIAL_RET]
    ) -> typing.Any:
        return self.__register_impl_fn(name, stream_cb, is_stream=True)

    def __register_impl_fn(
        self,
        name: str,
        run_impl_fn: Callable[[typing.Any], typing.Any],
        is_stream: bool = False,
    ) -> typing.Any:
        # Runtime check
        sig = inspect.signature(run_impl_fn)
        expected_sig = inspect.signature(self.__interface.__call__)
        sig_params = list(sig.parameters.values())
        expected_sig_params = list(expected_sig.parameters.values())
        if expected_sig_params and expected_sig_params[0].name == "self":
            expected_sig_params = expected_sig_params[1:]
        assert (
            sig_params == expected_sig_params
        ), f"{self.name} {sig} does not match expected signature {expected_sig}"

        if is_stream:
            assert run_impl_fn.__qualname__.endswith(
                "_stream"
            ), "Stream function should end with _stream"
            name_without_stream = run_impl_fn.__qualname__[: -len("_stream")]
            run_impl_fn.__qualname__ = f"{self.__name}[impl:{name_without_stream}]"
        else:
            run_impl_fn.__qualname__ = f"{self.__name}[impl:{run_impl_fn.__qualname__}]"

        run_impl_fn.__annotations__["baml_is_stream"] = is_stream

        if asyncio.iscoroutinefunction(run_impl_fn):
            if is_stream:
                raise ValueError("Stream functions shouldn't be async")

            else:

                @functools.wraps(run_impl_fn)
                async def async_wrapper(
                    *args: typing.Any, **kwargs: typing.Any
                ) -> typing.Any:
                    create_event("variant", {"name": name})
                    return await run_impl_fn(*args, **kwargs)

                async_wrapper.__name__ = self.__name
                return async_wrapper
        else:
            if is_stream:

                @functools.wraps(run_impl_fn)
                def stream_wrapper(
                    *args: typing.Any, **kwargs: typing.Any
                ) -> typing.Any:
                    # For streams override the actual function name (v1_stream).
                    # The qualname already contains impl info anyway and that is what is printed out in the logs.
                    # TODO: Determine how to call create_event("variant", {"name": name}) for streams.
                    run_impl_fn.__name__ = self.__name
                    return AsyncGenWrapper(name, run_impl_fn, *args, **kwargs)

                stream_wrapper.__name__ = self.__name
                return stream_wrapper

            else:

                @functools.wraps(run_impl_fn)
                def wrapper(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
                    create_event("variant", {"name": name})
                    return run_impl_fn(*args, **kwargs)

                wrapper.__name__ = self.__name
                return wrapper

    def get_impl(self, name: str) -> BAMLImpl[RET, PARTIAL_RET]:
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
    def _impls(self) -> typing.Dict[str, BAMLImpl[RET, PARTIAL_RET]]:
        """
        Gets the implementations for the BAML function.

        Returns:
            A dictionary of implementations.
        """
        return self.__impls

    def __parametrize_test_methods(
        self,
        test_class: T,
        use_stream: bool,
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
        decorator = self.__test_wrapper(selected_impls, use_stream=use_stream)

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

    def __test_wrapper(
        self, impls: typing.Iterable[str], *, use_stream: bool
    ) -> pytest.MarkDecorator:
        """
        Creates a pytest.mark.parametrize decorator for the given implementations.

        Args:
            impls: The implementations to include in the test.
            use_stream: Whether to use streaming for the test.

        Returns:
            A pytest.mark.parametrize decorator.
        """

        if use_stream:
            return baml_function_stream_test(impls=list(impls), owner=self, stream=True)
        return baml_function_test(impls=list(impls), owner=self)

    @contextmanager
    def mock(self) -> typing.Generator[mock.AsyncMock, None, None]:
        mocked_impl = mock.AsyncMock()

        base_line = {
            name: mock.patch.object(impl, "run", new=mocked_impl)
            for name, impl in self._impls.items()
        }

        for patch in base_line.values():
            patch.start()

        try:
            yield mocked_impl
        finally:
            for patch in base_line.values():
                patch.stop()

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
                return self.__parametrize_test_methods(args[0], use_stream=False)
            elif callable(args[0]):
                return self.__test_wrapper(self.__impls.keys(), use_stream=False)(
                    args[0]
                )
        if len(args) != 0:
            raise ValueError(
                "Only keyword arguments are supported. Otherwise use without ()."
            )

        excluded_impls = _parse_arg(
            kwargs.get("exclude_impl"),
            typing.cast(typing.Type[typing.Iterable[str]], set),
            set(),
        )
        use_stream = kwargs.get("stream", False)
        selected_impls = filter(lambda k: k not in excluded_impls, self.__impls.keys())
        return self.__test_wrapper(selected_impls, use_stream=use_stream)
