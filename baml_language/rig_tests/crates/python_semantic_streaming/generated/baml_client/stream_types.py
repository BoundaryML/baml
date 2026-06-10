import typing
import typing_extensions
from pydantic import BaseModel, ConfigDict, Field

import baml_py

from . import types

StreamStateValueT = typing.TypeVar('StreamStateValueT')
class StreamState(BaseModel, typing.Generic[StreamStateValueT]):
    value: StreamStateValueT
    state: typing_extensions.Literal["Pending", "Incomplete", "Complete"]


class ClassWithoutDone(BaseModel):
    i_16_digits: int
    s_20_words: StreamState[str]


class SemanticContainer(BaseModel):
    sixteen_digit_number: int
    string_with_twenty_words: StreamState[str]
    class_1: ClassWithoutDone
    class_2: types.ClassWithBlockDone
    class_done_needed: StreamState[types.ClassWithBlockDone]
    class_needed: StreamState[ClassWithoutDone]
    three_small_things: typing.List[SmallThing]
    final_string: str


class SmallThing(BaseModel):
    i_16_digits: StreamState[int]
    i_8_digits: int