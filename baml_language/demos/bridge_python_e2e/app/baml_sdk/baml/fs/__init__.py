from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_instance_method as _define_instance_method


class File(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _handle: None

    text       = _define_instance_method("baml.fs.File.text", "sync",  ["self"])
    text_async = _define_instance_method("baml.fs.File.text", "async", ["self"])

    bytes       = _define_instance_method("baml.fs.File.bytes", "sync",  ["self"])
    bytes_async = _define_instance_method("baml.fs.File.bytes", "async", ["self"])

    read       = _define_instance_method("baml.fs.File.read", "sync",  ["self", "n"])
    read_async = _define_instance_method("baml.fs.File.read", "async", ["self", "n"])

    read_bytes       = _define_instance_method("baml.fs.File.read_bytes", "sync",  ["self", "n"])
    read_bytes_async = _define_instance_method("baml.fs.File.read_bytes", "async", ["self", "n"])

    close       = _define_instance_method("baml.fs.File.close", "sync",  ["self"])
    close_async = _define_instance_method("baml.fs.File.close", "async", ["self"])

    seek_from       = _define_instance_method("baml.fs.File.seek_from", "sync",  ["self", "whence", "offset"])
    seek_from_async = _define_instance_method("baml.fs.File.seek_from", "async", ["self", "whence", "offset"])

    write       = _define_instance_method("baml.fs.File.write", "sync",  ["self", "data"])
    write_async = _define_instance_method("baml.fs.File.write", "async", ["self", "data"])

    write_bytes       = _define_instance_method("baml.fs.File.write_bytes", "sync",  ["self", "data"])
    write_bytes_async = _define_instance_method("baml.fs.File.write_bytes", "async", ["self", "data"])


class DirEntry(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    is_dir: bool
    is_file: bool
    is_symlink: bool


class MkdirOptions(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    recursive: bool


__all__ = [
    "File",
    "DirEntry",
    "MkdirOptions",
]
