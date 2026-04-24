from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml

import typing
import pydantic


class MyClass(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int


class CoverAll(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    unknown_field: typing.Any
    callable_field: typing.Callable[[int], str]
    alias_field: RecursiveAlias
    literal_field: typing.Literal["Hello"]
    optional_nested: typing.List[typing.Optional[MyClass]]
    union_field: typing.Union[int, str]
    self_ref: typing.Optional[CoverAll]


RecursiveAlias: typing.TypeAlias = 'typing.Union[int, typing.List[RecursiveAlias]]'


__all__ = [
    "MyClass",
    "CoverAll",
    "RecursiveAlias",
]
