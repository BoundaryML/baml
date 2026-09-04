"""Host proxy for a live ``ai.FunctionSpec<Out>`` value."""

from __future__ import annotations

import asyncio
from typing import Any, Generic, TypeVar

from .typemap import get_type_map, runtime_bound
from .baml_py import BamlPyHandle

TOut = TypeVar("TOut")


class BamlFunctionSpec(Generic[TOut]):
    """Opaque bound LLM recipe owned by the engine that created it."""

    __slots__ = ("_type_map", "_handle",)

    def __init__(self, handle: BamlPyHandle) -> None:
        self._type_map = get_type_map()
        self._handle = handle

    @classmethod
    def _from_pyhandle(cls, handle: BamlPyHandle) -> "BamlFunctionSpec[Any]":
        return cls(handle)

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def name(self) -> str:
        return self._call_sync("ai.FunctionSpec.name")

    async def name_async(self) -> str:
        return await self._call_async("ai.FunctionSpec.name")

    def arguments(self) -> dict[str, Any]:
        return self._call_sync("ai.FunctionSpec.arguments")

    async def arguments_async(self) -> dict[str, Any]:
        return await self._call_async("ai.FunctionSpec.arguments")

    def output_type(self) -> Any:
        return self._call_sync("ai.FunctionSpec.output_type")

    async def output_type_async(self) -> Any:
        return await self._call_async("ai.FunctionSpec.output_type")

    def prompt(self) -> Any:
        return self._call_sync("ai.FunctionSpec.prompt")

    async def prompt_async(self) -> Any:
        return await self._call_async("ai.FunctionSpec.prompt")

    def tools(self) -> Any:
        return self._call_sync("ai.FunctionSpec.tools")

    async def tools_async(self) -> Any:
        return await self._call_async("ai.FunctionSpec.tools")

    def client_id(self) -> str:
        return self._call_sync("ai.FunctionSpec.client_id")

    async def client_id_async(self) -> str:
        return await self._call_async("ai.FunctionSpec.client_id")

    def build_request(self, **kwargs: Any) -> Any:
        return self._call_sync("ai.FunctionSpec.build_request", kwargs)

    async def build_request_async(self, **kwargs: Any) -> Any:
        return await self._call_async("ai.FunctionSpec.build_request", kwargs)

    def parse(self, json: str) -> TOut:
        return self._call_sync("ai.FunctionSpec.parse", {"json": json})

    async def parse_async(self, json: str) -> TOut:
        return await self._call_async("ai.FunctionSpec.parse", {"json": json})

    def call(self, **kwargs: Any) -> TOut:
        return self._call_sync("ai.FunctionSpec.call", kwargs)

    async def call_async(self, **kwargs: Any) -> TOut:
        return await self._call_async("ai.FunctionSpec.call", kwargs)

    @runtime_bound
    def _call_sync(self, fqn: str, kwargs: dict[str, Any] | None = None) -> Any:
        from . import get_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        values = {"self": self}
        values.update(kwargs or {})
        encoded = encode_call_args(
            values,
            new_function_call(),
            function_name=fqn,
        )
        return decode_call_result(get_runtime().call_function_sync(encoded, None, None))

    @runtime_bound
    async def _call_async(self, fqn: str, kwargs: dict[str, Any] | None = None) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        values = {"self": self}
        values.update(kwargs or {})
        call_id = new_function_call()
        encoded = encode_call_args(values, call_id, function_name=fqn)
        try:
            result = await get_runtime().call_function(encoded, None, None)
        except asyncio.CancelledError:
            try:
                cancel_function_call(call_id)
            except Exception:
                pass
            raise
        return _decode_call_result_async(result)

    @classmethod
    def __get_pydantic_core_schema__(cls, _source_type: Any, _handler: Any) -> Any:
        from pydantic_core import core_schema  # type: ignore[import-untyped]

        return core_schema.is_instance_schema(cls)

    def __repr__(self) -> str:
        return "<BamlFunctionSpec>"
