from __future__ import annotations

import typing
import pydantic


class Audio(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _data: None


class Image(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _data: None


class Pdf(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _data: None


class Video(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _data: None


__all__ = [
    "Audio",
    "Image",
    "Pdf",
    "Video",
]
