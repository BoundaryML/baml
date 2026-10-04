"""Host proxy for a live ``ai.FunctionSpec<Out>`` value."""

from __future__ import annotations

import asyncio
from typing import Any, Generic, TypeVar

from .baml_py import BamlPyHandle

TOut = TypeVar("TOut")


class BamlFunctionSpec(Generic[TOut]):
    """Opaque bound LLM recipe owned by the engine that created it."""

    __slots__ = ("_handle",)

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle

    @classmethod
    def _from_pyhandle(cls, handle: BamlPyHandle) -> "BamlFunctionSpec[Any]":
        return cls(handle)

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def name(self, *, _baml: Any = None) -> str:
        return self._call_sync("ai.FunctionSpec.name", _baml=_baml)

    async def name_async(self, *, _baml: Any = None) -> str:
        return await self._call_async("ai.FunctionSpec.name", _baml=_baml)

    def arguments(self, *, _baml: Any = None) -> dict[str, Any]:
        return self._call_sync("ai.FunctionSpec.arguments", _baml=_baml)

    async def arguments_async(self, *, _baml: Any = None) -> dict[str, Any]:
        return await self._call_async("ai.FunctionSpec.arguments", _baml=_baml)

    def output_type(self, *, _baml: Any = None) -> Any:
        return self._call_sync("ai.FunctionSpec.output_type", _baml=_baml)

    async def output_type_async(self, *, _baml: Any = None) -> Any:
        return await self._call_async("ai.FunctionSpec.output_type", _baml=_baml)

    def prompt(self, *, _baml: Any = None) -> Any:
        return self._call_sync("ai.FunctionSpec.prompt", _baml=_baml)

    async def prompt_async(self, *, _baml: Any = None) -> Any:
        return await self._call_async("ai.FunctionSpec.prompt", _baml=_baml)

    def tools(self, *, _baml: Any = None) -> Any:
        return self._call_sync("ai.FunctionSpec.tools", _baml=_baml)

    async def tools_async(self, *, _baml: Any = None) -> Any:
        return await self._call_async("ai.FunctionSpec.tools", _baml=_baml)

    def client_id(self, *, _baml: Any = None) -> str:
        return self._call_sync("ai.FunctionSpec.client_id", _baml=_baml)

    async def client_id_async(self, *, _baml: Any = None) -> str:
        return await self._call_async("ai.FunctionSpec.client_id", _baml=_baml)

    def build_request(self, *, _baml: Any = None, **kwargs: Any) -> Any:
        return self._call_sync("ai.FunctionSpec.build_request", kwargs, _baml=_baml)

    async def build_request_async(self, *, _baml: Any = None, **kwargs: Any) -> Any:
        return await self._call_async("ai.FunctionSpec.build_request", kwargs, _baml=_baml)

    def parse(self, json: str, *, _baml: Any = None) -> TOut:
        return self._call_sync("ai.FunctionSpec.parse", {"json": json}, _baml=_baml)

    async def parse_async(self, json: str, *, _baml: Any = None) -> TOut:
        return await self._call_async("ai.FunctionSpec.parse", {"json": json}, _baml=_baml)

    def call(self, *, _baml: Any = None, **kwargs: Any) -> TOut:
        return self._call_sync("ai.FunctionSpec.call", kwargs, _baml=_baml)

    async def call_async(self, *, _baml: Any = None, **kwargs: Any) -> TOut:
        return await self._call_async("ai.FunctionSpec.call", kwargs, _baml=_baml)

    def _call_sync(self, fqn: str, kwargs: dict[str, Any] | None = None, *, _baml: Any = None) -> Any:
        from . import get_or_init_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        values = {"self": self}
        values.update(kwargs or {})
        encoded = encode_call_args(
            values,
            new_function_call(),
            function_name=fqn,
            _baml=_baml,
        )
        return decode_call_result(get_or_init_runtime().call_function_sync(encoded))

    async def _call_async(self, fqn: str, kwargs: dict[str, Any] | None = None, *, _baml: Any = None) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_or_init_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        values = {"self": self}
        values.update(kwargs or {})
        call_id = new_function_call()
        encoded = encode_call_args(values, call_id, function_name=fqn, _baml=_baml)
        try:
            result = await get_or_init_runtime().call_function(encoded)
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
