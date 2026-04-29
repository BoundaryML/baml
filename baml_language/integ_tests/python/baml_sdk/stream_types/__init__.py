from __future__ import annotations

from . import test003

import typing
import pydantic


class Inner001(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: typing.Union[str, None]


class StreamDoneNested(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    done_class: Inner001
    done_list: typing.List[Inner001]
    done_map: typing.Dict[str, Inner001]
    regular: typing.Union[Inner001, None]


class Test001Basic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: typing.Union[str, None]
    skills: typing.List[str]


class ContactInfo(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: typing.Union[str, None]
    email: typing.Union[str, None]
    phone: typing.Union[str, None]


__all__ = [
    "Inner001",
    "StreamDoneNested",
    "Test001Basic",
    "ContactInfo",
]
