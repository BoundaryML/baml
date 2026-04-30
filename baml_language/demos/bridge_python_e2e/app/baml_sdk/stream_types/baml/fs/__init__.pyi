from __future__ import annotations

import pydantic


class DirEntry(pydantic.BaseModel): ...


class File(pydantic.BaseModel): ...


class MkdirOptions(pydantic.BaseModel): ...


__all__ = [
    "DirEntry",
    "File",
    "MkdirOptions",
]
