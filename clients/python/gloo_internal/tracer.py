from __future__ import annotations

import functools
import inspect
import traceback
from types import TracebackType
import typing
import asyncio
from datetime import datetime
import uuid
from contextvars import ContextVar

from . import api_types
from .env import ENV
from .api import API

current_trace_id: ContextVar[typing.Optional[str]] = ContextVar(
    "current_trace_id", default=None
)
first_trace_id: ContextVar[typing.Optional[str]] = ContextVar(
    "first_trace_id", default=None
)


class TraceStackItem:
    def __init__(self, *, _id: str, func: str) -> None:
        self.__id = _id
        self.__func = func

    @property
    def id(self) -> str:
        return self.__id

    @property
    def func(self) -> str:
        return self.__func


trace_stack: ContextVar[typing.List[TraceStackItem]] = ContextVar(
    "trace_stack", default=[]
)


class ContextVarStorage:
    llmMetadata: typing.Optional[api_types.LLMEventSchema] = None

    def __init__(self, _id: str, tags: typing.Dict[str, str], io: api_types.IO) -> None:
        self._id = _id
        self.tags = tags
        self.io = io
        self.start_time = datetime.utcnow()
        self.error: typing.Optional[api_types.Error] = None

    async def emit(
        self,
        *,
        error: typing.Optional[api_types.Error],
        io: api_types.IO,
        # Always includes self.
        call_history: typing.List[TraceStackItem],
    ) -> None:
        if not call_history or call_history[-1].id != self._id:
            raise Exception("Call history is not valid.")

        latency_ms = int((datetime.utcnow() - self.start_time).total_seconds() * 1000)
        variant_name = self.tags.pop("__variant", None)

        schema_context = api_types.LogSchemaContext(
            start_time=self.start_time.isoformat() + "Z",
            hostname=ENV.HOSTNAME,
            process_id=ENV.GLOO_PROCESS_ID,
            stage=ENV.GLOO_STAGE,
            latency_ms=latency_ms,
            tags=self.tags,
            event_chain=[
                api_types.EventChain(function_name=x.func, variant_name=variant_name)
                for x in call_history
            ],
        )
        payload = api_types.LogSchema(
            project_id="",
            event_type="func_llm" if self.llmMetadata else "func_code",
            event_id=self._id,
            parent_event_id=call_history[-2].id if len(call_history) > 1 else None,
            root_event_id=call_history[0].id,
            context=schema_context,
            error=error or self.error,
            io=io,
            metadata=self.llmMetadata,
        )

        try:
            await API.log(payload=payload)
        except Exception:
            pass


context_storage: ContextVar[typing.Dict[str, ContextVarStorage]] = ContextVar(
    "context_storage", default={}
)

T = typing.TypeVar("T", bound=typing.Callable[..., typing.Any])


class TraceContext:
    io: api_types.IO

    def __init__(
        self,
        name: str,
        func: T,
        args: typing.Tuple[typing.Any, ...],
        kwargs: typing.Dict[str, typing.Any],
    ) -> None:
        self.func_name = name
        self.args = args
        self.kwargs = kwargs

        self.tags = kwargs.pop("__tags", {})
        # Ensure tags are a Dict[str, str]
        if not isinstance(self.tags, dict):
            raise Exception(
                f"Tags must be a Dict[str, str], got {type(self.tags)} instead."
            )
        for key, value in self.tags.items():
            if not isinstance(key, str):
                raise Exception(f"Tag keys must be strings, got {type(key)} instead.")
            if not isinstance(value, str):
                raise Exception(
                    f"Tag values must be strings, got {type(value)} instead."
                )

        self.uid = str(uuid.uuid4())

        param_names = list(inspect.signature(func).parameters.keys())
        params = {param_names[i]: arg for i, arg in enumerate(args)}
        params.update(kwargs)

        if "self" in params:
            params.pop("self")

        if "cls" in params:
            params.pop("cls")

        if len(params) == 0:
            self.io = api_types.IO(input=None, output=None)
        elif len(params) == 1:
            _, value = params.popitem()
            self.io = api_types.IO(
                input=api_types.IOValue(
                    value=value,
                    type=api_types.TypeSchema(name=type(value).__name__, fields={}),
                ),
                output=None,
            )
        else:
            self.io: api_types.IO = api_types.IO(
                input=api_types.IOValue(
                    value=params,
                    type=api_types.TypeSchema(
                        name="Dict[str, str]",
                        fields={
                            name: type(value).__name__ for name, value in params.items()
                        },
                    ),
                ),
                output=None,
            )

    async def __aenter__(self) -> "TraceContext":
        self._enter()
        return self

    async def __aexit__(
        self,
        exc_type: typing.Optional[typing.Type[Exception]],
        exc_value: typing.Optional[Exception],
        tb: typing.Optional[TracebackType],
    ) -> None:
        ctx, chain, error = self._exit(exc_type, exc_value, tb)
        await ctx.emit(
            io=self.io,
            error=error,
            call_history=chain,
        )

    def __enter__(self) -> "TraceContext":
        self._enter()
        return self

    def __exit__(
        self,
        exc_type: typing.Optional[typing.Type[Exception]],
        exc_value: typing.Optional[Exception],
        tb: typing.Optional[TracebackType],
    ) -> None:
        ctx, chain, error = self._exit(exc_type, exc_value, tb)
        asyncio.run(
            ctx.emit(
                io=self.io,
                error=error,
                call_history=chain,
            )
        )

    def set_output(self, output: typing.Any) -> None:
        self.io.output = api_types.IOValue(
            value=output,
            type=api_types.TypeSchema(name=type(output).__name__, fields={}),
        )

    def _merge_tags(self) -> None:
        current_stack = trace_stack.get()
        if current_stack:
            parent_context_id = current_stack[-1]
            parent_context = context_storage.get().get(parent_context_id.id)
            if parent_context:
                parent_tags = parent_context.tags
                if self.tags:
                    merged_tags = parent_tags.copy()
                    merged_tags.update(self.tags)
                    self.tags = merged_tags
                else:
                    self.tags = parent_tags

    def _enter(self) -> None:
        self._merge_tags()
        ctx = ContextVarStorage(self.uid, self.tags, self.io)

        current_stack = trace_stack.get()
        stack_item = TraceStackItem(_id=self.uid, func=self.func_name)
        if current_trace_id.get() is None:
            first_trace_id.set(self.uid)
            trace_stack.set([stack_item])
        else:
            trace_stack.set(current_stack + [stack_item])

        current_trace_id.set(self.uid)
        context_storage.get()[self.uid] = ctx

    def _exit(
        self,
        exc_type: typing.Optional[typing.Type[Exception]],
        exc_value: typing.Optional[Exception],
        tb: typing.Optional[TracebackType],
    ) -> typing.Tuple[
        ContextVarStorage, typing.List[TraceStackItem], typing.Optional[api_types.Error]
    ]:
        if exc_type is not None:
            formatted_traceback = "".join(
                traceback.format_exception(exc_type, exc_value, tb)
            )
            error = api_types.Error(
                # TODO: For GlooErrors, we should have a list of error codes.
                code=1,  # Unknown error.
                message=f"{exc_type.__name__}: {exc_value}",
                traceback=formatted_traceback,
            )
        else:
            error = None
        current_stack = trace_stack.get()
        trace_stack.set(current_stack[:-1])

        context_data = context_storage.get()
        ctx = context_data.pop(self.uid, None)
        context_storage.set(context_data)

        if ctx is None:
            raise Exception("Context not found")
        return ctx, current_stack, error


