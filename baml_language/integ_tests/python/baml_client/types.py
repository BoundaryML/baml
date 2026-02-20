from __future__ import annotations

import typing
import typing_extensions
from enum import Enum
from pydantic import BaseModel, ConfigDict, Field

import baml_py

CheckT = typing_extensions.TypeVar('CheckT')
CheckName = typing_extensions.TypeVar('CheckName', bound=str)

class Check(BaseModel):
    name: str
    expression: str
    status: str

class Checked(BaseModel, typing.Generic[CheckT, CheckName]):
    value: CheckT
    checks: typing.Dict[CheckName, Check]

def get_checks(checks: typing.Dict[CheckName, Check]) -> typing.List[Check]:
    return list(checks.values())

def all_succeeded(checks: typing.Dict[CheckName, Check]) -> bool:
    return all(check.status == "succeeded" for check in get_checks(checks))


class ContactInfo(BaseModel):
    name: str
    email: str
    phone: typing.Union[str, None]

class Inner001(BaseModel):
    value: str

class Sentiment(BaseModel):
    label: typing.Union[typing.Literal["positive"], typing.Literal["negative"], typing.Literal["neutral"]]
    confidence: float

class StreamDoneNested(BaseModel):
    done_class: Inner001
    done_list: typing.List[Inner001]
    done_map: typing.Dict[str, Inner001]
    regular: Inner001

class Test001Basic(BaseModel):
    name: str
    skills: typing.List[str]