from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class MapContainer(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    simple: typing.Dict[str, int]
    nested: typing.Dict[str, typing.Dict[str, str]]
    array_val: typing.Dict[str, typing.List[str]]
    union_val: typing.Dict[str, typing.Union[str, int]]


map_string_int       = __define_function("root.map_string_int", "sync",  ["m"])
map_string_int_async = __define_function("root.map_string_int", "async", ["m"])


nested_map       = __define_function("root.nested_map", "sync",  ["m"])
nested_map_async = __define_function("root.nested_map", "async", ["m"])


map_of_arrays       = __define_function("root.map_of_arrays", "sync",  ["m"])
map_of_arrays_async = __define_function("root.map_of_arrays", "async", ["m"])


__all__ = [
    "MapContainer",
    "map_string_int",
    "map_string_int_async",
    "nested_map",
    "nested_map_async",
    "map_of_arrays",
    "map_of_arrays_async",
]
