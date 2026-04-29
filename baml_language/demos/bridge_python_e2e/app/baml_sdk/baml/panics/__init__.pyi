from __future__ import annotations

import typing
import pydantic


class DivisionByZero(pydantic.BaseModel): ...


class IndexOutOfBounds(pydantic.BaseModel): ...


class MapKeyNotFound(pydantic.BaseModel): ...


class StackOverflow(pydantic.BaseModel): ...


class AssertionFailed(pydantic.BaseModel): ...


class Unreachable(pydantic.BaseModel): ...


class UserPanic(pydantic.BaseModel): ...


class AllocFailure(pydantic.BaseModel): ...


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
