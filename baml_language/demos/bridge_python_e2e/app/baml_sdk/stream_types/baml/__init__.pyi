from __future__ import annotations

from . import errors, fs, http, llm, media, net, panics, stream

import pydantic


class Array(pydantic.BaseModel): ...


class Map(pydantic.BaseModel): ...


class String(pydantic.BaseModel): ...


class Uint8Array(pydantic.BaseModel): ...


__all__ = [
    "Array",
    "Map",
    "String",
    "Uint8Array",
]
