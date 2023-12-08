import functools
import asyncio
import inspect
from typing import Any, Callable, TypeVar
import typing
from opentelemetry.trace import get_current_span
import typeguard
from .provider import BamlSpanContextManager, baml_tracer, set_tags

F = TypeVar("F", bound=Callable[..., Any])  # Function type


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

    if asyncio.iscoroutinefunction(func):

        @functools.wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {param_names[i] if i < len(param_names) else f"<arg:{i}>": arg for i, arg in enumerate(args)}
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

        return wrapper  # type: ignore
    else:

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {param_names[i] if i < len(param_names) else f"<arg:{i}>": arg for i, arg in enumerate(args)}
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
