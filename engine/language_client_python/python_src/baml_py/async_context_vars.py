# Due to tracing, we need to ensure we track context vars for each thread.
# This helps ensure we correctly instantiate the span and context for each thread.

import asyncio
import contextvars
import functools
import inspect
import typing
from .baml_py import RuntimeContextManagerPy, BamlRuntimeFfi, BamlSpan
import atexit

F = typing.TypeVar("F", bound=typing.Callable[..., typing.Any])


class CtxManager:
    def __init__(self, rt: BamlRuntimeFfi):
        self.rt = rt
        self.ctx = contextvars.ContextVar[RuntimeContextManagerPy](
            "baml_ctx", default=rt.create_context_manager()
        )
        atexit.register(self.rt.flush)

    def upsert_tags(self, tags: typing.Dict[str, str]) -> None:
        mngr = self.ctx.get()
        mngr.upsert_tags(tags)

    def get(self) -> RuntimeContextManagerPy:
        return self.ctx.get()

    def start_trace_sync(
        self, name: str, args: typing.Dict[str, typing.Any]
    ) -> BamlSpan:
        mng = self.ctx.get()
        return BamlSpan.new(self.rt, name, args, mng)

    def start_trace_async(
        self, name: str, args: typing.Dict[str, typing.Any]
    ) -> BamlSpan:
        mng = self.ctx.get()
        cln = mng.deep_clone()
        self.ctx.set(cln)
        return BamlSpan.new(self.rt, name, args, cln)

    async def end_trace(self, span: BamlSpan, response: typing.Any) -> None:
        await span.finish(response, self.ctx.get())

    def trace_fn(self, func: F) -> F:
        func_name = func.__name__
        signature = inspect.signature(func).parameters
        param_names = list(signature.keys())

        if asyncio.iscoroutinefunction(func):

            @functools.wraps(func)
            async def async_wrapper(
                *args: typing.Any, **kwargs: typing.Any
            ) -> typing.Any:
                params = {
                    param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                    for i, arg in enumerate(args)
                }
                params.update(kwargs)
                span = self.start_trace_async(func_name, params)
                try:
                    response = await func(*args, **kwargs)
                    await self.end_trace(span, response)
                    return response
                except Exception as e:
                    await self.end_trace(span, e)
                    raise e

            return typing.cast(F, async_wrapper)

        else:

            @functools.wraps(func)
            def wrapper(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
                params = {
                    param_names[i] if i < len(param_names) else f"<arg:{i}>": arg
                    for i, arg in enumerate(args)
                }
                params.update(kwargs)
                span = self.start_trace_sync(func_name, params)
                try:
                    response = func(*args, **kwargs)
                    asyncio.run(self.end_trace(span, response))
                    return response
                except Exception as e:
                    asyncio.run(self.end_trace(span, e))
                    raise e

            return typing.cast(F, wrapper)
