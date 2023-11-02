import functools
import asyncio
import inspect
from typing import Any, Callable, TypeVar
from opentelemetry.trace import get_current_span, use_span
from .provider import BamlSpanContextManager, baml_tracer

F = TypeVar("F", bound=Callable[..., Any])  # Function type


def trace(func: F) -> F:
    param_names = list(inspect.signature(func).parameters.keys())

    if asyncio.iscoroutinefunction(func):

        @functools.wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            params = {param_names[i]: arg for i, arg in enumerate(args)}
            params.update(kwargs)

            parent_id = get_current_span().get_span_context().span_id
            with use_span(baml_tracer.start_span(func.__name__)) as span:
                with BamlSpanContextManager(parent_id, span, params) as ctx:
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
            with baml_tracer.start_as_current_span(func.__name__) as span:
                with BamlSpanContextManager(parent_id, span, params) as ctx:
                    response = func(*args, **kwargs)
                    ctx.complete(response)
                    return response

        return wrapper  # type: ignore
