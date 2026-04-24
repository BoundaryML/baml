from __future__ import annotations

import typing
import pydantic


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


class SmallThing(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    i_16_digits: typing.Optional[int]
    i_8_digits: int


__all__ = [
    "SemanticContainer",
    "ClassWithoutDone",
    "SmallThing",
]
