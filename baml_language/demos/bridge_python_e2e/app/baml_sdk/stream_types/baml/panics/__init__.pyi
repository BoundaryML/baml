from __future__ import annotations

import typing
import pydantic


class AllocFailure(pydantic.BaseModel): ...


class AssertionFailed(pydantic.BaseModel): ...


class DivisionByZero(pydantic.BaseModel): ...


class IndexOutOfBounds(pydantic.BaseModel): ...


class MapKeyNotFound(pydantic.BaseModel): ...


class StackOverflow(pydantic.BaseModel): ...


class Unreachable(pydantic.BaseModel): ...


class UserPanic(pydantic.BaseModel): ...


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
