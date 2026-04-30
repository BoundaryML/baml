# Context manager for BAML runtime — handles contextvars + thread isolation.
#
# Architecture (same as engine/):
# - contextvars.ContextVar provides async isolation (each asyncio.Task
#   gets a snapshot on creation).
# - Dict[thread_id, HostSpanManager] provides thread isolation
#   (ThreadPoolExecutor workers get fresh managers).
# - deep_clone() on async @trace entry forks the span stack so
#   concurrent coroutines from asyncio.gather each get independent copies.
#
# Event lifecycle is handled in Rust (HostSpanManager.enter/exit_ok/exit_error/
# upsert_tags).  Python only drives the control flow and writes the JSONL file.

import asyncio
import contextvars
import functools
import inspect
import threading
import typing
from typing import Any, Dict

from .baml_py import BamlRuntime, HostSpanManager

F = typing.TypeVar("F", bound=typing.Callable[..., typing.Any])


def current_thread_id() -> int:
    current_thread = threading.current_thread()
    if hasattr(current_thread, "native_id"):
        return current_thread.native_id or 0
    return current_thread.ident or 0


prev_ctx_manager: typing.Optional["CtxManager"] = None


class CtxManager:
    """Manages BAML runtime context across async tasks and threads.

    Uses contextvars.ContextVar to provide per-async-task isolation
    and thread_id tracking for per-thread isolation.

    Key methods:
    - trace_fn: Decorator for tracing function calls
    - upsert_tags: Set tags on the current span
    - flush: Write accumulated trace events to disk (JSONL)
    """

    def __new__(cls, *args: typing.Any, **kwargs: typing.Any) -> "CtxManager":
        if prev_ctx_manager is not None:
            return prev_ctx_manager
        return super().__new__(cls)

    def __init__(self, rt: BamlRuntime) -> None:
        global prev_ctx_manager
        if prev_ctx_manager is not None:
            if rt is not prev_ctx_manager.rt:
                import warnings
                warnings.warn(
                    "CtxManager is a singleton; ignoring new BamlRuntime argument",
                    stacklevel=2,
                )
            self.rt = prev_ctx_manager.rt
            self.ctx = prev_ctx_manager.ctx
            return

        prev_ctx_manager = self
        self.rt = rt
        self.ctx: contextvars.ContextVar[
            typing.Dict[int, HostSpanManager]
        ] = contextvars.ContextVar("baml_ctx", default={})

    def __mgr(self) -> HostSpanManager:
        ctx = self.ctx.get()
        thread_id = current_thread_id()
        if thread_id not in ctx:
            ctx[thread_id] = HostSpanManager()
        return ctx[thread_id]

    def get(self) -> HostSpanManager:
        return self.__mgr()

    def allow_reset(self) -> bool:
        ctx = self.ctx.get()
        if len(ctx) > 1:
            return False
        thread_id = current_thread_id()
        if thread_id not in ctx:
            return False
        for c in ctx.values():
            if c.context_depth() > 0:
                return False
        return True

    def reset(self) -> None:
        self.ctx.set(
            {current_thread_id(): HostSpanManager()}
        )

    def clone_context(self) -> HostSpanManager:
        mng = self.__mgr()
        cln = mng.deep_clone()
        self.ctx.set({current_thread_id(): cln})
        return cln

    # ── @trace decorator ──

    def trace_fn(self, func: F) -> F:
        """Decorator that traces a function call.

        Usage:
            ctx = BamlCtxManager(runtime)

            @ctx.trace_fn
            def my_function(x: int) -> int:
                return x * 2

            @ctx.trace_fn
            async def my_async_function(x: int) -> int:
                return x * 2
        """
        func_name = func.__name__
        signature = inspect.signature(func).parameters
        param_names = list(signature.keys())

        if asyncio.iscoroutinefunction(func):

            @functools.wraps(func)
            async def async_wrapper(
                *args: typing.Any, **kwargs: typing.Any
            ) -> typing.Any:
                params = _build_params(args, kwargs, param_names)

                # Fork the span manager (async isolation via deep_clone)
                mgr = self.__mgr()
                clone = mgr.deep_clone()
                self.ctx.set({current_thread_id(): clone})

                clone.enter(func_name, params)
                try:
                    response = await func(*args, **kwargs)
                    clone.exit_ok()
                    return response
                except BaseException as e:
                    clone.exit_error(str(e))
                    raise
                finally:
                    # Restore parent context so sequential siblings
                    # don't inherit this child's tags.
                    self.ctx.set({current_thread_id(): mgr})

            return typing.cast(F, async_wrapper)

        else:

            @functools.wraps(func)
            def sync_wrapper(
                *args: typing.Any, **kwargs: typing.Any
            ) -> typing.Any:
                params = _build_params(args, kwargs, param_names)
                mgr = self.__mgr()

                mgr.enter(func_name, params)
                try:
                    response = func(*args, **kwargs)
                    mgr.exit_ok()
                    return response
                except BaseException as e:
                    mgr.exit_error(str(e))
                    raise

            return typing.cast(F, sync_wrapper)

    # ── Tags ──

    def upsert_tags(self, **tags: str) -> None:
        """Set tags on the current span.

        Merges tags into the HostSpanManager's tag set and emits a
        SetTags event (handled in Rust).
        """
        self.__mgr().upsert_tags(tags)

    # ── Flush ──

    def flush(self) -> None:
        """Flush all buffered trace events to the JSONL file.

        Delegates to the global publisher thread in bex_events which
        handles BAML_TRACE_FILE internally.
        """
        from .baml_py import flush_events

        flush_events()


def _build_params(
    args: tuple, kwargs: dict, param_names: list
) -> Dict[str, Any]:
    """Build a parameter dict from positional + keyword args."""
    params: Dict[str, Any] = {}
    for i, arg in enumerate(args):
        key = param_names[i] if i < len(param_names) else f"<arg:{i}>"
        params[key] = arg
    params.update(kwargs)
    return params
