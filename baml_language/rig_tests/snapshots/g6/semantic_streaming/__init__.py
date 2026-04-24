from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml, stream_types

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class SemanticContainer(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    sixteen_digit_number: int
    string_with_twenty_words: typing.Optional[str]
    class_1: ClassWithoutDone
    class_2: ClassWithBlockDone
    class_done_needed: typing.Optional[ClassWithBlockDone]
    class_needed: typing.Optional[ClassWithoutDone]
    three_small_things: typing.List[SmallThing]
    final_string: str


class ClassWithoutDone(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    i_16_digits: int
    s_20_words: typing.Optional[str]


class ClassWithBlockDone(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    i_16_digits: int
    s_20_words: str


class SmallThing(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    i_16_digits: typing.Optional[int]
    i_8_digits: int


MakeSemanticContainer       = __define_function("root.MakeSemanticContainer", "sync",  [])
MakeSemanticContainer_async = __define_function("root.MakeSemanticContainer", "async", [])


MakeClassWithBlockDone       = __define_function("root.MakeClassWithBlockDone", "sync",  [])
MakeClassWithBlockDone_async = __define_function("root.MakeClassWithBlockDone", "async", [])


MakeClassWithExternalDone       = __define_function("root.MakeClassWithExternalDone", "sync",  [])
MakeClassWithExternalDone_async = __define_function("root.MakeClassWithExternalDone", "async", [])


__all__ = [
    "SemanticContainer",
    "ClassWithoutDone",
    "ClassWithBlockDone",
    "SmallThing",
    "MakeSemanticContainer",
    "MakeSemanticContainer_async",
    "MakeClassWithBlockDone",
    "MakeClassWithBlockDone_async",
    "MakeClassWithExternalDone",
    "MakeClassWithExternalDone_async",
]
