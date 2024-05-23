import asyncio
import functools
import inspect
import typing
from .baml_py import BamlSpan, BamlRuntimeFfi

F = typing.TypeVar(
    "F",
    bound=typing.Callable[
        ...,
        typing.Coroutine[
            typing.Any, typing.Any, typing.AsyncGenerator[typing.Any, None]
        ],
    ],
)


def rt_trace(rt: BamlRuntimeFfi, func: F) -> F:
    func_name = func.__name__
    signature = inspect.signature(func).parameters
    param_names = list(signature.keys())

    if asyncio.iscoroutinefunction(func):

        @functools.wraps(func)
        async def async_wrapper(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
            params = {
                param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                for i, arg in enumerate(args)
            }
            params.update(kwargs)
            span = BamlSpan.new(rt, func_name, {}, params)
            try:
                return await func(*args, **kwargs)
            finally:
                await span.finish(None)

        return typing.cast(F, async_wrapper)

    else:

        @functools.wraps(func)
        def wrapper(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
            params = {
                param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                for i, arg in enumerate(args)
            }
            params.update(kwargs)
            span = BamlSpan.new(rt, func_name, {}, params)
            try:
                return func(*args, **kwargs)
            finally:
                asyncio.run(span.finish(None))

        return typing.cast(F, wrapper)
