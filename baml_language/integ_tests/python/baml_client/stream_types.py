from __future__ import annotations

import typing
import typing_extensions
from pydantic import BaseModel, ConfigDict, Field

import baml_py

from . import types

StreamStateValueT = typing.TypeVar('StreamStateValueT')
class StreamState(BaseModel, typing.Generic[StreamStateValueT]):
    value: StreamStateValueT
    state: typing_extensions.Literal["Pending", "Incomplete", "Complete"]


class ContactInfo(BaseModel):
    name: typing.Union[str, None]
    email: typing.Union[str, None]
    phone: typing.Union[str, None]


class Inner001(BaseModel):
    value: typing.Union[str, None]


class Sentiment(BaseModel):
    label: typing.Union[typing.Literal["positive"], typing.Literal["negative"], typing.Literal["neutral"]]
    confidence: typing.Union[float, None]


class StreamDoneNested(BaseModel):
    done_class: types.Inner001
    done_list: typing.List[types.Inner001]
    done_map: typing.Dict[str, types.Inner001]
    regular: typing.Union[Inner001, None]


class Test001Basic(BaseModel):
    name: typing.Union[str, None]
    skills: typing.List[str]