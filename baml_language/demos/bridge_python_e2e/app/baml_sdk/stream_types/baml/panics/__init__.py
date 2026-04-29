from __future__ import annotations

import typing
import pydantic


class AllocFailure(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class AssertionFailed(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class DivisionByZero(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    dividend: typing.Union[int, None]


class IndexOutOfBounds(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    index: typing.Union[int, None]
    length: typing.Union[int, None]


class MapKeyNotFound(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    key: typing.Union[str, None]


class StackOverflow(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class Unreachable(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class UserPanic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


Panic: typing.TypeAlias = typing.Union[DivisionByZero, IndexOutOfBounds, MapKeyNotFound, StackOverflow, AssertionFailed, Unreachable, UserPanic, AllocFailure]


__all__ = [
    "AllocFailure",
    "AssertionFailed",
    "DivisionByZero",
    "IndexOutOfBounds",
    "MapKeyNotFound",
    "StackOverflow",
    "Unreachable",
    "UserPanic",
    "Panic",
]
