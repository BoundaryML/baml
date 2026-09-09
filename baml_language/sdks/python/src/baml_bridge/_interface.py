"""Owned interface references used by generated SDK facades.

Method names and annotations belong to generated subclasses. The handle is
the checked capability; Python class identity and diagnostic FQNs are not.
"""

from __future__ import annotations

from typing import Any, Awaitable

from .baml_py import new_function_call
from ._reference import BamlRef


def _method_type_arguments(
    choices: dict[str, Any], expected: tuple[str, ...]
) -> tuple[Any, ...]:
    from .proto import BamlType, python_type_to_wire_ty

    if not isinstance(choices, dict) or set(choices) != set(expected):
        raise TypeError(f"method _types must specify exactly {expected!r}")
    # Definition tokens carry their own codec/type definition. Keep them whole
    # for encode_call_args instead of converting them to a diagnostic wire Ty.
    return tuple(
        choices[name]
        if isinstance(choices[name], BamlType)
        else python_type_to_wire_ty(choices[name])
        for name in expected
    )


class BamlInterfaceRef(BamlRef):
    __slots__ = ()

    def _invoke(
        self,
        member: str,
        arguments: dict[str, Any],
        type_arguments: tuple[Any, ...] = (),
    ) -> Awaitable[Any]:
        from .proto import decode_call_result, encode_call_args
        from .typemap import _using_type_map

        handle = self._to_pyhandle()
        with _using_type_map(self._type_map):
            encoded = encode_call_args(
                arguments,
                new_function_call(),
                [("", ty) for ty in type_arguments],
            )
        # Native preparation retains the view and issuing runtime before
        # returning this awaitable. Dropping/closing the wrapper cannot revoke
        # an admitted call. Decode all nested results using this SDK's typemap.
        pending = handle._call_method(member, encoded)
        type_map = self._type_map

        async def receive() -> Any:
            return decode_call_result(await pending, type_map=type_map)

        return receive()
