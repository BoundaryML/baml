"""Small host-only additions to the generated ``baml.reflect`` namespace."""

from typing import Any

from .proto import BamlType


class _TypeNamespace:
    __slots__ = ()

    def of(self, token: Any) -> BamlType:
        """Create an opaque BAML type handle from an accepted Python token."""
        return BamlType._from_python(token)


type_ = _TypeNamespace()

__all__ = ["type_"]
