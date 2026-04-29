from __future__ import annotations

import typing
import pydantic


class Request(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    method: typing.Union[str, None]
    url: typing.Union[str, None]
    headers: typing.Dict[str, str]
    body: typing.Union[str, None]


class Response(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    status_code: typing.Union[int, None]
    headers: typing.Dict[str, str]
    url: typing.Union[str, None]
    _body: None


class SseStream(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    url: typing.Union[str, None]
    _handle: None


__all__ = [
    "Request",
    "Response",
    "SseStream",
]