def set_llm_metadata(metadata: api_types.LLMEventSchema) -> None:
    current_stack = trace_stack.get()
    if current_stack:
        current_id = current_stack[-1]
        current_context = context_storage.get().get(current_id.id)
        if current_context:
            current_context.llmMetadata = metadata
            return
    raise Exception(
        "No trace context found. Please use set_llm_metadata inside a traced function."
    )


def set_ctx_error(error: api_types.Error) -> None:
    current_stack = trace_stack.get()
    if current_stack:
        current_id = current_stack[-1]
        current_context = context_storage.get().get(current_id.id)
        if current_context:
            current_context.error = error
            return
    raise Exception(
        "No trace context found. Please use set_ctx_error inside a traced function."
    )


def get_ctx() -> typing.Tuple[TraceStackItem, ContextVarStorage]:
    current_stack = trace_stack.get()
    if current_stack:
        current_id = current_stack[-1]
        current_context = context_storage.get().get(current_id.id)
        if current_context:
            return current_id, current_context
    raise Exception(
        "No trace context found. Please use get_ctx inside a traced function."
    )


def trace(
    *,
    _name: typing.Optional[str] = None,
    _tags: typing.Optional[typing.Dict[str, str]] = None,
) -> typing.Callable[[T], T]:
    def decorator(func: T) -> T:
        name = _name or func.__name__
        tags = _tags or {}

        @functools.wraps(func)
        async def wrapper_async(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
            async with TraceContext(name, func, args, kwargs) as ctx:
                if tags:
                    update_trace_tags(**tags)
                result = await func(*args, **kwargs)
                ctx.set_output(result)
                return result

        @functools.wraps(func)
        def wrapper_sync(*args: typing.Any, **kwargs: typing.Any) -> typing.Any:
            with TraceContext(name, func, args, kwargs) as ctx:
                if tags:
                    update_trace_tags(**tags)
                result = func(*args, **kwargs)
                ctx.set_output(result)
                return result

        if asyncio.iscoroutinefunction(func):
            return wrapper_async  # type: ignore
        else:
            return wrapper_sync  # type: ignore

    return decorator


class TagContextManager:
    def __init__(self, **tags: typing.Any) -> None:
        self.tags: typing.Dict[str, typing.Any] = tags
        self.previous_tags: typing.Optional[typing.Dict[str, typing.Any]] = None

    def __enter__(self) -> None:
        current_stack = trace_stack.get()
        if current_stack:
            current_id = current_stack[-1]
            current_context = context_storage.get().get(current_id.id)
            if current_context:
                # We don't want to mutate the original tags dict
                self.previous_tags = current_context.tags.copy()
                current_context.tags = {**self.previous_tags, **self.tags}
        else:
            # Throw an error
            raise Exception(
                "No trace context found. Please use set_tags inside a traced function."
            )

    def __exit__(
        self, exc_type: typing.Any, exc_val: typing.Any, exc_tb: typing.Any
    ) -> None:
        if self.previous_tags is not None:
            current_stack = trace_stack.get()
            if current_stack:
                current_id = current_stack[-1]
                current_context = context_storage.get().get(current_id.id)
                if current_context:
                    current_context.tags = self.previous_tags


def update_trace_tags(**tags: str | None) -> None:
    """
    Update the tags for the current trace.

    Args:
        **tags: The tags to update. If a tag is None, it will be removed.

    Raises:
        Exception: If no trace context is found.
    """
    current_stack = trace_stack.get()
    if current_stack:
        current_id = current_stack[-1]
        current_context = context_storage.get().get(current_id.id)
        if current_context:
            prev_tags = current_context.tags.copy()
            for k, v in tags.items():
                if v is None:
                    prev_tags.pop(k, None)
                else:
                    prev_tags[k] = v

            current_context.tags = prev_tags
    else:
        # Throw an error
        raise Exception(
            "No trace context found. Please use set_tags inside a traced function."
        )
