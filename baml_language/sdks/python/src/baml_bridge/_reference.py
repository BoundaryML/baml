"""Shared ownership for retained BAML receivers."""

from __future__ import annotations

import copy
from typing import TYPE_CHECKING, Any, Awaitable, TypeVar, get_origin

from .baml_py import BamlPyHandle
from .typemap import BamlTypeMap

if TYPE_CHECKING:
    from ._interface import BamlInterfaceRef

_InterfaceRefT = TypeVar("_InterfaceRefT", bound="BamlInterfaceRef")


class BamlRef:
    __slots__ = ("_handle", "_type_map")

    def as_interface(self, target: type[_InterfaceRefT]) -> Awaitable[_InterfaceRefT]:
        """Check a fully specified interface view without copying the receiver.

        Use a generated Ref type, including every generic/associated argument,
        for example ``value.as_interface(SourceRef[str, ReadError])``.
        """
        from ._interface import BamlInterfaceRef
        from .baml_py import new_function_call
        from .proto import decode_call_result, encode_call_args, python_type_to_wire_ty
        from .typemap import _using_type_map

        origin = get_origin(target) or target
        if not isinstance(origin, type) or not issubclass(origin, BamlInterfaceRef):
            raise TypeError("as_interface expects a generated interface Ref type")
        handle = self._to_pyhandle()
        type_map = self._type_map
        with _using_type_map(type_map):
            ty = python_type_to_wire_ty(target)
            if ty.WhichOneof("ty") != "interface":
                raise TypeError("as_interface expects a fully specified interface type")
            if type_map.get_interface_ref(ty.interface.name) is not origin:
                raise TypeError("target Ref type does not belong to this reference's SDK")
            encoded = encode_call_args(
                {"value": self},
                new_function_call(),
                [("T", ty)],
                function_name="baml.identity",
            )
        pending = handle._call_owned_function(encoded)

        async def receive() -> _InterfaceRefT:
            return decode_call_result(await pending, type_map=type_map)

        return receive()

    def _method_types(
        self, choices: dict[str, Any], expected: tuple[str, ...]
    ) -> tuple[Any, ...]:
        from ._interface import _method_type_arguments
        from .typemap import _using_type_map

        with _using_type_map(self._type_map):
            return _method_type_arguments(choices, expected)

    def __new__(cls, *args: Any, **kwargs: Any) -> BamlRef:
        raise TypeError("BAML references are returned by the SDK")

    @classmethod
    def __get_pydantic_core_schema__(cls, source: Any, handler: Any) -> Any:
        from pydantic_core import core_schema

        # A record field receives an already decoded retained receiver.
        # Pydantic never constructs an implementation from a field-shaped dict.
        # Exact declaration/pin checks remain at the checked bridge boundary.
        return core_schema.is_instance_schema(cls)

    @classmethod
    def _from_handle(cls, handle: BamlPyHandle, type_map: BamlTypeMap) -> BamlRef:
        ref = object.__new__(cls)
        ref._handle = handle
        ref._type_map = type_map
        return ref

    def _to_pyhandle(self) -> BamlPyHandle:
        if self._handle is None:
            raise RuntimeError("BAML reference is closed")
        return self._handle

    def __copy__(self) -> BamlRef:
        return type(self)._from_handle(copy.copy(self._to_pyhandle()), self._type_map)

    def __deepcopy__(self, memo: dict[int, Any]) -> BamlRef:
        # Copy a lease, never the implementing object's state.
        return self.__copy__()

    def close(self) -> None:
        """Release this SDK reference; independent copies remain usable."""
        self._handle = None
