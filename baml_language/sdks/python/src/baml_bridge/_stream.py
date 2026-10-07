"""`BamlStream` — pure-Python wrapper for a BAML stream handle.

Holds a `BamlPyHandle` whose `HANDLE_TABLE` row is a
`CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`.
`next` / `final` (sync + async) round-trip through `BamlRuntime.call_function`
exactly like any codegen-emitted instance method — `encode_call_args`
emits `handle_value(ADT_TAGGED_HEAP_HANDLE)` for `{"self": self}`, the
engine substitutes `T` / `S`, and `decode_call_result` produces the
typed Python return.

Lives outside the PyO3 module because nothing on the call path needed
Rust: the args encoder, runtime accessor, and result decoder are all
already exposed to Python; the previous Rust impl just duplicated the
plumbing in `bridge_python/src/stream.rs`. The handle-table entry, the
`BexExternalAdt::TaggedHeapHandle` variant, and the `ADT_TAGGED_HEAP_HANDLE`
proto tag stay in Rust — those are engine-side and not reachable from
Python.
"""

from __future__ import annotations

import asyncio
from typing import Any, Generic, TypeVar, cast
from contextlib import contextmanager

from .baml_py import BamlPyHandle

TNext = TypeVar("TNext")
TYield = TypeVar("TYield")
TFinal = TypeVar("TFinal")

# Terminal marker FQN for async iteration; resolved lazily through the
# installed typemap so the bridge never imports the generated package.
_DONE_FQN = "ai.stream.Done"


