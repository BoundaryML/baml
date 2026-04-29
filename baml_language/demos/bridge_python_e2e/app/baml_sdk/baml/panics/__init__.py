from __future__ import annotations

import typing
import pydantic


class DivisionByZero(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    dividend: int


class IndexOutOfBounds(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    index: int
    length: int


class MapKeyNotFound(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    key: str


class StackOverflow(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class AssertionFailed(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class Unreachable(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class UserPanic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class AllocFailure(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


Panic: typing.TypeAlias = typing.Union[DivisionByZero, IndexOutOfBounds, MapKeyNotFound, StackOverflow, AssertionFailed, Unreachable, UserPanic, AllocFailure]


__all__ = [
    "DivisionByZero",
    "IndexOutOfBounds",
    "MapKeyNotFound",
    "StackOverflow",
    "AssertionFailed",
    "Unreachable",
    "UserPanic",
    "AllocFailure",
    "Panic",
]
