"""`BamlStream` — pure-Python wrapper for `baml.llm.Stream`.

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

from typing import Any, Generic, TypeVar

from .baml_py import BamlPyHandle

_STREAM_NEXT_FN = "baml.llm.Stream.next"
_STREAM_FINAL_FN = "baml.llm.Stream.final"

TStream = TypeVar("TStream")
TFinal = TypeVar("TFinal")


class BamlStream(Generic[TStream, TFinal]):
    """Opaque wrapper around a streaming-call handle.

    `TStream` / `TFinal` are erased at runtime — `BamlStream[TStream, TFinal]`
    is just a `typing.Generic` subscription, handled natively by Python.
    Codegen emits `Stream[X, Y]` annotations in generated leaves; they
    evaluate to a parameterized alias whose `isinstance` falls back to the
    unparameterized origin, which is what `proto.py` checks against.

    The positional order mirrors the BAML signature
    `Stream<TStream, TFinal>` (stream type first, final type second).
    """

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle

    @classmethod
    def _from_pyhandle(cls, pyhandle: BamlPyHandle) -> "BamlStream":
        """Internal: build a `BamlStream` from a `BamlPyHandle`. Used by
        `proto.py::_decode_handle`, which has already dispatched on the
        wire `handle_type` tag — no further validation needed here."""
        return cls(pyhandle)

    def _to_pyhandle(self) -> BamlPyHandle:
        """Internal: expose the inner `BamlPyHandle` for inbound encode."""
        return self._handle

    def next(self) -> Any:
        return self._call_sync(_STREAM_NEXT_FN)

    async def next_async(self) -> Any:
        return await self._call_async(_STREAM_NEXT_FN)

    def final(self) -> Any:
        return self._call_sync(_STREAM_FINAL_FN)

    async def final_async(self) -> Any:
        return await self._call_async(_STREAM_FINAL_FN)

    # `proto.py` imports `BamlStream` at module load, so the call-path
    # imports (`get_runtime`, `encode_call_args`, `decode_call_result`)
    # have to be method-local to avoid a circular import.
    def _call_sync(self, fqn: str) -> Any:
        from . import get_runtime
        from .proto import decode_call_result, encode_call_args

        rt = get_runtime()
        args_proto = encode_call_args({"self": self})
        result_bytes = rt.call_function_sync(fqn, args_proto, None, None, None)
        return decode_call_result(result_bytes)

    async def _call_async(self, fqn: str) -> Any:
        from . import get_runtime
        from .proto import decode_call_result, encode_call_args

        rt = get_runtime()
        args_proto = encode_call_args({"self": self})
        result_bytes = await rt.call_function(fqn, args_proto, None, None, None)
        return decode_call_result(result_bytes)

    @classmethod
    def __get_pydantic_core_schema__(cls, _source_type: Any, _handler: Any) -> Any:
        """Pydantic v2 hook so user models can declare `BamlStream`-typed
        fields without `arbitrary_types_allowed=True`."""
        from pydantic_core import core_schema  # type: ignore[import-untyped]

        return core_schema.is_instance_schema(cls)
