from __future__ import annotations

import typing
import pydantic


class DirEntry(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: typing.Union[str, None]
    is_dir: typing.Union[bool, None]
    is_file: typing.Union[bool, None]
    is_symlink: typing.Union[bool, None]


class File(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _handle: None


class MkdirOptions(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    recursive: typing.Union[bool, None]


__all__ = [
    "DirEntry",
    "File",
    "MkdirOptions",
]
