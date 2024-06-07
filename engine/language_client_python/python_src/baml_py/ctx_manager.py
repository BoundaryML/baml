# Due to tracing, we need to ensure we track context vars for each thread.
# This helps ensure we correctly instantiate the span and context for each thread.

import asyncio
import contextvars
import functools
import inspect
import typing
from .baml_py import RuntimeContextManager, BamlRuntime, BamlSpan
import atexit
import threading

F = typing.TypeVar("F", bound=typing.Callable[..., typing.Any])


# See this article about why we need to track for every thread:
# https://kobybass.medium.com/python-contextvars-and-multithreading-faa33dbe953d
RTContextVar = contextvars.ContextVar[typing.Dict[int, RuntimeContextManager]]


def current_thread_id() -> int:
    current_thread = threading.current_thread()
    return current_thread.native_id or 0


class CtxManager:
    def __init__(self, rt: BamlRuntime):
        self.rt = rt
        self.ctx = contextvars.ContextVar[typing.Dict[int, RuntimeContextManager]](
            "baml_ctx", default={current_thread_id(): rt.create_context_manager()}
        )
        atexit.register(self.rt.flush)

    def __ctx(self) -> RuntimeContextManager:
        ctx = self.ctx.get()
        thread_id = current_thread_id()
        if thread_id not in ctx:
            ctx[thread_id] = self.rt.create_context_manager()
        return ctx[thread_id]

    def upsert_tags(self, **tags: str) -> None:
        mngr = self.__ctx()
        mngr.upsert_tags(tags)

    def get(self) -> RuntimeContextManager:
        return self.__ctx()

    def start_trace_sync(
        self, name: str, args: typing.Dict[str, typing.Any]
    ) -> BamlSpan:
        mng = self.__ctx()
        return BamlSpan.new(self.rt, name, args, mng)

    def start_trace_async(
        self, name: str, args: typing.Dict[str, typing.Any]
    ) -> BamlSpan:
        mng = self.__ctx()
        cln = mng.deep_clone()
        self.ctx.set({current_thread_id(): cln})
        return BamlSpan.new(self.rt, name, args, cln)

    def end_trace(self, span: BamlSpan, response: typing.Any) -> None:
        span.finish(response, self.__ctx())

    def flush(self) -> None:
        self.rt.flush()

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
                    self.end_trace(span, response)
                    return response
                except Exception as e:
                    self.end_trace(span, e)
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
                    self.end_trace(span, response)
                    return response
                except Exception as e:
                    self.end_trace(span, e)
                    raise e

            return typing.cast(F, wrapper)
