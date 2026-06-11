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


class MapContainer(BaseModel):
    simple: typing.Dict[str, int]
    nested: typing.Dict[str, typing.Dict[str, str]]
    array_val: typing.Dict[str, typing.List[str]]
    union_val: typing.Dict[str, typing.Union[str, int]]