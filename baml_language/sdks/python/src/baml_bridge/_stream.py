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

from .typemap import get_type_map, runtime_bound
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

    BAML's `Stream<Partial, Final>` supplies all three host views: raw next,
    filtered iteration, and final.
    """

    def __init__(self, handle: BamlPyHandle) -> None:
        self._type_map = get_type_map()
        self._handle = handle

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

    @runtime_bound
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

    def next(self) -> TNext:
        return self._call_sync("ai.stream.Stream.next")

    async def next_async(self) -> TNext:
        return await self._call_async("ai.stream.Stream.next")

    def final(self) -> TFinal:
        return self._call_sync("ai.stream.Stream.final")

    async def final_async(self) -> TFinal:
        return await self._call_async("ai.stream.Stream.final")

    # `proto.py` imports `BamlStream` at module load, so the call-path
    # imports (`get_runtime`, `encode_call_args`, `decode_call_result`)
    # have to be method-local to avoid a circular import.
    @runtime_bound
    def _call_sync(self, fqn: str) -> Any:
        from . import get_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        rt = get_runtime()
        args_proto = encode_call_args(
            {"self": self},
            new_function_call(),
            function_name=fqn,
        )
        result_bytes = rt.call_function_sync(args_proto, None, None)
        return decode_call_result(result_bytes)

    @runtime_bound
    async def _call_async(self, fqn: str) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        rt = get_runtime()
        call_id = new_function_call()
        args_proto = encode_call_args(
            {"self": self},
            call_id,
            function_name=fqn,
        )
        try:
            result_bytes = await rt.call_function(args_proto, None, None)
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
