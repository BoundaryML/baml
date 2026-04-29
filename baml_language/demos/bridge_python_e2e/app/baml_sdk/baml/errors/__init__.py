from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_instance_method as _define_instance_method


class InvalidArgument(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class Io(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class Timeout(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str
    duration_ms: typing.Union[int, None]


class Unsupported(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class AccessError(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class RenderPrompt(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class NotImplemented(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class LlmClient(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class DevOther(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class HostPanic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: str


class StackFrame(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    file: str
    line: int
    function_name: str


class StackTrace(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    frames: typing.List[StackFrame]

    to_string       = _define_instance_method("baml.errors.StackTrace.to_string", "sync",  ["self"])
    to_string_async = _define_instance_method("baml.errors.StackTrace.to_string", "async", ["self"])


__all__ = [
    "InvalidArgument",
    "Io",
    "Timeout",
    "Unsupported",
    "AccessError",
    "RenderPrompt",
    "NotImplemented",
    "LlmClient",
    "DevOther",
    "HostPanic",
    "StackFrame",
    "StackTrace",
]
