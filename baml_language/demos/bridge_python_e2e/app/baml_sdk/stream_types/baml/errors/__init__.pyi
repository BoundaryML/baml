from __future__ import annotations

import pydantic


class AccessError(pydantic.BaseModel): ...


class DevOther(pydantic.BaseModel): ...


class HostPanic(pydantic.BaseModel): ...


class InvalidArgument(pydantic.BaseModel): ...


class Io(pydantic.BaseModel): ...


class LlmClient(pydantic.BaseModel): ...


class NotImplemented(pydantic.BaseModel): ...


class RenderPrompt(pydantic.BaseModel): ...


class Timeout(pydantic.BaseModel): ...


class Unsupported(pydantic.BaseModel): ...


class StackFrame(pydantic.BaseModel): ...


class StackTrace(pydantic.BaseModel): ...


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
