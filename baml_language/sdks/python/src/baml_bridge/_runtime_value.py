"""Host proxies for engine-held nominal BAML values."""

from __future__ import annotations

import asyncio
from typing import Any, Tuple

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

    def to_data(self, *, _baml: Any = None) -> Any:
        from . import get_runtime
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args

        encoded = encode_call_args(
            {"value": self},
            new_function_call(),
            function_name="baml.json.from",
            _baml=_baml,
        )
        return decode_call_result(get_runtime().call_function_sync(encoded, encoded.kwargs, encoded.cancel))

    async def to_data_async(self, *, _baml: Any = None) -> Any:
        from . import _decode_call_result_async, cancel_function_call, get_runtime
        from .baml_py import new_function_call
        from .proto import encode_call_args

        call_id = new_function_call()
        encoded = encode_call_args(
            {"value": self},
            call_id,
            function_name="baml.json.from",
            _baml=_baml,
        )
        try:
            result = await get_runtime().call_function(encoded, encoded.kwargs, encoded.cancel)
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


class BamlOpaque:
    """An instance of a standard-library class the runtime keeps to itself,
    such as a compiled `reflect.Package`: a reference to the engine's
    instance, with no fields to read and no public constructor.

    Pass it back to BAML as you received it. Two references are equal when
    they hold the same engine handle.

    An instance of a generic class also holds the class's type arguments, in
    the class's parameter order: what the engine says the instance is, which
    an instance method binds the class's parameters from.
    """

    __slots__ = ("_handle", "_key", "_type_args")

    def __init__(self, *_args: Any, **_kwargs: Any) -> None:
        raise TypeError(
            f"{type(self).__qualname__} is the runtime's own; "
            "pass the instance you received"
        )

    @classmethod
    def _from_wire(
        cls, handle: BamlPyHandle, key: int, type_args: Tuple[Any, ...] = ()
    ) -> Any:
        """Wrap `handle`, the engine reference keyed `key` on the wire, an
        instance at the class type arguments `type_args` (`BamlType`s)."""
        instance = object.__new__(cls)
        instance._handle = handle
        instance._key = key
        instance._type_args = type_args
        return instance

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, BamlOpaque):
            return NotImplemented
        return self._key == other._key

    def __hash__(self) -> int:
        return hash(self._key)

    @classmethod
    def __get_pydantic_core_schema__(cls, _source_type: Any, _handler: Any) -> Any:
        from pydantic_core import core_schema  # type: ignore[import-untyped]

        return core_schema.is_instance_schema(cls)

    def __repr__(self) -> str:
        return f"<{type(self).__qualname__}>"
