from __future__ import annotations

import typing
import pydantic


class InvalidArgument(pydantic.BaseModel): ...


class Io(pydantic.BaseModel): ...


class Timeout(pydantic.BaseModel): ...


class Unsupported(pydantic.BaseModel): ...


class AccessError(pydantic.BaseModel): ...


class RenderPrompt(pydantic.BaseModel): ...


class NotImplemented(pydantic.BaseModel): ...


class LlmClient(pydantic.BaseModel): ...


class DevOther(pydantic.BaseModel): ...


class HostPanic(pydantic.BaseModel): ...


class StackFrame(pydantic.BaseModel): ...


class StackTrace(pydantic.BaseModel):
    def to_string(self) -> str: ...
    async def to_string_async(self) -> str: ...


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
