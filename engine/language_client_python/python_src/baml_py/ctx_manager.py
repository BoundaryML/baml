# Due to tracing, we need to ensure we track context vars for each thread.
# This helps ensure we correctly instantiate the span and context for each thread.

import asyncio
import contextvars
import functools
import inspect
import os
import typing
from .baml_py import BamlLogEvent, RuntimeContextManager, BamlRuntime, BamlSpan
import atexit
import threading
from typing import Dict

F = typing.TypeVar("F", bound=typing.Callable[..., typing.Any])


# See this article about why we need to track for every thread:
# https://kobybass.medium.com/python-contextvars-and-multithreading-faa33dbe953d
RTContextVar = contextvars.ContextVar[typing.Dict[int, RuntimeContextManager]]


def current_thread_id() -> int:
    current_thread = threading.current_thread()
    if hasattr(current_thread, "native_id"):
        return current_thread.native_id or 0
    return current_thread.ident or 0


prev_ctx_manager: typing.Optional["CtxManager"] = None


class CtxManager:
    def __new__(cls, *args, **kwargs):
        if prev_ctx_manager is not None:
            return prev_ctx_manager
        return super().__new__(cls)

    def __init__(self, rt: BamlRuntime):
        global prev_ctx_manager
        if prev_ctx_manager is not None:
            self.rt = prev_ctx_manager.rt
            self.ctx = prev_ctx_manager.ctx
            return

        prev_ctx_manager = self

        self.rt = rt

        self.ctx = contextvars.ContextVar[typing.Dict[int, RuntimeContextManager]](
            "baml_ctx", default={}
        )
        atexit.register(self.rt.flush)

    def __ctx(self) -> RuntimeContextManager:
        ctx = self.ctx.get()
        thread_id = current_thread_id()
        if thread_id not in ctx:
            ctx[thread_id] = self.rt.create_context_manager()
        return ctx[thread_id]

    def allow_reset(self) -> bool:
        ctx = self.ctx.get()

        if len(ctx) > 1:
            print("Too many ctxs!")
            return False

        thread_id = current_thread_id()
        if thread_id not in ctx:
            print("Thread not in ctx!")
            return False

        for c in ctx.values():
            if c.context_depth() > 0:
                print("Context depth is greater than 0!")
                return False

        return True

    def reset(self) -> None:
        self.ctx.set({current_thread_id(): self.rt.create_context_manager()})

    def upsert_tags(self, **tags: str) -> None:
        mngr = self.__ctx()
        mngr.upsert_tags(tags)

    def get(self) -> RuntimeContextManager:
        return self.__ctx()

    def start_trace_sync(
        self,
        name: str,
        args: typing.Dict[str, typing.Any],
        env_vars: typing.Dict[str, str],
    ) -> BamlSpan:
        # Clone the current context before creating the span
        mng = self.__ctx()
        return BamlSpan.new(self.rt, name, args, mng, env_vars)

    def start_trace_async(
        self,
        name: str,
        args: typing.Dict[str, typing.Any],
        env_vars: typing.Dict[str, str],
    ) -> BamlSpan:
        mng = self.__ctx()
        cln = mng.deep_clone()
        self.ctx.set({current_thread_id(): cln})
        return BamlSpan.new(self.rt, name, args, cln, env_vars)

    def clone_context(self) -> RuntimeContextManager:
        mng = self.__ctx()
        cln = mng.deep_clone()
        self.ctx.set({current_thread_id(): cln})
        return cln

    def end_trace(
        self, span: BamlSpan, response: typing.Any, env_vars: typing.Dict[str, str]
    ) -> None:
        span.finish(response, self.__ctx(), env_vars)

    def flush(self) -> None:
        self.rt.flush()

    def on_log_event(
        self, handler: typing.Optional[typing.Callable[[BamlLogEvent], None]]
    ) -> None:
        self.rt.set_log_event_callback(handler)

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
                span = self.start_trace_async(func_name, params, os.environ.copy())
                try:
                    response = await func(*args, **kwargs)
                    self.end_trace(span, response, os.environ.copy())
                    return response
                except Exception as e:
                    self.end_trace(span, e, os.environ.copy())
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
                span = self.start_trace_sync(func_name, params, os.environ.copy())
                try:
                    response = func(*args, **kwargs)
                    self.end_trace(span, response, os.environ.copy())
                    return response
                except Exception as e:
                    print("Except but ending trace!")
                    self.end_trace(span, e, os.environ.copy())
                    raise e

            return typing.cast(F, wrapper)
