"""Retained concrete receivers; generated facades supply their public methods."""

from __future__ import annotations

from typing import Any, Awaitable

from .cffi.v1.baml_type_pb2 import BamlTy

from ._reference import BamlRef


class BamlConcreteRef(BamlRef):
    __slots__ = ()

    def _invoke_concrete(
        self,
        class_name: str,
        interface_pattern: BamlTy | None,
        member: str,
        arguments: dict[str, Any],
        type_arguments: tuple[Any, ...] = (),
    ) -> Awaitable[Any]:
        """Invoke a compiler-selected concrete contract on the issuing engine."""
        from .baml_py import new_function_call
        from .cffi.v1.baml_inbound_pb2 import ConcreteMethodTarget
        from .proto import decode_call_result, encode_call_args
        from .typemap import _using_type_map

        handle = self._to_pyhandle()
        target = ConcreteMethodTarget(
            receiver=handle._key_for_concrete_call(),
            class_name=class_name,
            member=member,
        )
        if interface_pattern is None:
            target.inherent = True
        else:
            target.interface_pattern.CopyFrom(interface_pattern)
        type_map = self._type_map
        with _using_type_map(type_map):
            encoded = encode_call_args(
                arguments,
                new_function_call(),
                [("", ty) for ty in type_arguments],
                concrete_method=target,
            )
        # Preparation pins the receiver and argument evidence before yielding.
        pending = handle._call_owned_function(encoded)

        async def receive() -> Any:
            return decode_call_result(await pending, type_map=type_map)

        return receive()

    def _data_call(self) -> bytes:
        from .baml_py import new_function_call
        from .proto import encode_call_args

        return encode_call_args(
            {"value": self}, new_function_call(), function_name="baml.json.from"
        )

    def to_data(self) -> Any:
        """Explicit data conversion runs on this receiver's issuing runtime."""
        from .proto import decode_call_result

        handle = self._to_pyhandle()
        result = handle._call_owned_function_sync(self._data_call())
        return decode_call_result(result, type_map=self._type_map)

    async def to_data_async(self) -> Any:
        from .proto import decode_call_result

        handle = self._to_pyhandle()
        pending = handle._call_owned_function(self._data_call())
        return decode_call_result(await pending, type_map=self._type_map)
