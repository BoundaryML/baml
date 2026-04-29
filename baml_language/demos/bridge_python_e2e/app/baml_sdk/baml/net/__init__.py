from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_instance_method as _define_instance_method


class Socket(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _handle: None

    read       = _define_instance_method("baml.net.Socket.read", "sync",  ["self"])
    read_async = _define_instance_method("baml.net.Socket.read", "async", ["self"])

    close       = _define_instance_method("baml.net.Socket.close", "sync",  ["self"])
    close_async = _define_instance_method("baml.net.Socket.close", "async", ["self"])


__all__ = [
    "Socket",
]
