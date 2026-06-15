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


class Company(BaseModel):
    name: str
    industry: str

class Container(BaseModel):
    items: typing.List[typing.Union[str, int, bool]]
    matrix: typing.List[typing.List[typing.Union[str, int]]]
    optional_union: typing.Optional[typing.Union[str, int]]

class User(BaseModel):
    name: str