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


class Literals(BaseModel):
    priority_1: typing.Literal["1"]
    priority_2: typing.Literal["2"]
    priority_3: typing.Literal["3"]
    status_draft: typing.Literal["draft"]
    status_published: typing.Literal["published"]
    count: typing.Literal[42]
    enabled: typing.Literal[True]
    disabled: typing.Literal[False]