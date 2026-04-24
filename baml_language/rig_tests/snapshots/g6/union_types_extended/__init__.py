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


union_simple       = __define_function("root.union_simple", "sync",  ["u"])
union_simple_async = __define_function("root.union_simple", "async", ["u"])


union_complex       = __define_function("root.union_complex", "sync",  ["u"])
union_complex_async = __define_function("root.union_complex", "async", ["u"])


union_in_list       = __define_function("root.union_in_list", "sync",  ["items"])
union_in_list_async = __define_function("root.union_in_list", "async", ["items"])


union_return       = __define_function("root.union_return", "sync",  [])
union_return_async = __define_function("root.union_return", "async", [])


class User(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str


class Company(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    industry: str


class Container(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    items: typing.List[typing.Union[str, int, bool]]
    matrix: typing.List[typing.List[typing.Union[str, int]]]
    optional_union: typing.Optional[typing.Union[str, int]]


__all__ = [
    "union_simple",
    "union_simple_async",
    "union_complex",
    "union_complex_async",
    "union_in_list",
    "union_in_list_async",
    "union_return",
    "union_return_async",
    "User",
    "Company",
    "Container",
]
