import functools
import asyncio
import inspect
from typing import Any, Callable, TypeVar
import typing
from opentelemetry.trace import get_current_span
from .provider import BamlSpanContextManager, baml_tracer, set_tags

F = TypeVar("F", bound=Callable[..., Any])  # Function type


def trace(func: F) -> F:
    return _trace_internal(func)


def _trace_internal(func: F, **kwargs: typing.Any) -> F:
    """
    This is a decorator for internal functions that are not exposed to the user.


    kwargs can be:
    __tags__: dict[str, str]
    __name__: str
    """
    param_names = list(inspect.signature(func).parameters.keys())
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
            params = {param_names[i]: arg for i, arg in enumerate(args)}
            params.update(kwargs)

            parent_id = get_current_span().get_span_context().span_id
            with baml_tracer.start_as_current_span(name) as span:
                with BamlSpanContextManager(
                    func.__name__, parent_id, span, params
                ) as ctx:
                    if tags:
                        set_tags(**tags)
                    response = await func(*args, **kwargs)
                    ctx.complete(response)
                    return response

        return wrapper  # type: ignore
    else:

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {param_names[i]: arg for i, arg in enumerate(args)}
            params.update(kwargs)

            parent_id = get_current_span().get_span_context().span_id
            with baml_tracer.start_as_current_span(name) as span:
                with BamlSpanContextManager(
                    func.__name__, parent_id, span, params
                ) as ctx:
                    if tags:
                        set_tags(**tags)
                    response = func(*args, **kwargs)
                    ctx.complete(response)
                    return response

        return wrapper  # type: ignore
