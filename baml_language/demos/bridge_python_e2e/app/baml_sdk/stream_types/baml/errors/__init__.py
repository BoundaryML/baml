from __future__ import annotations

import typing
import pydantic


class AccessError(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class DevOther(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class HostPanic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class InvalidArgument(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class Io(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class LlmClient(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class NotImplemented(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class RenderPrompt(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class Timeout(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]
    duration_ms: typing.Union[int, None]


class Unsupported(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    message: typing.Union[str, None]


class StackFrame(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    file: typing.Union[str, None]
    line: typing.Union[int, None]
    function_name: typing.Union[str, None]


class StackTrace(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    frames: typing.List[StackFrame]


__all__ = [
    "AccessError",
    "DevOther",
    "HostPanic",
    "InvalidArgument",
    "Io",
    "LlmClient",
    "NotImplemented",
    "RenderPrompt",
    "Timeout",
    "Unsupported",
    "StackFrame",
    "StackTrace",
]
