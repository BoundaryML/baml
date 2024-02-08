import functools
import asyncio
import inspect
from typing import Any, Callable, TypeVar, Coroutine, AsyncGenerator
import typing
from opentelemetry.trace import get_current_span
from .provider import BamlSpanContextManager, baml_tracer, set_tags
from baml_core.stream import AsyncStream
import sys


def trace(*args, **kwargs) -> Any:
    if len(args) == 1:
        func = args[0]
        assert callable(func), f"Expected func to be callable, got {func}"
        return _trace_internal(func)
    else:
        assert not args, f"Unexpected args: {args}"
        assert kwargs, f"Expected kwargs: {kwargs}"
        name = kwargs.pop("name")
        assert isinstance(name, str), f"Expected name to be a str, got {name}"
        assert not kwargs, f"Unexpected kwargs: {kwargs}"

        def wrapper(func: F) -> F:
            return _trace_internal(func, __name__=name)

        return wrapper


F = TypeVar("F", bound=Callable[..., Coroutine[Any, Any, AsyncGenerator[Any, None]]])


class AsyncGeneratorContextManager:
    def __init__(
        self,
        gen_factory: Callable[..., AsyncStream[Any, Any]],
        name,
        params,
        *args,
        **kwargs,
    ):
        self.gen_factory = gen_factory
        self.name = name
        self.params = params
        self.args = args
        self.kwargs = kwargs
        self.gen_instance = None
        self.span = None
        self.span_context = None
        self.ctx = None

    async def __aenter__(self) -> "AsyncStream":
        name = self.name
        parent_id = get_current_span().get_span_context().span_id
        tags = self.kwargs.get("tags", {})

        # Start the tracing span
        self.span_context = baml_tracer.start_as_current_span(name)
        self.span = self.span_context.__enter__()
        # Enter the custom context manager with tracing and context setup
        self.ctx = BamlSpanContextManager(name, parent_id, self.span, self.params)
        self.ctx.__enter__()

        if tags:
            set_tags(**tags)

        self.gen_instance = self.gen_factory(*self.args, **self.kwargs)
        await self.gen_instance.__aenter__()
        return self.gen_instance

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if not self.gen_instance:
            raise ValueError("The async generator has not been initialized.")

        try:
            await self.gen_instance.__aexit__(exc_type, exc_val, exc_tb)
            final_res = await self.gen_instance.get_final_response()
            if self.ctx:
                self.ctx.complete(final_res.value)
        except Exception:
            exc_type, exc_val, exc_tb = sys.exc_info()

        if self.ctx:
            self.ctx.__exit__(exc_type, exc_val, exc_tb)
        if self.span_context:
            self.span_context.__exit__(exc_type, exc_val, exc_tb)


def _trace_internal(func: F, **kwargs: typing.Any) -> F:
    """
    This is a decorator for internal functions that are not exposed to the user.


    kwargs can be:
    __tags__: dict[str, str]
    __name__: str
    """
    signature = inspect.signature(func).parameters
    param_names = list(signature.keys())
    tags = kwargs.pop("__tags__", {})
    name = kwargs.pop("__name__", func.__name__)
    # Validate that the user is passing in the correct kwargs types
    assert isinstance(tags, dict), f"Expected tags to be a dict, got {tags}"
    for key, value in tags.items():
        assert isinstance(key, str), f"Expected key to be a str, got {key}"
    assert isinstance(name, str), f"Expected name to be a str, got {name}"

    # Ensure that the user doesn't pass in any other kwargs
    assert not kwargs, f"Unexpected kwargs: {kwargs}"

    # TODO: find a resilient way to check
    if "_stream" in func.__qualname__:

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> AsyncGeneratorContextManager:
            params = {
                param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                for i, arg in enumerate(args)
            }
            params.update(kwargs)

            return AsyncGeneratorContextManager(func, name, params, *args, **kwargs)  # type: ignore

        return typing.cast(F, wrapper)

    if asyncio.iscoroutinefunction(func):

        @functools.wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {
                param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                for i, arg in enumerate(args)
            }
            params.update(kwargs)

            parent_id = get_current_span().get_span_context().span_id
            with baml_tracer.start_as_current_span(name) as span:
                with BamlSpanContextManager(name, parent_id, span, params) as ctx:
                    ctx.span.get_span_context().span_id
                    if tags:
                        set_tags(**tags)
                    response = await func(*args, **kwargs)
                    ctx.complete(response)
                    return response

        return typing.cast(F, wrapper)
    else:

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {
                param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                for i, arg in enumerate(args)
            }
            params.update(kwargs)

            parent_id = get_current_span().get_span_context().span_id

            with baml_tracer.start_as_current_span(name) as span:
                with BamlSpanContextManager(name, parent_id, span, params) as ctx:
                    if tags:
                        set_tags(**tags)
                    response = func(*args, **kwargs)

                    ctx.complete(response)
                    return response

        return wrapper  # type: ignore
