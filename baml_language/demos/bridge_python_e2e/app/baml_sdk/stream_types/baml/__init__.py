from __future__ import annotations

from . import errors, fs, glob, http, llm, media, net, panics, stream

import typing
import pydantic


class Array(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


class Map(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


class String(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


class Uint8Array(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


__all__ = [
    "Array",
    "Map",
    "String",
    "Uint8Array",
]
