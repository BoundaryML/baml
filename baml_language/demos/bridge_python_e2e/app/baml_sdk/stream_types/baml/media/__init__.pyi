from __future__ import annotations

import pydantic


class Audio(pydantic.BaseModel): ...


class Image(pydantic.BaseModel): ...


class Pdf(pydantic.BaseModel): ...


class Video(pydantic.BaseModel): ...


__all__ = [
    "Audio",
    "Image",
    "Pdf",
    "Video",
]
