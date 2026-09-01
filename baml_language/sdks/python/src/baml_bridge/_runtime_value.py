"""Host proxy for a runtime-created nominal BAML value."""

from __future__ import annotations

import asyncio
from typing import Any

from .baml_py import BamlPyHandle


class BamlRuntimeValue:
    """Identity-preserving live value with explicit data projection."""

    __slots__ = ("_handle",)

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle

    @classmethod
    def _from_pyhandle(cls, handle: BamlPyHandle) -> "BamlRuntimeValue":
        return cls(handle)

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def to_data(self) -> Any:
        from . import get_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        encoded = encode_call_args(
            {"value": self},
            new_function_call(),
            function_name="baml.json.from",
        )
        return decode_call_result(get_runtime().call_function_sync(encoded, None, None))

    async def to_data_async(self) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        call_id = new_function_call()
        encoded = encode_call_args(
            {"value": self},
            call_id,
            function_name="baml.json.from",
        )
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
        return "<BamlRuntimeValue>"