class BamlStream(Generic[TNext, TYield, TFinal]):
    """Opaque wrapper around a streaming-call handle.

    The type arguments are erased at runtime. `TNext` is the complete return
    type of `next` (including null and the generated `ai.stream.Done` terminal
    marker), `TYield` is the non-null partial produced by async iteration, and
    `TFinal` is the settled return type of `final`.
    Codegen emits those concrete annotations in generated leaves; they
    evaluate to a parameterized alias whose `isinstance` falls back to the
    unparameterized origin, which is what `proto.py` checks against.

    BAML's `Stream<T>` supplies all three host views from its one type: raw
    next (`T` or the terminal marker), filtered iteration (non-null `T`), and
    final (`T`).
    """

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle
        self._execution = None
        self._context = None
        self._settled = False
        self._final_value = None
        self._final_error = None

    @classmethod
    def _from_pyhandle(cls, pyhandle: BamlPyHandle) -> "BamlStream":
        """Internal: build a `BamlStream` from a `BamlPyHandle`. Used by
        `proto.py::_decode_handle`, which has already dispatched on the
        trusted stream handle tag."""
        return cls(pyhandle)

    def _to_pyhandle(self) -> BamlPyHandle:
        """Internal: expose the inner `BamlPyHandle` for inbound encode."""
        return self._handle

    def __aiter__(self) -> "BamlStream[TNext, TYield, TFinal]":
        return self

    async def __anext__(self) -> TYield:
        """Async-iteration sugar over the sentinel protocol: yields each
        non-null partial, translating the `ai.stream.Done` terminal marker
        into `StopAsyncIteration`. `final()` / `final_async()` remain the
        way to obtain the settled value after the loop."""
        from .typemap import get_type_map

        done_cls = get_type_map().get_class(_DONE_FQN)
        while True:
            item = await self.next_async()
            if isinstance(item, done_cls):
                raise StopAsyncIteration
            if item is not None:
                return cast(TYield, item)

    @contextmanager
    def _scope(self):
        from ._execution_context import _current_execution_context

        token = (
            _current_execution_context.set(self._context)
            if self._context is not None
            else None
        )
        try:
            yield
        finally:
            if token is not None:
                _current_execution_context.reset(token)

    def _finish(self, outcome, value=None):
        execution, self._execution = self._execution, None
        if execution is not None:
            from .errors import BamlError, BamlPanic

            if isinstance(value, (BamlError, BamlPanic)):
                value = value.value
            execution.finish(outcome, value)

    def __del__(self):
        # An abandoned stream is not a successful invocation. No recording
        # producer is kept bound while the Python handle is idle.
        try:
            self._finish("cancelled")
        except Exception:
            pass

    def _failed(self, error):
        self._finish(
            "cancelled" if isinstance(error, asyncio.CancelledError) else "error", error
        )

    def _done(self, value):
        from .typemap import get_type_map

        return isinstance(value, get_type_map().get_class(_DONE_FQN))

    def next(self, *, _baml: Any = None) -> TNext:
        with self._scope():
            try:
                result = self._call_sync("ai.stream.Stream.next", _baml=_baml)
                if self._execution is not None and self._done(result):
                    # EOF is not success until final parsing succeeds. Preserve
                    # the final result/error for final(), without changing the
                    # sentinel protocol or raising a final parse error at EOF.
                    try:
                        self.final(_baml=_baml)
                    except Exception:
                        if self._final_error is None:
                            raise
                return result
            except BaseException as error:
                self._failed(error)
                raise

    async def next_async(self, *, _baml: Any = None) -> TNext:
        with self._scope():
            try:
                result = await self._call_async("ai.stream.Stream.next", _baml=_baml)
                if self._execution is not None and self._done(result):
                    try:
                        await self.final_async(_baml=_baml)
                    except Exception:
                        if self._final_error is None:
                            raise
                return result
            except BaseException as error:
                self._failed(error)
                raise

    def _cached_final(self):
        if self._final_error is not None:
            raise self._final_error
        return self._final_value

    def final(self, *, _baml: Any = None) -> TFinal:
        if self._settled:
            return self._cached_final()
        with self._scope():
            try:
                result = self._call_sync("ai.stream.Stream.final", _baml=_baml)
                self._final_value, self._settled = result, True
                self._finish("ok", result)
                return result
            except BaseException as error:
                self._final_error, self._settled = error, True
                self._failed(error)
                raise

    async def final_async(self, *, _baml: Any = None) -> TFinal:
        if self._settled:
            return self._cached_final()
        with self._scope():
            try:
                result = await self._call_async("ai.stream.Stream.final", _baml=_baml)
                self._final_value, self._settled = result, True
                self._finish("ok", result)
                return result
            except BaseException as error:
                self._final_error, self._settled = error, True
                self._failed(error)
                raise

    # `proto.py` imports `BamlStream` at module load, so the call-path
    # imports (`get_runtime`, `encode_call_args`, `decode_call_result`)
    # have to be method-local to avoid a circular import.
    def _call_sync(self, fqn: str, *, _baml: Any = None) -> Any:
        from . import get_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        rt = get_runtime()
        args_proto = encode_call_args(
            {"self": self},
            new_function_call(),
            function_name=fqn,
            _baml=_baml,
            _stream_step=self._context is not None,
        )
        result_bytes = rt.call_function_sync(args_proto)
        return decode_call_result(result_bytes)

    async def _call_async(self, fqn: str, *, _baml: Any = None) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        rt = get_runtime()
        call_id = new_function_call()
        args_proto = encode_call_args(
            {"self": self},
            call_id,
            function_name=fqn,
            _baml=_baml,
            _stream_step=self._context is not None,
        )
        try:
            result_bytes = await rt.call_function(args_proto)
        except asyncio.CancelledError:
            try:
                cancel_function_call(call_id)
            except Exception:
                pass
            raise
        return _decode_call_result_async(result_bytes)

    @classmethod
    def __get_pydantic_core_schema__(cls, _source_type: Any, _handler: Any) -> Any:
        """Pydantic v2 hook so user models can declare `BamlStream`-typed
        fields without `arbitrary_types_allowed=True`."""
        from pydantic_core import core_schema  # type: ignore[import-untyped]

        return core_schema.is_instance_schema(cls)


def _decode_stream_result(result, *, asynchronous=False):
    from . import _decode_call_result_async
    from .proto import decode_call_result
    from ._execution_context import _ExecutionContext

    result_bytes, frame = result
    execution = context = None
    if frame is not None:
        execution, state, cancel = frame
        context = _ExecutionContext((state, cancel))
    try:
        stream = (_decode_call_result_async if asynchronous else decode_call_result)(
            result_bytes
        )
    except BaseException as error:
        if execution is not None:
            execution.finish(
                "cancelled" if isinstance(error, asyncio.CancelledError) else "error",
                getattr(error, "value", error),
            )
        raise
    if not isinstance(stream, BamlStream):
        if execution is not None:
            execution.finish("error")
        raise TypeError("stream companion did not return a BamlStream")
    stream._execution, stream._context = execution, context
    return stream
